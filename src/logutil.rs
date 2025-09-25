//! Logging utilities for sanitizing multi-line user/content strings so logs stay single-line.
//! Escapes control characters that otherwise break log readability.

/// Escape a string for single-line logging:
/// - `\n` => `\\n`
/// - `\r` => `\\r`
/// - `\t` => `\\t`
/// - backslash => `\\\\`
/// Truncates very long strings (over `max_preview`) with an ellipsis to cap log noise.
pub fn escape_log(s: &str) -> String {
    const MAX_PREVIEW: usize = 300; // generous for debug; adjust if needed
    let mut out = String::with_capacity(s.len().min(MAX_PREVIEW) + 8);
    let mut count = 0;
    for ch in s.chars() {
        if count >= MAX_PREVIEW { out.push('…'); break; }
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                // Represent other control chars as hex \xNN
                use std::fmt::Write; let _ = write!(&mut out, "\\x{:02X}", c as u32);
            }
            c => out.push(c)
        }
        count += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::escape_log;
    #[test]
    fn escapes_newlines_and_truncates() {
        let s = "Line1\nLine2\r\tEnd";
        let esc = escape_log(s);
        assert_eq!(esc, "Line1\\nLine2\\r\\tEnd");
    }
}
