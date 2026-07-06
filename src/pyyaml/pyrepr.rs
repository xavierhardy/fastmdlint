//! Python `repr()` compatible formatting for characters and strings.
//! PyYAML embeds `%r` of characters/strings in its error messages; matching
//! them byte-for-byte requires reproducing CPython's `repr` rules.

fn is_python_printable(ch: char) -> bool {
    // CPython str.isprintable(): not a separator (other than space) and not
    // an "Other" category character. Approximate using Rust's char classes:
    // control, format, surrogate, private use and unassigned are
    // non-printable, as are separators other than ' '.
    if ch == ' ' {
        return true;
    }
    if ch.is_control() {
        return false;
    }
    match ch {
        '\u{200B}'..='\u{200F}' | '\u{2028}' | '\u{2029}' | '\u{202A}'..='\u{202E}' => false,
        '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{205F}' | '\u{3000}' => false,
        '\u{FEFF}' => false,
        '\u{E000}'..='\u{F8FF}' => false, // private use
        _ => !ch.is_whitespace() || !ch.is_ascii(),
    }
}

fn escape_char(ch: char, out: &mut String) {
    match ch {
        '\\' => out.push_str("\\\\"),
        '\t' => out.push_str("\\t"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        _ => {
            if is_python_printable(ch) {
                out.push(ch);
            } else {
                let code = ch as u32;
                if code <= 0xFF {
                    out.push_str(&format!("\\x{code:02x}"));
                } else if code <= 0xFFFF {
                    out.push_str(&format!("\\u{code:04x}"));
                } else {
                    out.push_str(&format!("\\U{code:08x}"));
                }
            }
        }
    }
}

/// Python `repr()` of a string (also used for single characters).
pub fn py_repr(s: &str) -> String {
    // Python uses single quotes unless the string contains a single quote
    // and no double quote.
    let has_single = s.contains('\'');
    let has_double = s.contains('"');
    let quote = if has_single && !has_double { '"' } else { '\'' };

    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        if ch == quote {
            out.push('\\');
            out.push(ch);
        } else {
            escape_char(ch, &mut out);
        }
    }
    out.push(quote);
    out
}

pub fn py_repr_char(ch: char) -> String {
    let mut buf = [0u8; 4];
    py_repr(ch.encode_utf8(&mut buf))
}
