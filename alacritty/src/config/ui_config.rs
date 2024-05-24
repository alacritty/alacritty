use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{self, Formatter};
use std::path::PathBuf;
use std::rc::Rc;

use alacritty_config::SerdeReplace;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::tty::{Options as PtyOptions, Shell};
use log::{error, warn};
use serde::de::{Error as SerdeError, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use unicode_width::UnicodeWidthChar;
use winit::keyboard::{Key, ModifiersState};

use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};
use alacritty_terminal::term::search::RegexSearch;

use crate::config::bell::BellConfig;
use crate::config::bindings::{
    self, Action, Binding, BindingKey, KeyBinding, KeyLocation, ModeWrapper, ModsWrapper,
    MouseBinding,
};
use crate::config::color::Colors;
use crate::config::cursor::Cursor;
use crate::config::debug::Debug;
use crate::config::font::Font;
use crate::config::mouse::{Mouse, MouseBindings};
use crate::config::scrolling::Scrolling;
use crate::config::selection::Selection;
use crate::config::terminal::Terminal;
use crate::config::window::WindowConfig;
use crate::config::LOG_TARGET_CONFIG;

/// Regex used for the default URL hint.
#[rustfmt::skip]
const URL_REGEX: &str = "(ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file:|git://|ssh:|ftp://)\
                         [^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`]+";

#[derive(ConfigDeserialize, Clone, Debug, PartialEq)]
pub struct UiConfig {
    /// Extra environment variables.
    pub env: HashMap<String, String>,

    /// How much scrolling history to keep.
    pub scrolling: Scrolling,

    /// Cursor configuration.
    pub cursor: Cursor,

    /// Selection configuration.
    pub selection: Selection,

    /// Font configuration.
    pub font: Font,

    /// Window configuration.
    pub window: WindowConfig,

    /// Mouse configuration.
    pub mouse: Mouse,

    /// Debug options.
    pub debug: Debug,

    /// Send escape sequences using the alt key.
    #[config(removed = "It's now always set to 'true'. If you're on macOS use \
                        'window.option_as_alt' to alter behavior of Option")]
    pub alt_send_esc: Option<bool>,

    /// Live config reload.
    pub live_config_reload: bool,

    /// Bell configuration.
    pub bell: BellConfig,

    /// RGB values for colors.
    pub colors: Colors,

    /// Path where config was loaded from.
    #[config(skip)]
    pub config_paths: Vec<PathBuf>,

    /// Regex hints for interacting with terminal content.
    pub hints: Hints,

    /// Offer IPC through a unix socket.
    #[cfg(unix)]
    pub ipc_socket: bool,

    /// Config for the alacritty_terminal itself.
    pub terminal: Terminal,

    /// Path to a shell program to run on startup.
    pub shell: Option<Program>,

    /// Shell startup directory.
    pub working_directory: Option<PathBuf>,

    /// Keyboard configuration.
    keyboard: Keyboard,

    /// Should draw bold text with brighter colors instead of bold font.
    #[config(deprecated = "use colors.draw_bold_text_with_bright_colors instead")]
    draw_bold_text_with_bright_colors: bool,

    /// Keybindings.
    #[config(deprecated = "use keyboard.bindings instead")]
    key_bindings: Option<KeyBindings>,

    /// Bindings for the mouse.
    #[config(deprecated = "use mouse.bindings instead")]
    mouse_bindings: Option<MouseBindings>,

