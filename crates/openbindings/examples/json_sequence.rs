//! A host-owned whitespace-separated sequence with bounded input buffering.
use openbindings::{JsonLimits, JsonPrefixError, JsonValue};
use std::io::{self, Read};

fn whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn read_more(
    reader: &mut impl Read,
    buffer: &mut Vec<u8>,
    max_bytes: usize,
    minimum: usize,
) -> io::Result<bool> {
    let mut chunk = [0; 8192];
    let start = buffer.len();
    let room = max_bytes.saturating_add(1).saturating_sub(start);
    if room == 0 {
        return Err(io::Error::other(
            "an input value exceeds the admitted byte limit",
        ));
    }
    let required = minimum.max(1).min(room);
    loop {
        let size = max_bytes
            .saturating_add(1)
            .saturating_sub(buffer.len())
            .min(chunk.len());
        match reader.read(&mut chunk[..size]) {
            Ok(0) => return Ok(true),
            Ok(count) => {
                buffer.extend_from_slice(&chunk[..count]);
                if buffer.len() - start >= required {
                    return Ok(false);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
thread_local! {
    static PREFIX_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn read_sequence(
    reader: &mut impl Read,
    limits: JsonLimits,
    mut accept: impl FnMut(JsonValue),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = Vec::new();
    let mut start = 0;
    let mut eof = false;
    let mut needs_separator = false;
    loop {
        let leading = buffer[start..]
            .iter()
            .take_while(|&&byte| whitespace(byte))
            .count();
        if leading != 0 {
            start += leading;
            needs_separator = false;
        }
        if start == buffer.len() {
            if eof {
                return Ok(());
            }
            buffer.clear();
            start = 0;
            eof = read_more(reader, &mut buffer, limits.max_bytes, 1)?;
            continue;
        }
        if needs_separator {
            return Err(io::Error::other("JSON values need whitespace between them").into());
        }
        #[cfg(test)]
        PREFIX_CALLS.with(|count| count.set(count.get() + 1));
        match JsonValue::parse_prefix_with_limits(&buffer[start..], limits) {
            Ok((value, end)) => {
                accept(value);
                // Consume by offset; repeatedly shifting a large remainder
                // for small completed values would itself be quadratic.
                start += end;
                needs_separator = true;
            }
            Err(JsonPrefixError::Incomplete) if eof => {
                accept(JsonValue::parse_with_limits(&buffer[start..], limits)?);
                return Ok(());
            }
            Err(JsonPrefixError::Incomplete) => {
                // Double the retained bytes before retrying, even when Read
                // returns tiny chunks. The sum of reparsed sizes stays linear
                // in one value. EOF and the byte allowance end the read early.
                buffer.drain(..start);
                start = 0;
                let minimum = buffer.len();
                eof = read_more(reader, &mut buffer, limits.max_bytes, minimum)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    read_sequence(&mut io::stdin().lock(), JsonLimits::default(), |value| {
        println!("{}", value.text());
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Chunked<'a> {
        input: &'a [u8],
        size: usize,
    }
    impl Read for Chunked<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let size = self.size.min(output.len()).min(self.input.len());
            output[..size].copy_from_slice(&self.input[..size]);
            self.input = &self.input[size..];
            Ok(size)
        }
    }
    fn sequence(input: &[u8], size: usize) -> (Vec<String>, bool) {
        let mut values = Vec::new();
        let result = read_sequence(
            &mut Chunked { input, size },
            JsonLimits::default(),
            |value| {
                values.push(value.text().to_owned());
            },
        );
        (values, result.is_ok())
    }

    #[test]
    fn host_handles_every_read_size_and_final_number() {
        let input = " \n{} \t [1, \"é😀\"] \"\\ud800\" true false null -12e+3".as_bytes();
        for size in 1..=input.len() {
            let (values, success) = sequence(input, size);
            assert!(success);
            assert_eq!(
                values,
                [
                    "{}",
                    "[1, \"é😀\"]",
                    "\"\\ud800\"",
                    "true",
                    "false",
                    "null",
                    "-12e+3"
                ]
            );
        }
    }

    #[test]
    fn host_owns_separators_empty_sequences_and_later_failures() {
        for input in [b"".as_slice(), b" \r\n\t"] {
            assert_eq!(sequence(input, 1), (Vec::new(), true));
        }
        for size in 1..=4 {
            assert!(!sequence(b"{}[]", size).1);
            assert_eq!(sequence(b"1 \xff", size), (vec!["1".to_owned()], false));
            assert_eq!(sequence(b"1 2e", size), (vec!["1".to_owned()], false));
        }
    }

    #[test]
    fn host_bounds_retained_input_per_value_and_discards_sequence_whitespace() {
        let limits = JsonLimits {
            max_bytes: 2,
            ..JsonLimits::default()
        };
        let mut values = Vec::new();
        read_sequence(&mut b"     12   {}  34".as_slice(), limits, |value| {
            values.push(value)
        })
        .unwrap();
        assert_eq!(values.len(), 3);
        assert!(read_sequence(&mut b"123".as_slice(), limits, |_| {}).is_err());
    }

    #[test]
    fn large_values_from_tiny_reads_need_logarithmically_many_prefix_calls() {
        let mut previous = None;
        for size in [64 * 1024, 1024 * 1024] {
            let input = format!("\"{}\"", "a".repeat(size));
            PREFIX_CALLS.with(|count| count.set(0));
            let (values, success) = sequence(input.as_bytes(), 7);
            assert!(success);
            assert_eq!(values, [input]);
            let calls = PREFIX_CALLS.with(std::cell::Cell::get);
            eprintln!("{size} payload bytes in 7-byte reads: {calls} prefix calls");
            assert!(calls <= size.ilog2() as usize + 2, "{calls} prefix calls");
            if let Some(smaller_calls) = previous {
                assert!(calls <= smaller_calls + 5);
            }
            previous = Some(calls);
        }
    }
}
