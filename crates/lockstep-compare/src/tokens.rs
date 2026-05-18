//! Token-text canonicalization for leaf nodes.
//!
//! Reasons for canonicalizing rather than direct-text-equality:
//!   * `"foo"` and `'foo'` are the same string literal.
//!   * `1.0` and `1` are the same number.
//!   * Tagged templates keep their backticks; nothing to do.

pub fn canonical(kind: &str, text: &str) -> String {
    match kind {
        "string" | "string_fragment" => canonical_string(text),
        "number" => canonical_number(text),
        _ => text.to_string(),
    }
}

fn canonical_string(text: &str) -> String {
    // Strip outer quotes if present, then wrap in canonical double quotes.
    let bytes = text.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[0] == bytes[bytes.len() - 1]
    {
        let inner = &text[1..text.len() - 1];
        format!("\"{}\"", inner.replace('"', "\\\""))
    } else {
        text.to_string()
    }
}

fn canonical_number(text: &str) -> String {
    if let Ok(n) = text.parse::<f64>() {
        // Use Rust's debug-like format that drops trailing zeros for ints.
        if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e16 {
            format!("{}", n as i64)
        } else {
            format!("{}", n)
        }
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_and_double_quoted_strings_equal() {
        assert_eq!(canonical("string", "'foo'"), canonical("string", "\"foo\""));
    }

    #[test]
    fn integer_and_float_form_equal() {
        assert_eq!(canonical("number", "1"), canonical("number", "1.0"));
    }

    #[test]
    fn distinct_numbers_remain_distinct() {
        assert_ne!(canonical("number", "1"), canonical("number", "2"));
    }
}