    /// Configuration file imports.
    ///
    /// This is never read since the field is directly accessed through the config's
    /// [`toml::Value`], but still present to prevent unused field warnings.
    import: Vec<String>,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            live_config_reload: true,
            #[cfg(unix)]
            ipc_socket: true,
            draw_bold_text_with_bright_colors: Default::default(),
            working_directory: Default::default(),
            mouse_bindings: Default::default(),
            config_paths: Default::default(),
            key_bindings: Default::default(),
            alt_send_esc: Default::default(),
            scrolling: Default::default(),
            selection: Default::default(),
            keyboard: Default::default(),
            terminal: Default::default(),
            import: Default::default(),
            cursor: Default::default(),
            window: Default::default(),
            colors: Default::default(),
            shell: Default::default(),
            mouse: Default::default(),
            debug: Default::default(),
            hints: Default::default(),
            font: Default::default(),
            bell: Default::default(),
            env: Default::default(),
        }
    }
}

impl UiConfig {
    /// Derive [`TermConfig`] from the config.
    pub fn term_options(&self) -> TermConfig {
        TermConfig {
            semantic_escape_chars: self.selection.semantic_escape_chars.clone(),
            scrolling_history: self.scrolling.history() as usize,
            vi_mode_cursor_style: self.cursor.vi_mode_style(),
            default_cursor_style: self.cursor.style(),
            osc52: self.terminal.osc52.0,
            kitty_keyboard: true,
        }
    }

    /// Derive [`PtyOptions`] from the config.
    pub fn pty_config(&self) -> PtyOptions {
        let shell = self.shell.clone().map(Into::into);
        PtyOptions {
            shell,
            working_directory: self.working_directory.clone(),
            hold: false,
            env: HashMap::new(),
        }
    }

    /// Generate key bindings for all keyboard hints.
    pub fn generate_hint_bindings(&mut self) {
        // Check which key bindings is most likely to be the user's configuration.
        //
        // Both will be non-empty due to the presence of the default keybindings.
        let key_bindings = if let Some(key_bindings) = self.key_bindings.as_mut() {
            &mut key_bindings.0
        } else {
            &mut self.keyboard.bindings.0
        };

        for hint in &self.hints.enabled {
            let binding = match &hint.binding {
                Some(binding) => binding,
                None => continue,
            };

            let binding = KeyBinding {
                trigger: binding.key.clone(),
                mods: binding.mods.0,
                mode: binding.mode.mode,
                notmode: binding.mode.not_mode,
                action: Action::Hint(hint.clone()),
            };

            key_bindings.push(binding);
        }
    }

    #[inline]
    pub fn window_opacity(&self) -> f32 {
        self.window.opacity.as_f32()
    }

    #[inline]
    pub fn key_bindings(&self) -> &[KeyBinding] {
        if let Some(key_bindings) = self.key_bindings.as_ref() {
            &key_bindings.0
        } else {
            &self.keyboard.bindings.0
        }
    }

    #[inline]
    pub fn mouse_bindings(&self) -> &[MouseBinding] {
        if let Some(mouse_bindings) = self.mouse_bindings.as_ref() {
            &mouse_bindings.0
        } else {
            &self.mouse.bindings.0
        }
    }

    #[inline]
    pub fn draw_bold_text_with_bright_colors(&self) -> bool {
        self.colors.draw_bold_text_with_bright_colors || self.draw_bold_text_with_bright_colors
    }
}

/// Keyboard configuration.
#[derive(ConfigDeserialize, Default, Clone, Debug, PartialEq)]
struct Keyboard {
    /// Keybindings.
    bindings: KeyBindings,
}

#[derive(SerdeReplace, Clone, Debug, PartialEq, Eq)]
struct KeyBindings(Vec<KeyBinding>);

impl Default for KeyBindings {
    fn default() -> Self {
        Self(bindings::default_key_bindings())
    }
}

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self(deserialize_bindings(deserializer, Self::default().0)?))
    }
}

