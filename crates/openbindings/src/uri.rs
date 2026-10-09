//! RFC 3986 syntax via fluent-uri; literal-dot resolution by §5.2.
//! Deliberately no URI normalization or acquisition-base inference.
use fluent_uri::UriRef;

pub fn absolute(text: &str) -> bool {
    UriRef::parse(text).is_ok_and(|u| u.scheme().is_some())
}
pub fn valid(text: &str) -> bool {
    UriRef::parse(text).is_ok()
}
pub fn fragment(text: &str) -> Option<&str> {
    UriRef::parse(text).ok()?.fragment().map(|f| f.as_str())
}

/// Resolve against an explicitly named base. Inputs must be URI references;
/// `base` must be absolute. Component spelling and defined emptiness survive.
pub fn resolve(base: &str, reference: &str) -> Option<String> {
    let b = UriRef::parse(base).ok()?;
    let r = UriRef::parse(reference).ok()?;
    b.scheme()?;
    let (scheme, authority, path, query) = if let Some(scheme) = r.scheme() {
        (
            scheme.as_str(),
            r.authority().map(|a| a.as_str()),
            remove_dots(r.path().as_str()),
            r.query().map(|q| q.as_str()),
        )
    } else if r.authority().is_some() {
        (
            b.scheme()?.as_str(),
            r.authority().map(|a| a.as_str()),
            remove_dots(r.path().as_str()),
            r.query().map(|q| q.as_str()),
        )
    } else {
        let rp = r.path().as_str();
        let bp = b.path().as_str();
        let (path, query) = if rp.is_empty() {
            (
                bp.to_owned(),
                r.query().or_else(|| b.query()).map(|q| q.as_str()),
            )
        } else if rp.starts_with('/') {
            (remove_dots(rp), r.query().map(|q| q.as_str()))
        } else {
            let prefix = if b.authority().is_some() && bp.is_empty() {
                "/"
            } else {
                &bp[..bp.rfind('/').map_or(0, |p| p + 1)]
            };
            (
                remove_dots(&format!("{prefix}{rp}")),
                r.query().map(|q| q.as_str()),
            )
        };
        (
            b.scheme()?.as_str(),
            b.authority().map(|a| a.as_str()),
            path,
            query,
        )
    };
    let mut out = String::with_capacity(base.len() + reference.len());
    out.push_str(scheme);
    out.push(':');
    if let Some(a) = authority {
        out.push_str("//");
        out.push_str(a);
    }
    out.push_str(&path);
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    if let Some(f) = r.fragment() {
        out.push('#');
        out.push_str(f.as_str());
    }
    Some(out)
}

/// OBI-13 resolves every compared identifier, including absolute identifiers.
/// Remove literal dot segments and an empty fragment, preserving other spelling.
pub fn compared_id(parent: Option<&str>, identifier: &str) -> Option<String> {
    let id = UriRef::parse(identifier).ok()?;
    let result = if id.scheme().is_some() {
        resolve(identifier, identifier)?
    } else {
        resolve(parent?, identifier)?
    };
    Some(result.strip_suffix('#').unwrap_or(&result).to_owned())
}

fn remove_dots(mut input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    while !input.is_empty() {
        if let Some(rest) = input.strip_prefix("../") {
            input = rest;
        } else if let Some(rest) = input.strip_prefix("./") {
            input = rest;
        } else if input.starts_with("/./") {
            input = &input[2..];
        } else if input == "/." {
            input = "/";
        } else if input.starts_with("/../") || input == "/.." {
            input = if input == "/.." { "/" } else { &input[3..] };
            out.truncate(out.rfind('/').unwrap_or(0));
        } else if input == "." || input == ".." {
            input = "";
        } else {
            let end = input[1..].find('/').map_or(input.len(), |i| i + 1);
            out.push_str(&input[..end]);
            input = &input[end..];
        }
    }
    out
}

pub fn decode_fragment(fragment: &str) -> Option<String> {
    let mut out = Vec::with_capacity(fragment.len());
    let src = fragment.as_bytes();
    let mut i = 0;
    while i < src.len() {
        if src[i] == b'%' {
            let hi = char::from(*src.get(i + 1)?).to_digit(16)?;
            let lo = char::from(*src.get(i + 2)?).to_digit(16)?;
            out.push(((hi << 4) | lo) as u8);
            i += 3;
        } else {
            out.push(src[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc_examples() {
        let base = "http://a/b/c/d;p?q";
        for (reference, want) in [
            ("g:h", "g:h"),
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q#s"),
            ("g#s", "http://a/b/c/g#s"),
            ("g?y#s", "http://a/b/c/g?y#s"),
            (";x", "http://a/b/c/;x"),
            ("", "http://a/b/c/d;p?q"),
            (".", "http://a/b/c/"),
            ("./", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../g", "http://a/g"),
            ("../../../g", "http://a/g"),
            ("../../../../g", "http://a/g"),
            ("/./g", "http://a/g"),
            ("/../g", "http://a/g"),
            ("g.", "http://a/b/c/g."),
            (".g", "http://a/b/c/.g"),
            ("g..", "http://a/b/c/g.."),
            ("..g", "http://a/b/c/..g"),
            ("./../g", "http://a/b/g"),
            ("./g/.", "http://a/b/c/g/"),
            ("g/./h", "http://a/b/c/g/h"),
            ("g/../h", "http://a/b/c/h"),
            ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
            ("g;x=1/../y", "http://a/b/c/y"),
            ("g?y/./x", "http://a/b/c/g?y/./x"),
            ("g#s/../x", "http://a/b/c/g#s/../x"),
            ("http:g", "http:g"),
        ] {
            assert_eq!(
                resolve(base, reference).as_deref(),
                Some(want),
                "{reference}"
            );
        }
    }
    #[test]
    fn spelling() {
        assert_eq!(
            resolve("HTTP://Example.TEST/a/b", "%2e%2e/c%2f?x=#").unwrap(),
            "HTTP://Example.TEST/a/%2e%2e/c%2f?x=#"
        );
        assert_eq!(
            compared_id(None, "https://X/a/../b#").unwrap(),
            "https://X/b"
        );
        assert_eq!(resolve("scheme:a", "//").unwrap(), "scheme://");
        assert_eq!(resolve("scheme://", "x?").unwrap(), "scheme:///x?");
        assert_eq!(compared_id(None, "relative"), None);
        for s in [
            "%",
            "x%GG",
            "https://x/é",
            "http://[::g]",
            "1:x",
            "//host:word",
        ] {
            assert!(!valid(s), "{s}");
        }
        assert_eq!(decode_fragment("/%C3%A9~1x"), Some("/é~1x".into()));
        assert_eq!(decode_fragment("%ff"), None);
    }
}
