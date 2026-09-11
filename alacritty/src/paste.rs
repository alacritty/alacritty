//! Sanitization for bracketed paste.

use strip_ansi::strip_str;

const FORBIDDEN_RAW_CHARS: [char; 2] = ['\x1b', '\x03'];

/// Strip escape sequences and control characters from pasted text.
pub fn sanitize_paste(text: &str) -> String {
    let without_ansi = strip_str(text);
    without_ansi.replace(FORBIDDEN_RAW_CHARS, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_preserves_plain_text() {
        let input = "the quick brown fox";

        assert_eq!(sanitize_paste(input), input);
    }

    #[test]
    fn paste_strips_escape_sequences() {
        let input = "before\x1b[201~after";

        assert_eq!(sanitize_paste(input), "beforeafter");
    }

    #[test]
    fn paste_strips_etx() {
        let input = "before\x03after";

        assert_eq!(sanitize_paste(input), "beforeafter");
    }

    #[test]
    fn paste_strips_osc52() {
        let input = "before\x1b]52;c;aGVsbG8=\x07after";

        assert_eq!(sanitize_paste(input), "beforeafter");
    }
}