pub fn deserialize_bindings<'a, D, T>(
    deserializer: D,
    mut default: Vec<Binding<T>>,
) -> Result<Vec<Binding<T>>, D::Error>
where
    D: Deserializer<'a>,
    T: Clone + Eq,
    Binding<T>: Deserialize<'a>,
{
    let values = Vec::<toml::Value>::deserialize(deserializer)?;

    // Skip all invalid values.
    let mut bindings = Vec::with_capacity(values.len());
    for value in values {
        match Binding::<T>::deserialize(value) {
            Ok(binding) => bindings.push(binding),
            Err(err) => {
                error!(target: LOG_TARGET_CONFIG, "Config error: {}; ignoring binding", err);
            },
        }
    }

    // Remove matching default bindings.
    for binding in bindings.iter() {
        default.retain(|b| !b.triggers_match(binding));
    }

    bindings.extend(default);

    Ok(bindings)
}

/// A delta for a point in a 2 dimensional plane.
#[derive(ConfigDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta<T: Default> {
    /// Horizontal change.
    pub x: T,
    /// Vertical change.
    pub y: T,
}

/// Regex terminal hints.
#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Hints {
    /// Characters for the hint labels.
    alphabet: HintsAlphabet,

    /// All configured terminal hints.
    pub enabled: Vec<Hint>,
}

impl Default for Hints {
    fn default() -> Self {
        // Add URL hint by default when no other hint is present.
        let pattern = LazyRegexVariant::Pattern(String::from(URL_REGEX));
        let regex = LazyRegex(Rc::new(RefCell::new(pattern)));
        let content = HintContent::new(Some(regex), true);

        #[cfg(not(any(target_os = "macos", windows)))]
        let action = HintAction::Command(Program::Just(String::from("xdg-open")));
        #[cfg(target_os = "macos")]
        let action = HintAction::Command(Program::Just(String::from("open")));
        #[cfg(windows)]
        let action = HintAction::Command(Program::WithArgs {
            program: String::from("cmd"),
            args: vec!["/c".to_string(), "start".to_string(), "".to_string()],
        });

        Self {
            enabled: vec![Hint {
                content,
                action,
                persist: false,
                post_processing: true,
                mouse: Some(HintMouse { enabled: true, mods: Default::default() }),
                binding: Some(HintBinding {
                    key: BindingKey::Keycode {
                        key: Key::Character("u".into()),
                        location: KeyLocation::Standard,
                    },
                    mods: ModsWrapper(ModifiersState::SHIFT | ModifiersState::CONTROL),
                    mode: Default::default(),
                }),
            }],
            alphabet: Default::default(),
        }
    }
}

impl Hints {
    /// Characters for the hint labels.
    pub fn alphabet(&self) -> &str {
        &self.alphabet.0
    }
}

#[derive(SerdeReplace, Clone, Debug, PartialEq, Eq)]
struct HintsAlphabet(String);

impl Default for HintsAlphabet {
    fn default() -> Self {
        Self(String::from("jfkdls;ahgurieowpq"))
    }
}

impl<'de> Deserialize<'de> for HintsAlphabet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        let mut character_count = 0;
        for character in value.chars() {
            if character.width() != Some(1) {
                return Err(D::Error::custom("characters must be of width 1"));
            }
            character_count += 1;
        }

        if character_count < 2 {
            return Err(D::Error::custom("must include at last 2 characters"));
        }

        Ok(Self(value))
    }
}

/// Built-in actions for hint mode.
#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum HintInternalAction {
    /// Copy the text to the clipboard.
    Copy,
    /// Write the text to the PTY/search.
    Paste,
    /// Select the text matching the hint.
    Select,
    /// Move the vi mode cursor to the beginning of the hint.
    MoveViModeCursor,
}

/// Actions for hint bindings.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum HintAction {
    /// Built-in hint action.
    #[serde(rename = "action")]
    Action(HintInternalAction),

    /// Command the text will be piped to.
    #[serde(rename = "command")]
    Command(Program),
}

