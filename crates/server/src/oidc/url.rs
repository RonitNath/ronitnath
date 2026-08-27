//! Building the two URLs this module has to build, character by character.
//!
//! There is no URL crate in this workspace and this is not the place to add
//! one: what is needed is percent-encoding of a query value and appending a
//! query to a URI that may already have one. Both are short, both are exactly
//! specified, and both are tested against the cases that matter — a `state`
//! containing an ampersand, a redirect URI that already carries a parameter.

/// Percent-encode a query value.
///
/// The unreserved set of RFC 3986 §2.3 passes; everything else is escaped,
/// including the characters that would otherwise end the value (`&`, `=`,
/// `#`) and the space. A `state` an RP chose is opaque text, and an RP that
/// chose `a&b` gets `a&b` back.
#[must_use]
pub fn encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Append query parameters to a URI, whether or not it already has some.
///
/// A registered redirect URI is matched exactly, so one carrying `?tenant=a`
/// is a legitimate registration and the response's parameters have to join it
/// rather than replace it.
#[must_use]
pub fn with_query(uri: &str, pairs: &[(&str, String)]) -> String {
    let mut out = uri.to_owned();
    let mut separator = if uri.contains('?') { '&' } else { '?' };
    for (name, value) in pairs {
        out.push(separator);
        out.push_str(name);
        out.push('=');
        out.push_str(&encode(value));
        separator = '&';
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_that_could_end_the_query_is_escaped_instead() {
        assert_eq!(encode("plain"), "plain");
        assert_eq!(encode("a&b=c#d"), "a%26b%3Dc%23d");
        assert_eq!(encode("a b"), "a%20b");
        assert_eq!(encode("-_.~"), "-_.~");
        assert_eq!(encode("é"), "%C3%A9", "utf-8, byte by byte");
        assert_eq!(encode(""), "");
    }

    #[test]
    fn parameters_join_a_uri_that_already_has_some() {
        assert_eq!(
            with_query("https://app.test/cb", &[("code", "abc".into())]),
            "https://app.test/cb?code=abc"
        );
        assert_eq!(
            with_query(
                "https://app.test/cb?tenant=a",
                &[("code", "abc".into()), ("state", "x y".into())]
            ),
            "https://app.test/cb?tenant=a&code=abc&state=x%20y"
        );
        assert_eq!(
            with_query("https://app.test/cb", &[]),
            "https://app.test/cb"
        );
    }
}
