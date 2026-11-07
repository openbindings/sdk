//! RFC 3339 §§5.6–5.7 lexical/calendar checks. Leap-second syntax is restricted
//! to a UTC month boundary; no external historical or future leap table is read.
//! Numeric fields are checked byte by byte before arithmetic: packed digit
//! subtraction in the pinned vendor checker accepts some punctuation as digits.

struct Date {
    day: u16,
    month_days: u16,
}
struct Time {
    second: u16,
    utc_minutes: i16,
}

fn digits(bytes: &[u8]) -> Option<u16> {
    bytes.iter().try_fold(0u16, |value, digit| {
        digit
            .is_ascii_digit()
            .then(|| value * 10 + u16::from(digit - b'0'))
    })
}
fn parse_date(bytes: &[u8]) -> Option<Date> {
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = digits(&bytes[..4])?;
    let month = digits(&bytes[5..7])?;
    let day = digits(&bytes[8..10])?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    (day != 0 && day <= month_days).then_some(Date { day, month_days })
}
fn parse_time(bytes: &[u8]) -> Option<Time> {
    if bytes.len() < 9 || bytes[2] != b':' || bytes[5] != b':' {
        return None;
    }
    let hour = digits(&bytes[..2])?;
    let minute = digits(&bytes[3..5])?;
    let second = digits(&bytes[6..8])?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut at = 8;
    if bytes[at] == b'.' {
        at += 1;
        let first = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == first {
            return None;
        }
    }
    let offset = match bytes.get(at)? {
        b'Z' | b'z' if at + 1 == bytes.len() => 0,
        sign @ (b'+' | b'-') if bytes.len() - at == 6 && bytes[at + 3] == b':' => {
            let hours = digits(&bytes[at + 1..at + 3])?;
            let minutes = digits(&bytes[at + 4..at + 6])?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let offset = (hours * 60 + minutes) as i16;
            if *sign == b'+' { offset } else { -offset }
        }
        _ => return None,
    };
    let utc_minutes = (hour * 60 + minute) as i16 - offset;
    if second == 60 && utc_minutes.rem_euclid(1440) != 1439 {
        return None;
    }
    Some(Time {
        second,
        utc_minutes,
    })
}
pub(super) fn date(bytes: &[u8]) -> bool {
    parse_date(bytes).is_some()
}
pub(super) fn time(bytes: &[u8]) -> bool {
    parse_time(bytes).is_some()
}
pub(super) fn date_time(bytes: &[u8]) -> bool {
    if !matches!(bytes.get(10), Some(b'T' | b't')) {
        return false;
    }
    let Some(date) = parse_date(&bytes[..10]) else {
        return false;
    };
    let Some(time) = parse_time(&bytes[11..]) else {
        return false;
    };
    time.second != 60
        || if time.utc_minutes < 0 {
            date.day == 1 // the previous UTC date is the last day of its month
        } else if time.utc_minutes >= 1440 {
            date.day + 1 == date.month_days
        } else {
            date.day == date.month_days
        }
}