/// Hint configuration.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    /// Regex for finding matches.
    #[serde(flatten)]
    pub content: HintContent,

    /// Action executed when this hint is triggered.
    #[serde(flatten)]
    pub action: HintAction,

    /// Hint text post processing.
    #[serde(default)]
    pub post_processing: bool,

    /// Persist hints after selection.
    #[serde(default)]
    pub persist: bool,

    /// Hint mouse highlighting.
    pub mouse: Option<HintMouse>,

    /// Binding required to search for this hint.
    binding: Option<HintBinding>,
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct HintContent {
    /// Regex for finding matches.
    pub regex: Option<LazyRegex>,

    /// Escape sequence hyperlinks.
    pub hyperlinks: bool,
}

impl HintContent {
    pub fn new(regex: Option<LazyRegex>, hyperlinks: bool) -> Self {
        Self { regex, hyperlinks }
    }
}

impl<'de> Deserialize<'de> for HintContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct HintContentVisitor;
        impl<'a> Visitor<'a> for HintContentVisitor {
            type Value = HintContent;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a mapping")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'a>,
            {
                let mut content = Self::Value::default();

                while let Some((key, value)) = map.next_entry::<String, toml::Value>()? {
                    match key.as_str() {
                        "regex" => match Option::<LazyRegex>::deserialize(value) {
                            Ok(regex) => content.regex = regex,
                            Err(err) => {
                                error!(
                                    target: LOG_TARGET_CONFIG,
                                    "Config error: hint's regex: {}", err
                                );
                            },
                        },
                        "hyperlinks" => match bool::deserialize(value) {
                            Ok(hyperlink) => content.hyperlinks = hyperlink,
                            Err(err) => {
                                error!(
                                    target: LOG_TARGET_CONFIG,
                                    "Config error: hint's hyperlinks: {}", err
                                );
                            },
                        },
                        "command" | "action" => (),
                        key => warn!(target: LOG_TARGET_CONFIG, "Unrecognized hint field: {key}"),
                    }
                }

                // Require at least one of hyperlinks or regex trigger hint matches.
                if content.regex.is_none() && !content.hyperlinks {
                    return Err(M::Error::custom(
                        "Config error: At least one of the hint's regex or hint's hyperlinks must \
                         be set",
                    ));
                }

                Ok(content)
            }
        }

        deserializer.deserialize_any(HintContentVisitor)
    }
}

/// Binding for triggering a keyboard hint.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HintBinding {
    pub key: BindingKey,
    #[serde(default)]
    pub mods: ModsWrapper,
    #[serde(default)]
    pub mode: ModeWrapper,
}

/// Hint mouse highlighting.
#[derive(ConfigDeserialize, Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct HintMouse {
    /// Hint mouse highlighting availability.
    pub enabled: bool,

    /// Required mouse modifiers for hint highlighting.
    pub mods: ModsWrapper,
}

/// Lazy regex with interior mutability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LazyRegex(Rc<RefCell<LazyRegexVariant>>);

impl LazyRegex {
    /// Execute a function with the compiled regex DFAs as parameter.
    pub fn with_compiled<T, F>(&self, f: F) -> Option<T>
    where
        F: FnMut(&mut RegexSearch) -> T,
    {
        self.0.borrow_mut().compiled().map(f)
    }
}

impl<'de> Deserialize<'de> for LazyRegex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let regex = LazyRegexVariant::Pattern(String::deserialize(deserializer)?);
        Ok(Self(Rc::new(RefCell::new(regex))))
    }
}

/// Regex which is compiled on demand, to avoid expensive computations at startup.
#[derive(Clone, Debug)]
pub enum LazyRegexVariant {
    Compiled(Box<RegexSearch>),
    Pattern(String),
    Uncompilable,
}

