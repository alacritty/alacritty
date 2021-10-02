use std::time::Duration;

use alacritty_config_derive::ConfigDeserialize;

#[derive(ConfigDeserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub double_click: ClickHandler,
    pub triple_click: ClickHandler,
    pub hide_when_typing: bool,
    #[config(deprecated = "use `hints` section instead")]
    pub url: Option<serde_yaml::Value>,
    pub escape_dropped_path: Option<ShellEscapeMode>,
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClickHandler {
    threshold: u16,
}

impl Default for ClickHandler {
    fn default() -> Self {
        Self { threshold: 300 }
    }
}

impl ClickHandler {
    pub fn threshold(&self) -> Duration {
        Duration::from_millis(self.threshold as u64)
    }
}

#[derive(ConfigDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellEscapeMode {
    SingleQuotes,
    Backslashes,
}

impl ShellEscapeMode {
    pub fn escape(self, input: &str) -> String {
        match self {
            ShellEscapeMode::SingleQuotes => format!("'{}'", input.replace('\'', "'\\''")),
            ShellEscapeMode::Backslashes => input.chars().flat_map(ShellEscapeChar::from).collect(),
        }
    }
}

enum ShellEscapeChar {
    Plain(char),
    Backslash(char),
    Empty,
}

impl From<char> for ShellEscapeChar {
    fn from(c: char) -> Self {
        static NON_ESCAPE_CHARS: &str = ",._+:@%/-";

        if c.is_alphanumeric() || NON_ESCAPE_CHARS.contains(c) {
            ShellEscapeChar::Plain(c)
        } else {
            ShellEscapeChar::Backslash(c)
        }
    }
}

impl Iterator for ShellEscapeChar {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        match *self {
            ShellEscapeChar::Plain(c) => {
                *self = ShellEscapeChar::Empty;
                Some(c)
            },
            ShellEscapeChar::Backslash(c) => {
                *self = ShellEscapeChar::Plain(c);
                Some('\\')
            },
            ShellEscapeChar::Empty => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_quote_escapes() {
        let mode = ShellEscapeMode::SingleQuotes;
        assert_eq!(mode.escape("alphanum42"), "'alphanum42'");
        assert_eq!(mode.escape(r#"foo_+@bar\$baz"&"#), r#"'foo_+@bar\$baz"&'"#);
        assert_eq!(mode.escape("foo'bar"), "'foo'\\''bar'");
    }

    #[test]
    fn test_backslash_escapes() {
        let mode = ShellEscapeMode::Backslashes;
        assert_eq!(mode.escape("alphanum42"), "alphanum42");
        assert_eq!(mode.escape(r#"foo_+@bar\$baz"&"#), r#"foo_+@bar\\\$baz\"\&"#);
        assert_eq!(mode.escape("foo'bar"), "foo\\'bar");
    }
}
