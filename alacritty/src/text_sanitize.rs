//! Sanitization of terminal-controlled text.

use strip_ansi::strip_str;

/// Strip escape sequences from a window title.
pub fn sanitize_title(text: &str) -> String {
    strip_str(text).into_owned()
}

/// Strip escape sequences from an OSC 52 clipboard payload.
pub fn sanitize_clipboard_payload(text: &str) -> String {
    strip_str(text).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_preserves_plain_text() {
        assert_eq!(sanitize_title("my-shell"), "my-shell");
    }

    #[test]
    fn title_strips_ansi() {
        assert_eq!(sanitize_title("\x1b[31mHello\x1b[0m"), "Hello");
    }

    #[test]
    fn clipboard_payload_preserves_plain_text() {
        assert_eq!(sanitize_clipboard_payload("hello world"), "hello world");
    }

    #[test]
    fn clipboard_payload_strips_ansi() {
        assert_eq!(sanitize_clipboard_payload("before\x1b[31mafter"), "beforeafter");
    }
}