impl LazyRegexVariant {
    /// Get a reference to the compiled regex.
    ///
    /// If the regex is not already compiled, this will compile the DFAs and store them for future
    /// access.
    fn compiled(&mut self) -> Option<&mut RegexSearch> {
        // Check if the regex has already been compiled.
        let regex = match self {
            Self::Compiled(regex_search) => return Some(regex_search),
            Self::Uncompilable => return None,
            Self::Pattern(regex) => regex,
        };

        // Compile the regex.
        let regex_search = match RegexSearch::new(regex) {
            Ok(regex_search) => regex_search,
            Err(err) => {
                error!("could not compile hint regex: {err}");
                *self = Self::Uncompilable;
                return None;
            },
        };
        *self = Self::Compiled(Box::new(regex_search));

        // Return a reference to the compiled DFAs.
        match self {
            Self::Compiled(dfas) => Some(dfas),
            _ => unreachable!(),
        }
    }
}

impl PartialEq for LazyRegexVariant {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Pattern(regex), Self::Pattern(other_regex)) => regex == other_regex,
            _ => false,
        }
    }
}
impl Eq for LazyRegexVariant {}

/// Wrapper around f32 that represents a percentage value between 0.0 and 1.0.
#[derive(SerdeReplace, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Percentage(f32);

impl Default for Percentage {
    fn default() -> Self {
        Percentage(1.0)
    }
}

impl Percentage {
    pub fn new(value: f32) -> Self {
        Percentage(value.clamp(0., 1.))
    }

    pub fn as_f32(self) -> f32 {
        self.0
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
pub enum Program {
    Just(String),
    WithArgs {
        program: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

impl Program {
    pub fn program(&self) -> &str {
        match self {
            Program::Just(program) => program,
            Program::WithArgs { program, .. } => program,
        }
    }

    pub fn args(&self) -> &[String] {
        match self {
            Program::Just(_) => &[],
            Program::WithArgs { args, .. } => args,
        }
    }
}

impl From<Program> for Shell {
    fn from(value: Program) -> Self {
        match value {
            Program::Just(program) => Shell::new(program, Vec::new()),
            Program::WithArgs { program, args } => Shell::new(program, args),
        }
    }
}

impl SerdeReplace for Program {
    fn replace(&mut self, value: toml::Value) -> Result<(), Box<dyn Error>> {
        *self = Self::deserialize(value)?;

        Ok(())
    }
}

pub(crate) struct StringVisitor;
impl<'de> serde::de::Visitor<'de> for StringVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(s.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use alacritty_terminal::term::test::mock_term;

    use crate::display::hint::visible_regex_match_iter;

    #[test]
    fn positive_url_parsing_regex_test() {
        for regular_url in [
            "ipfs:s0mEhAsh",
            "ipns:an0TherHash1234",
            "magnet:?xt=urn:btih:L0UDHA5H12",
            "mailto:example@example.org",
            "gemini://gemini.example.org/",
            "gopher://gopher.example.org",
            "https://www.example.org",
            "http://example.org",
            "news:some.news.portal",
            "file:///C:/Windows/",
            "file:/home/user/whatever",
            "git://github.com/user/repo.git",
            "ssh:git@github.com:user/repo.git",
            "ftp://ftp.example.org",
        ] {
            let term = mock_term(regular_url);
            let mut regex = RegexSearch::new(URL_REGEX).unwrap();
            let matches = visible_regex_match_iter(&term, &mut regex).collect::<Vec<_>>();
            assert_eq!(
                matches.len(),
                1,
                "Should have exactly one match url {regular_url}, but instead got: {matches:?}"
            )
        }
    }

    #[test]
    fn negative_url_parsing_regex_test() {
        for url_like in [
            "http::trace::on_request::log_parameters",
            "http//www.example.org",
            "/user:example.org",
            "mailto: example@example.org",
            "http://<script>alert('xss')</script>",
            "mailto:",
        ] {
            let term = mock_term(url_like);
            let mut regex = RegexSearch::new(URL_REGEX).unwrap();
            let matches = visible_regex_match_iter(&term, &mut regex).collect::<Vec<_>>();
            assert!(
                matches.is_empty(),
                "Should not match url in string {url_like}, but instead got: {matches:?}"
            )
        }
    }
}
