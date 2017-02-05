//! Configuration definitions and file loading
//!
//! Alacritty reads from a config file at startup to determine various runtime
//! parameters including font family and style, font size, etc. In the future,
//! the config file will also hold user and platform specific keybindings.
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::mpsc;
use std::ops::{Index, IndexMut};
use std::fs::File;
use std::borrow::Cow;

use ::Rgb;
use font::Size;
use serde_yaml;
use serde::{self, de};
use serde::de::Error as SerdeError;
use serde::de::{Visitor, MapVisitor, Unexpected};
use notify::{Watcher as WatcherApi, RecommendedWatcher as FileWatcher, op};

use input::{Action, Binding, MouseBinding, KeyBinding};
use index::{Line, Column};

use ansi;

/// Function that returns true for serde default
fn true_bool() -> bool {
    true
}

/// List of indexed colors
///
/// The first 16 entries are the standard ansi named colors. Items 16..232 are
/// the color cube.  Items 233..256 are the grayscale ramp. Finally, item 256 is
/// the configured foreground color, item 257 is the configured background
/// color, item 258 is the cursor foreground color, item 259 is the cursor
//background color.
pub struct ColorList([Rgb; 260]);

impl fmt::Debug for ColorList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("ColorList[..]")
    }
}

impl Default for ColorList {
    fn default() -> ColorList {
        ColorList::from(Colors::default())
    }
}

impl From<Colors> for ColorList {
    fn from(colors: Colors) -> ColorList {
        // Type inference fails without this annotation
        let mut list: ColorList = unsafe { ::std::mem::uninitialized() };

        list.fill_named(&colors);
        list.fill_cube();
        list.fill_gray_ramp();

        list
    }
}

impl ColorList {
    fn fill_named(&mut self, colors: &Colors) {
        // Normals
        self[ansi::NamedColor::Black]   = colors.normal.black;
        self[ansi::NamedColor::Red]     = colors.normal.red;
        self[ansi::NamedColor::Green]   = colors.normal.green;
        self[ansi::NamedColor::Yellow]  = colors.normal.yellow;
        self[ansi::NamedColor::Blue]    = colors.normal.blue;
        self[ansi::NamedColor::Magenta] = colors.normal.magenta;
        self[ansi::NamedColor::Cyan]    = colors.normal.cyan;
        self[ansi::NamedColor::White]   = colors.normal.white;

        // Brights
        self[ansi::NamedColor::BrightBlack]   = colors.bright.black;
        self[ansi::NamedColor::BrightRed]     = colors.bright.red;
        self[ansi::NamedColor::BrightGreen]   = colors.bright.green;
        self[ansi::NamedColor::BrightYellow]  = colors.bright.yellow;
        self[ansi::NamedColor::BrightBlue]    = colors.bright.blue;
        self[ansi::NamedColor::BrightMagenta] = colors.bright.magenta;
        self[ansi::NamedColor::BrightCyan]    = colors.bright.cyan;
        self[ansi::NamedColor::BrightWhite]   = colors.bright.white;

        // Foreground and background
        self[ansi::NamedColor::Foreground] = colors.primary.foreground;
        self[ansi::NamedColor::Background] = colors.primary.background;

        // Foreground and background for custom cursor colors
        self[ansi::NamedColor::CursorForeground] = colors.cursor.foreground;
        self[ansi::NamedColor::CursorBackground] = colors.cursor.background;
    }

    fn fill_cube(&mut self) {
        let mut index = 16;
        // Build colors
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    self[index] = Rgb {
                        r: if r == 0 { 0 } else { r * 40 + 55 },
                        b: if b == 0 { 0 } else { b * 40 + 55 },
                        g: if g == 0 { 0 } else { g * 40 + 55 },
                    };
                    index += 1;
                }
            }
        }

        debug_assert!(index == 232);
    }

    fn fill_gray_ramp(&mut self) {
        let mut index = 232;

        for i in 0..24 {
            let value = i * 10 + 8;
            self[index] = Rgb {
                r: value,
                g: value,
                b: value
            };
            index += 1;
        }

        debug_assert!(index == 256);
    }
}

impl Index<ansi::NamedColor> for ColorList {
    type Output = Rgb;

    #[inline]
    fn index(&self, idx: ansi::NamedColor) -> &Self::Output {
        &self.0[idx as usize]
    }
}

impl IndexMut<ansi::NamedColor> for ColorList {
    #[inline]
    fn index_mut(&mut self, idx: ansi::NamedColor) -> &mut Self::Output {
        &mut self.0[idx as usize]
    }
}

impl Index<usize> for ColorList {
    type Output = Rgb;

    #[inline]
    fn index(&self, idx: usize) -> &Self::Output {
        &self.0[idx]
    }
}

impl Index<u8> for ColorList {
    type Output = Rgb;

    #[inline]
    fn index(&self, idx: u8) -> &Self::Output {
        &self.0[idx as usize]
    }
}

impl IndexMut<usize> for ColorList {
    #[inline]
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.0[idx]
    }
}

#[derive(Debug, Deserialize)]
pub struct Shell<'a> {
    program: Cow<'a, str>,

    #[serde(default)]
    args: Vec<String>,
}

impl<'a> Shell<'a> {
    pub fn new<S>(program: S) -> Shell<'a>
        where S: Into<Cow<'a, str>>
    {
        Shell {
            program: program.into(),
            args: Vec::new(),
        }
    }

    pub fn new_with_args<S>(program: S, args: Vec<String>) -> Shell<'a>
        where S: Into<Cow<'a, str>>
    {
        Shell {
            program: program.into(),
            args: args
        }
    }

    pub fn program(&self) -> &str {
        &*self.program
    }

    pub fn args(&self) -> &[String] {
        self.args.as_slice()
    }
}

/// Top-level config type
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Initial dimensions
    #[serde(default)]
    dimensions: Dimensions,

    /// Pixels per inch
    #[serde(default)]
    dpi: Dpi,

    /// Font configuration
    #[serde(default)]
    font: Font,

    /// Should show render timer
    #[serde(default)]
    render_timer: bool,

    /// Should use custom cursor colors
    #[serde(default)]
    custom_cursor_colors: bool,

    /// Should draw bold text with brighter colors intead of bold font
    #[serde(default="true_bool")]
    draw_bold_text_with_bright_colors: bool,

    #[serde(default)]
    colors: ColorList,

    /// Keybindings
    #[serde(default="default_key_bindings")]
    key_bindings: Vec<KeyBinding>,

    /// Bindings for the mouse
    #[serde(default="default_mouse_bindings")]
    mouse_bindings: Vec<MouseBinding>,

    /// Path to a shell program to run on startup
    #[serde(default)]
    shell: Option<Shell<'static>>,

    /// Path where config was loaded from
    config_path: Option<PathBuf>,
}

#[cfg(not(target_os="macos"))]
static DEFAULT_ALACRITTY_CONFIG: &'static str = include_str!("../alacritty.yml");
#[cfg(target_os="macos")]
static DEFAULT_ALACRITTY_CONFIG: &'static str = include_str!("../alacritty_macos.yml");

fn default_config() -> Config {
    serde_yaml::from_str(DEFAULT_ALACRITTY_CONFIG)
        .expect("default config is valid")
}

fn default_key_bindings() -> Vec<KeyBinding> {
    default_config().key_bindings
}

fn default_mouse_bindings() -> Vec<MouseBinding> {
    default_config().mouse_bindings
}

impl Default for Config {
    fn default() -> Config {
        Config {
            draw_bold_text_with_bright_colors: true,
            dimensions: Default::default(),
            dpi: Default::default(),
            font: Default::default(),
            render_timer: Default::default(),
            custom_cursor_colors: false,
            colors: Default::default(),
            key_bindings: Vec::new(),
            mouse_bindings: Vec::new(),
            shell: None,
            config_path: None,
        }
    }
}

/// Newtype for implementing deserialize on glutin Mods
///
/// Our deserialize impl wouldn't be covered by a derive(Deserialize); see the
/// impl below.
struct ModsWrapper(::glutin::Mods);

impl ModsWrapper {
    fn into_inner(self) -> ::glutin::Mods {
        self.0
    }
}

impl de::Deserialize for ModsWrapper {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer
    {
        struct ModsVisitor;

        impl Visitor for ModsVisitor {
            type Value = ModsWrapper;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("Some subset of Command|Shift|Super|Alt|Option|Control")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<ModsWrapper, E>
                where E: de::Error,
            {
                use ::glutin::{mods, Mods};
                let mut res = Mods::empty();
                for modifier in value.split('|') {
                    match modifier.trim() {
                        "Command" | "Super" => res |= mods::SUPER,
                        "Shift" => res |= mods::SHIFT,
                        "Alt" | "Option" => res |= mods::ALT,
                        "Control" => res |= mods::CONTROL,
                        _ => err_println!("unknown modifier {:?}", modifier),
                    }
                }

                Ok(ModsWrapper(res))
            }
        }

        deserializer.deserialize_str(ModsVisitor)
    }
}

struct ActionWrapper(::input::Action);

impl ActionWrapper {
    fn into_inner(self) -> ::input::Action {
        self.0
    }
}

impl de::Deserialize for ActionWrapper {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer
    {
        struct ActionVisitor;

        impl Visitor for ActionVisitor {
            type Value = ActionWrapper;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("Paste, Copy, PasteSelection, or Quit")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<ActionWrapper, E>
                where E: de::Error,
            {
                Ok(ActionWrapper(match value {
                    "Paste" => Action::Paste,
                    "Copy" => Action::Copy,
                    "PasteSelection" => Action::PasteSelection,
                    "Quit" => Action::Quit,
                    _ => return Err(E::invalid_value(Unexpected::Str(value), &self)),
                }))
            }
        }
        deserializer.deserialize_str(ActionVisitor)
    }
}

use ::term::{mode, TermMode};

struct ModeWrapper {
    pub mode: TermMode,
    pub not_mode: TermMode,
}

impl de::Deserialize for ModeWrapper {
    fn deserialize<D>(deserializer:  D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer
    {
        struct ModeVisitor;

        impl Visitor for ModeVisitor {
            type Value = ModeWrapper;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("Combination of AppCursor | AppKeypad, possibly with negation (~)")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<ModeWrapper, E>
                where E: de::Error,
            {
                let mut res = ModeWrapper {
                    mode: TermMode::empty(),
                    not_mode: TermMode::empty()
                };

                for modifier in value.split('|') {
                    match modifier.trim() {
                        "AppCursor" => res.mode |= mode::APP_CURSOR,
                        "~AppCursor" => res.not_mode |= mode::APP_CURSOR,
                        "AppKeypad" => res.mode |= mode::APP_KEYPAD,
                        "~AppKeypad" => res.not_mode |= mode::APP_KEYPAD,
                        _ => err_println!("unknown omde {:?}", modifier),
                    }
                }

                Ok(res)
            }
        }
        deserializer.deserialize_str(ModeVisitor)
    }
}

struct MouseButton(::glutin::MouseButton);

impl MouseButton {
    fn into_inner(self) -> ::glutin::MouseButton {
        self.0
    }
}

impl de::Deserialize for MouseButton {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer
    {
        struct MouseButtonVisitor;

        impl Visitor for MouseButtonVisitor {
            type Value = MouseButton;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("Left, Right, Middle, or a number")
            }

            fn visit_str<E>(self, value: &str) -> ::std::result::Result<MouseButton, E>
                where E: de::Error,
            {
                match value {
                    "Left" => Ok(MouseButton(::glutin::MouseButton::Left)),
                    "Right" => Ok(MouseButton(::glutin::MouseButton::Right)),
                    "Middle" => Ok(MouseButton(::glutin::MouseButton::Middle)),
                    _ => {
                        if let Ok(index) = u8::from_str(value) {
                            Ok(MouseButton(::glutin::MouseButton::Other(index)))
                        } else {
                            Err(E::invalid_value(Unexpected::Str(value), &self))
                        }
                    }
                }
            }
        }

        deserializer.deserialize_str(MouseButtonVisitor)
    }
}

/// Bindings are deserialized into a `RawBinding` before being parsed as a
/// `KeyBinding` or `MouseBinding`.
struct RawBinding {
    key: Option<::glutin::VirtualKeyCode>,
    mouse: Option<::glutin::MouseButton>,
    mods: ::glutin::Mods,
    mode: TermMode,
    notmode: TermMode,
    action: Action,
}

impl RawBinding {
    fn into_mouse_binding(self) -> ::std::result::Result<MouseBinding, Self> {
        if self.mouse.is_some() {
            Ok(Binding {
                trigger: self.mouse.unwrap(),
                mods: self.mods,
                action: self.action,
                mode: self.mode,
                notmode: self.notmode,
            })
        } else {
            Err(self)
        }
    }

    fn into_key_binding(self) -> ::std::result::Result<KeyBinding, Self> {
        if self.key.is_some() {
            Ok(KeyBinding {
                trigger: self.key.unwrap(),
                mods: self.mods,
                action: self.action,
                mode: self.mode,
                notmode: self.notmode,
            })
        } else {
            Err(self)
        }
    }
}

impl de::Deserialize for RawBinding {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer
    {
        enum Field {
            Key,
            Mods,
            Mode,
            Action,
            Chars,
            Mouse
        }

        impl de::Deserialize for Field {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Field, D::Error>
                where D: de::Deserializer
            {
                struct FieldVisitor;

                static FIELDS: &'static [&'static str] = &[
                        "key", "mods", "mode", "action", "chars", "mouse"
                ];

                impl Visitor for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        f.write_str("binding fields")
                    }

                    fn visit_str<E>(self, value: &str) -> ::std::result::Result<Field, E>
                        where E: de::Error,
                    {
                        match value {
                            "key" => Ok(Field::Key),
                            "mods" => Ok(Field::Mods),
                            "mode" => Ok(Field::Mode),
                            "action" => Ok(Field::Action),
                            "chars" => Ok(Field::Chars),
                            "mouse" => Ok(Field::Mouse),
                            _ => Err(E::unknown_field(value, FIELDS)),
                        }
                    }
                }

                deserializer.deserialize_struct_field(FieldVisitor)
            }
        }

        struct RawBindingVisitor;
        impl Visitor for RawBindingVisitor {
            type Value = RawBinding;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("binding specification")
            }

            fn visit_map<V>(
                self,
                mut visitor: V
            ) -> ::std::result::Result<RawBinding, V::Error>
                where V: MapVisitor,
            {
                let mut mods: Option<::glutin::Mods> = None;
                let mut key: Option<::glutin::VirtualKeyCode> = None;
                let mut chars: Option<String> = None;
                let mut action: Option<::input::Action> = None;
                let mut mode: Option<TermMode> = None;
                let mut not_mode: Option<TermMode> = None;
                let mut mouse: Option<::glutin::MouseButton> = None;

                use ::serde::de::Error;

                while let Some(struct_key) = visitor.visit_key::<Field>()? {
                    match struct_key {
                        Field::Key => {
                            if key.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("key"));
                            }

                            let coherent_key = visitor.visit_value::<Key>()?;
                            key = Some(coherent_key.to_glutin_key());
                        },
                        Field::Mods => {
                            if mods.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("mods"));
                            }

                            mods = Some(visitor.visit_value::<ModsWrapper>()?.into_inner());
                        },
                        Field::Mode => {
                            if mode.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("mode"));
                            }

                            let mode_deserializer = visitor.visit_value::<ModeWrapper>()?;
                            mode = Some(mode_deserializer.mode);
                            not_mode = Some(mode_deserializer.not_mode);
                        },
                        Field::Action => {
                            if action.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("action"));
                            }

                            action = Some(visitor.visit_value::<ActionWrapper>()?.into_inner());
                        },
                        Field::Chars => {
                            if chars.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("chars"));
                            }

                            chars = Some(visitor.visit_value()?);
                        },
                        Field::Mouse => {
                            if chars.is_some() {
                                return Err(<V::Error as Error>::duplicate_field("mouse"));
                            }

                            mouse = Some(visitor.visit_value::<MouseButton>()?.into_inner());
                        }
                    }
                }

                let action = match (action, chars) {
                    (Some(_), Some(_)) => {
                        return Err(V::Error::custom("must specify only chars or action"));
                    },
                    (Some(action), _) => action,
                    (_, Some(chars)) => Action::Esc(chars),
                    _ => return Err(V::Error::custom("must specify chars or action"))
                };

                let mode = mode.unwrap_or_else(TermMode::empty);
                let not_mode = not_mode.unwrap_or_else(TermMode::empty);
                let mods = mods.unwrap_or_else(::glutin::Mods::empty);

                if mouse.is_none() && key.is_none() {
                    return Err(V::Error::custom("bindings require mouse button or key"));
                }

                Ok(RawBinding {
                    mode: mode,
                    notmode: not_mode,
                    action: action,
                    key: key,
                    mouse: mouse,
                    mods: mods,
                })
            }
        }

        const FIELDS: &'static [&'static str] = &[
            "key", "mods", "mode", "action", "chars", "mouse"
        ];

        deserializer.deserialize_struct("RawBinding", FIELDS, RawBindingVisitor)
    }
}

impl de::Deserialize for ColorList {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer
    {
        let named_colors = Colors::deserialize(deserializer)?;
        Ok(ColorList::from(named_colors))
    }
}

impl de::Deserialize for MouseBinding {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer
    {
        let raw = RawBinding::deserialize(deserializer)?;
        raw.into_mouse_binding()
           .map_err(|_| D::Error::custom("expected mouse binding"))
    }
}

impl de::Deserialize for KeyBinding {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: de::Deserializer
    {
        let raw = RawBinding::deserialize(deserializer)?;
        raw.into_key_binding()
           .map_err(|_| D::Error::custom("expected key binding"))
    }
}

/// Errors occurring during config loading
#[derive(Debug)]
pub enum Error {
    /// Config file not found
    NotFound,

    /// Couldn't read $HOME environment variable
    ReadingEnvHome(env::VarError),

    /// io error reading file
    Io(io::Error),

    /// Not valid yaml or missing parameters
    Yaml(serde_yaml::Error),
}

#[derive(Debug, Deserialize)]
pub struct Colors {
    primary: PrimaryColors,
    #[serde(default="default_cursor_colors")]
    cursor: PrimaryColors,
    normal: AnsiColors,
    bright: AnsiColors,
}

fn default_cursor_colors() -> PrimaryColors {
    PrimaryColors {
        foreground: Rgb { r: 0, g: 0, b: 0 },
        background: Rgb { r: 0xff, g: 0xff, b: 0xff },
    }
}

#[derive(Debug, Deserialize)]
pub struct PrimaryColors {
    #[serde(deserialize_with = "rgb_from_hex")]
    background: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    foreground: Rgb,
}

impl Default for Colors {
    fn default() -> Colors {
        Colors {
            primary: PrimaryColors {
                background: Rgb { r: 0, g: 0, b: 0 },
                foreground: Rgb { r: 0xea, g: 0xea, b: 0xea },
            },
            cursor: default_cursor_colors(),
            normal: AnsiColors {
                black: Rgb {r: 0x00, g: 0x00, b: 0x00},
                red: Rgb {r: 0xd5, g: 0x4e, b: 0x53},
                green: Rgb {r: 0xb9, g: 0xca, b: 0x4a},
                yellow: Rgb {r: 0xe6, g: 0xc5, b: 0x47},
                blue: Rgb {r: 0x7a, g: 0xa6, b: 0xda},
                magenta: Rgb {r: 0xc3, g: 0x97, b: 0xd8},
                cyan: Rgb {r: 0x70, g: 0xc0, b: 0xba},
                white: Rgb {r: 0x42, g: 0x42, b: 0x42},
            },
            bright: AnsiColors {
                black: Rgb {r: 0x66, g: 0x66, b: 0x66},
                red: Rgb {r: 0xff, g: 0x33, b: 0x34},
                green: Rgb {r: 0x9e, g: 0xc4, b: 0x00},
                yellow: Rgb {r: 0xe7, g: 0xc5, b: 0x47},
                blue: Rgb {r: 0x7a, g: 0xa6, b: 0xda},
                magenta: Rgb {r: 0xb7, g: 0x7e, b: 0xe0},
                cyan: Rgb {r: 0x54, g: 0xce, b: 0xd6},
                white: Rgb {r: 0x2a, g: 0x2a, b: 0x2a},
            }
        }
    }
}

/// The normal or bright colors section of config
#[derive(Debug, Deserialize)]
pub struct AnsiColors {
    #[serde(deserialize_with = "rgb_from_hex")]
    black: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    red: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    green: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    yellow: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    blue: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    magenta: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    cyan: Rgb,
    #[serde(deserialize_with = "rgb_from_hex")]
    white: Rgb,
}

/// Deserialize an Rgb from a hex string
///
/// This is *not* the deserialize impl for Rgb since we want a symmetric
/// serialize/deserialize impl for ref tests.
fn rgb_from_hex<D>(deserializer: D) -> ::std::result::Result<Rgb, D::Error>
    where D: de::Deserializer
{
    struct RgbVisitor;

    impl ::serde::de::Visitor for RgbVisitor {
        type Value = Rgb;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("Hex colors spec like 'ffaabb'")
        }

        fn visit_str<E>(self, value: &str) -> ::std::result::Result<Rgb, E>
            where E: ::serde::de::Error
        {
            Rgb::from_str(&value[..])
                .map_err(|_| E::custom("failed to parse rgb; expect 0xrrggbb"))
        }
    }

    deserializer.deserialize_str(RgbVisitor)
}

impl FromStr for Rgb {
    type Err = ();
    fn from_str(s: &str) -> ::std::result::Result<Rgb, ()> {
        let mut chars = s.chars();
        let mut rgb = Rgb::default();

        macro_rules! component {
            ($($c:ident),*) => {
                $(
                    match chars.next().unwrap().to_digit(16) {
                        Some(val) => rgb.$c = (val as u8) << 4,
                        None => return Err(())
                    }

                    match chars.next().unwrap().to_digit(16) {
                        Some(val) => rgb.$c |= val as u8,
                        None => return Err(())
                    }
                )*
            }
        }

        if chars.next().unwrap() != '0' { return Err(()); }
        if chars.next().unwrap() != 'x' { return Err(()); }

        component!(r, g, b);

        Ok(rgb)
    }
}

impl ::std::error::Error for Error {
    fn cause(&self) -> Option<&::std::error::Error> {
        match *self {
            Error::NotFound => None,
            Error::ReadingEnvHome(ref err) => Some(err),
            Error::Io(ref err) => Some(err),
            Error::Yaml(ref err) => Some(err),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::NotFound => "could not locate config file",
            Error::ReadingEnvHome(ref err) => err.description(),
            Error::Io(ref err) => err.description(),
            Error::Yaml(ref err) => err.description(),
        }
    }
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Error::NotFound => write!(f, "{}", ::std::error::Error::description(self)),
            Error::ReadingEnvHome(ref err) => {
                write!(f, "could not read $HOME environment variable: {}", err)
            },
            Error::Io(ref err) => write!(f, "error reading config file: {}", err),
            Error::Yaml(ref err) => write!(f, "problem with config: {:?}", err),
        }
    }
}

impl From<env::VarError> for Error {
    fn from(val: env::VarError) -> Error {
        Error::ReadingEnvHome(val)
    }
}

impl From<io::Error> for Error {
    fn from(val: io::Error) -> Error {
        if val.kind() == io::ErrorKind::NotFound {
            Error::NotFound
        } else {
            Error::Io(val)
        }
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(val: serde_yaml::Error) -> Error {
        Error::Yaml(val)
    }
}

/// Result from config loading
pub type Result<T> = ::std::result::Result<T, Error>;

impl Config {
    /// Attempt to load the config file
    ///
    /// The config file is loaded from the first file it finds in this list of paths
    ///
    /// 1. $XDG_CONFIG_HOME/alacritty/alacritty.yml
    /// 2. $XDG_CONFIG_HOME/alacritty.yml
    /// 3. $HOME/.config/alacritty/alacritty.yml
    /// 4. $HOME/.alacritty.yml
    pub fn load() -> Result<Config> {
        let home = env::var("HOME")?;

        // Try using XDG location by default
        let path = ::xdg::BaseDirectories::with_prefix("alacritty")
            .ok()
            .and_then(|xdg| xdg.find_config_file("alacritty.yml"))
            .or_else(|| {
                ::xdg::BaseDirectories::new().ok().and_then(|fallback| {
                    fallback.find_config_file("alacritty.yml")
                })
            })
            .or_else(|| {
                // Fallback path: $HOME/.config/alacritty/alacritty.yml
                let fallback = PathBuf::from(&home).join(".config/alacritty/alacritty.yml");
                match fallback.exists() {
                    true => Some(fallback),
                    false => None
                }
            })
            .unwrap_or_else(|| {
                // Fallback path: $HOME/.alacritty.yml
                PathBuf::from(&home).join(".alacritty.yml")
            });

        Config::load_from(path)
    }

    pub fn write_defaults() -> io::Result<PathBuf> {
        let path = ::xdg::BaseDirectories::with_prefix("alacritty")
            .map_err(|err| io::Error::new(io::ErrorKind::NotFound, ::std::error::Error::description(&err)))
            .and_then(|p| p.place_config_file("alacritty.yml"))?;
        File::create(&path)?.write_all(DEFAULT_ALACRITTY_CONFIG.as_bytes())?;
        Ok(path)
    }

    /// Get list of colors
    ///
    /// The ordering returned here is expected by the terminal. Colors are simply indexed in this
    /// array for performance.
    pub fn color_list(&self) -> &ColorList {
        &self.colors
    }

    pub fn key_bindings(&self) -> &[KeyBinding] {
        &self.key_bindings[..]
    }

    pub fn mouse_bindings(&self) -> &[MouseBinding] {
        &self.mouse_bindings[..]
    }

    #[inline]
    pub fn draw_bold_text_with_bright_colors(&self) -> bool {
        self.draw_bold_text_with_bright_colors
    }

    /// Get font config
    #[inline]
    pub fn font(&self) -> &Font {
        &self.font
    }

    /// Get window dimensions
    #[inline]
    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    /// Get dpi config
    #[inline]
    pub fn dpi(&self) -> &Dpi {
        &self.dpi
    }

    /// Should show render timer
    #[inline]
    pub fn render_timer(&self) -> bool {
        self.render_timer
    }

    #[inline]
    pub fn use_thin_strokes(&self) -> bool {
        self.font.use_thin_strokes
    }

    /// show cursor as inverted
    #[inline]
    pub fn custom_cursor_colors(&self) -> bool {
        self.custom_cursor_colors
    }

    pub fn path(&self) -> Option<&Path> {
        self.config_path
            .as_ref()
            .map(|p| p.as_path())
    }

    pub fn shell(&self) -> Option<&Shell> {
        self.shell.as_ref()
    }

    fn load_from<P: Into<PathBuf>>(path: P) -> Result<Config> {
        let path = path.into();
        let raw = Config::read_file(path.as_path())?;
        let mut config: Config = serde_yaml::from_str(&raw)?;
        config.config_path = Some(path);

        Ok(config)
    }

    fn read_file<P: AsRef<Path>>(path: P) -> Result<String> {
        let mut f = fs::File::open(path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;

        Ok(contents)
    }
}

/// Window Dimensions
///
/// Newtype to avoid passing values incorrectly
#[derive(Debug, Copy, Clone, Deserialize)]
pub struct Dimensions {
    /// Window width in character columns
    columns: Column,

    /// Window Height in character lines
    lines: Line,
}

impl Default for Dimensions {
    fn default() -> Dimensions {
        Dimensions::new(Column(80), Line(24))
    }
}

impl Dimensions {
    pub fn new(columns: Column, lines: Line) -> Self {
        Dimensions {
            columns: columns,
            lines: lines
        }
    }

    /// Get lines
    #[inline]
    pub fn lines_u32(&self) -> u32 {
        self.lines.0 as u32
    }

    /// Get columns
    #[inline]
    pub fn columns_u32(&self) -> u32 {
        self.columns.0 as u32
    }
}

/// Pixels per inch
///
/// This is only used on `FreeType` systems
#[derive(Debug, Deserialize)]
pub struct Dpi {
    /// Horizontal dpi
    x: f32,

    /// Vertical dpi
    y: f32,
}

impl Default for Dpi {
    fn default() -> Dpi {
        Dpi { x: 96.0, y: 96.0 }
    }
}

impl Dpi {
    /// Get horizontal dpi
    #[inline]
    pub fn x(&self) -> f32 {
        self.x
    }

    /// Get vertical dpi
    #[inline]
    pub fn y(&self) -> f32 {
        self.y
    }
}

/// Modifications to font spacing
///
/// The way Alacritty calculates vertical and horizontal cell sizes may not be
/// ideal for all fonts. This gives the user a way to tweak those values.
#[derive(Debug, Deserialize)]
pub struct FontOffset {
    /// Extra horizontal spacing between letters
    x: f32,
    /// Extra vertical spacing between lines
    y: f32,
}

impl FontOffset {
    /// Get letter spacing
    #[inline]
    pub fn x(&self) -> f32 {
        self.x
    }

    /// Get line spacing
    #[inline]
    pub fn y(&self) -> f32 {
        self.y
    }
}

trait DeserializeFromF32 : Sized {
    fn deserialize_from_f32<D>(D) -> ::std::result::Result<Self, D::Error>
        where D: serde::de::Deserializer;
}

impl DeserializeFromF32 for Size {
    fn deserialize_from_f32<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where D: serde::de::Deserializer
    {
        use std::marker::PhantomData;

        struct FloatVisitor<__D> {
            _marker: PhantomData<__D>,
        }

        impl<__D> ::serde::de::Visitor for FloatVisitor<__D>
            where __D: ::serde::de::Deserializer
        {
            type Value = f64;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("f64")
            }

            fn visit_f64<E>(self, value: f64) -> ::std::result::Result<Self::Value, E>
                where E: ::serde::de::Error
            {
                Ok(value)
            }
        }

        deserializer
            .deserialize_f64(FloatVisitor::<D>{ _marker: PhantomData })
            .map(|v| Size::new(v as _))
    }
}

/// Font config
///
/// Defaults are provided at the level of this struct per platform, but not per
/// field in this struct. It might be nice in the future to have defaults for
/// each value independently. Alternatively, maybe erroring when the user
/// doesn't provide complete config is Ok.
#[derive(Debug, Deserialize)]
pub struct Font {
    /// Font family
    pub normal: FontDescription,

    #[serde(default="default_italic_desc")]
    pub italic: FontDescription,

    #[serde(default="default_bold_desc")]
    pub bold: FontDescription,

    // Font size in points
    #[serde(deserialize_with="DeserializeFromF32::deserialize_from_f32")]
    size: Size,

    /// Extra spacing per character
    offset: FontOffset,

    #[serde(default="true_bool")]
    use_thin_strokes: bool
}

fn default_bold_desc() -> FontDescription {
    Font::default().bold
}

fn default_italic_desc() -> FontDescription {
    Font::default().italic
}

/// Description of a single font
#[derive(Debug, Deserialize)]
pub struct FontDescription {
    pub family: String,
    pub style: Option<String>,
}

impl FontDescription {
    fn new_with_family<S: Into<String>>(family: S) -> FontDescription {
        FontDescription {
            family: family.into(),
            style: None,
        }
    }
}

impl Font {
    /// Get the font size in points
    #[inline]
    pub fn size(&self) -> Size {
        self.size
    }

    /// Get offsets to font metrics
    #[inline]
    pub fn offset(&self) -> &FontOffset {
        &self.offset
    }
}

#[cfg(target_os = "macos")]
impl Default for Font {
    fn default() -> Font {
        Font {
            normal: FontDescription::new_with_family("Menlo"),
            bold: FontDescription::new_with_family("Menlo"),
            italic: FontDescription::new_with_family("Menlo"),
            size: Size::new(11.0),
            use_thin_strokes: true,
            offset: FontOffset {
                x: 0.0,
                y: 0.0
            }
        }
    }
}

#[cfg(any(target_os = "linux",target_os = "freebsd"))]
impl Default for Font {
    fn default() -> Font {
        Font {
            normal: FontDescription::new_with_family("monospace"),
            bold: FontDescription::new_with_family("monospace"),
            italic: FontDescription::new_with_family("monospace"),
            size: Size::new(11.0),
            use_thin_strokes: false,
            offset: FontOffset {
                // TODO should improve freetype metrics... shouldn't need such
                // drastic offsets for the default!
                x: 2.0,
                y: -7.0
            }
        }
    }
}

pub struct Monitor {
    _thread: ::std::thread::JoinHandle<()>,
    rx: mpsc::Receiver<Config>,
}

pub trait OnConfigReload {
    fn on_config_reload(&mut self);
}

impl OnConfigReload for ::display::Notifier {
    fn on_config_reload(&mut self) {
        self.notify();
    }
}

impl Monitor {
    /// Get pending config changes
    pub fn pending_config(&self) -> Option<Config> {
        let mut config = None;
        while let Ok(new) = self.rx.try_recv() {
            config = Some(new);
        }

        config
    }
    pub fn new<H, P>(path: P, mut handler: H) -> Monitor
        where H: OnConfigReload + Send + 'static,
              P: Into<PathBuf>
    {
        let path = path.into();

        let (config_tx, config_rx) = mpsc::channel();

        Monitor {
            _thread: ::util::thread::spawn_named("config watcher", move || {
                let (tx, rx) = mpsc::channel();
                let mut watcher = FileWatcher::new(tx).unwrap();
                watcher.watch(&path).expect("watch alacritty yml");

                let config_path = path.as_path();

                loop {
                    let event = rx.recv().expect("watcher event");
                    let ::notify::Event { path, op } = event;

                    if let Ok(op) = op {
                        // Skip events that are just a rename
                        if op.contains(op::RENAME) && !op.contains(op::WRITE) {
                            continue;
                        }

                        // Need to handle ignore for linux
                        if op.contains(op::IGNORED) {
                            if let Some(path) = path.as_ref() {
                                if let Err(err) = watcher.watch(&path) {
                                    err_println!("failed to establish watch on {:?}: {:?}",
                                                 path, err);
                                }
                            }
                        }

                        // Reload file
                        path.map(|path| {
                            if path == config_path {
                                match Config::load() {
                                    Ok(config) => {
                                        let _ = config_tx.send(config);
                                        handler.on_config_reload();
                                    },
                                    Err(err) => err_println!("Ignoring invalid config: {}", err),
                                }
                            }
                        });
                    }
                }
            }),
            rx: config_rx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    static ALACRITTY_YML: &'static str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/alacritty.yml"));

    #[test]
    fn parse_config() {
        let config: Config = ::serde_yaml::from_str(ALACRITTY_YML)
            .expect("deserialize config");

        // Sanity check that mouse bindings are being parsed
        assert!(config.mouse_bindings.len() >= 1);

        // Sanity check that key bindings are being parsed
        assert!(config.key_bindings.len() >= 1);
    }

    #[test]
    fn defaults_are_ok() {
        super::default_key_bindings();
        super::default_mouse_bindings();
    }
}

#[cfg_attr(feature = "clippy", allow(enum_variant_names))]
#[derive(Deserialize, Copy, Clone)]
enum Key {
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,

    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    Escape,

    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,

    Snapshot,
    Scroll,
    Pause,
    Insert,
    Home,
    Delete,
    End,
    PageDown,
    PageUp,

    Left,
    Up,
    Right,
    Down,
    Back,
    Return,
    Space,
    Compose,
    Numlock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,

    AbntC1,
    AbntC2,
    Add,
    Apostrophe,
    Apps,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
    Decimal,
    Divide,
    Equals,
    Grave,
    Kana,
    Kanji,
    LAlt,
    LBracket,
    LControl,
    LMenu,
    LShift,
    LWin,
    Mail,
    MediaSelect,
    MediaStop,
    Minus,
    Multiply,
    Mute,
    MyComputer,
    NavigateForward,
    NavigateBackward,
    NextTrack,
    NoConvert,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    OEM102,
    Period,
    PlayPause,
    Power,
    PrevTrack,
    RAlt,
    RBracket,
    RControl,
    RMenu,
    RShift,
    RWin,
    Semicolon,
    Slash,
    Sleep,
    Stop,
    Subtract,
    Sysrq,
    Tab,
    Underline,
    Unlabeled,
    VolumeDown,
    VolumeUp,
    Wake,
    WebBack,
    WebFavorites,
    WebForward,
    WebHome,
    WebRefresh,
    WebSearch,
    WebStop,
    Yen,
}

impl Key {
    fn to_glutin_key(&self) -> ::glutin::VirtualKeyCode {
        // Thank you, vim macros!
        match *self {
            Key::Key1 => ::glutin::VirtualKeyCode::Key1,
            Key::Key2 => ::glutin::VirtualKeyCode::Key2,
            Key::Key3 => ::glutin::VirtualKeyCode::Key3,
            Key::Key4 => ::glutin::VirtualKeyCode::Key4,
            Key::Key5 => ::glutin::VirtualKeyCode::Key5,
            Key::Key6 => ::glutin::VirtualKeyCode::Key6,
            Key::Key7 => ::glutin::VirtualKeyCode::Key7,
            Key::Key8 => ::glutin::VirtualKeyCode::Key8,
            Key::Key9 => ::glutin::VirtualKeyCode::Key9,
            Key::Key0 => ::glutin::VirtualKeyCode::Key0,
            Key::A => ::glutin::VirtualKeyCode::A,
            Key::B => ::glutin::VirtualKeyCode::B,
            Key::C => ::glutin::VirtualKeyCode::C,
            Key::D => ::glutin::VirtualKeyCode::D,
            Key::E => ::glutin::VirtualKeyCode::E,
            Key::F => ::glutin::VirtualKeyCode::F,
            Key::G => ::glutin::VirtualKeyCode::G,
            Key::H => ::glutin::VirtualKeyCode::H,
            Key::I => ::glutin::VirtualKeyCode::I,
            Key::J => ::glutin::VirtualKeyCode::J,
            Key::K => ::glutin::VirtualKeyCode::K,
            Key::L => ::glutin::VirtualKeyCode::L,
            Key::M => ::glutin::VirtualKeyCode::M,
            Key::N => ::glutin::VirtualKeyCode::N,
            Key::O => ::glutin::VirtualKeyCode::O,
            Key::P => ::glutin::VirtualKeyCode::P,
            Key::Q => ::glutin::VirtualKeyCode::Q,
            Key::R => ::glutin::VirtualKeyCode::R,
            Key::S => ::glutin::VirtualKeyCode::S,
            Key::T => ::glutin::VirtualKeyCode::T,
            Key::U => ::glutin::VirtualKeyCode::U,
            Key::V => ::glutin::VirtualKeyCode::V,
            Key::W => ::glutin::VirtualKeyCode::W,
            Key::X => ::glutin::VirtualKeyCode::X,
            Key::Y => ::glutin::VirtualKeyCode::Y,
            Key::Z => ::glutin::VirtualKeyCode::Z,
            Key::Escape => ::glutin::VirtualKeyCode::Escape,
            Key::F1 => ::glutin::VirtualKeyCode::F1,
            Key::F2 => ::glutin::VirtualKeyCode::F2,
            Key::F3 => ::glutin::VirtualKeyCode::F3,
            Key::F4 => ::glutin::VirtualKeyCode::F4,
            Key::F5 => ::glutin::VirtualKeyCode::F5,
            Key::F6 => ::glutin::VirtualKeyCode::F6,
            Key::F7 => ::glutin::VirtualKeyCode::F7,
            Key::F8 => ::glutin::VirtualKeyCode::F8,
            Key::F9 => ::glutin::VirtualKeyCode::F9,
            Key::F10 => ::glutin::VirtualKeyCode::F10,
            Key::F11 => ::glutin::VirtualKeyCode::F11,
            Key::F12 => ::glutin::VirtualKeyCode::F12,
            Key::F13 => ::glutin::VirtualKeyCode::F13,
            Key::F14 => ::glutin::VirtualKeyCode::F14,
            Key::F15 => ::glutin::VirtualKeyCode::F15,
            Key::Snapshot => ::glutin::VirtualKeyCode::Snapshot,
            Key::Scroll => ::glutin::VirtualKeyCode::Scroll,
            Key::Pause => ::glutin::VirtualKeyCode::Pause,
            Key::Insert => ::glutin::VirtualKeyCode::Insert,
            Key::Home => ::glutin::VirtualKeyCode::Home,
            Key::Delete => ::glutin::VirtualKeyCode::Delete,
            Key::End => ::glutin::VirtualKeyCode::End,
            Key::PageDown => ::glutin::VirtualKeyCode::PageDown,
            Key::PageUp => ::glutin::VirtualKeyCode::PageUp,
            Key::Left => ::glutin::VirtualKeyCode::Left,
            Key::Up => ::glutin::VirtualKeyCode::Up,
            Key::Right => ::glutin::VirtualKeyCode::Right,
            Key::Down => ::glutin::VirtualKeyCode::Down,
            Key::Back => ::glutin::VirtualKeyCode::Back,
            Key::Return => ::glutin::VirtualKeyCode::Return,
            Key::Space => ::glutin::VirtualKeyCode::Space,
            Key::Compose => ::glutin::VirtualKeyCode::Compose,
            Key::Numlock => ::glutin::VirtualKeyCode::Numlock,
            Key::Numpad0 => ::glutin::VirtualKeyCode::Numpad0,
            Key::Numpad1 => ::glutin::VirtualKeyCode::Numpad1,
            Key::Numpad2 => ::glutin::VirtualKeyCode::Numpad2,
            Key::Numpad3 => ::glutin::VirtualKeyCode::Numpad3,
            Key::Numpad4 => ::glutin::VirtualKeyCode::Numpad4,
            Key::Numpad5 => ::glutin::VirtualKeyCode::Numpad5,
            Key::Numpad6 => ::glutin::VirtualKeyCode::Numpad6,
            Key::Numpad7 => ::glutin::VirtualKeyCode::Numpad7,
            Key::Numpad8 => ::glutin::VirtualKeyCode::Numpad8,
            Key::Numpad9 => ::glutin::VirtualKeyCode::Numpad9,
            Key::AbntC1 => ::glutin::VirtualKeyCode::AbntC1,
            Key::AbntC2 => ::glutin::VirtualKeyCode::AbntC2,
            Key::Add => ::glutin::VirtualKeyCode::Add,
            Key::Apostrophe => ::glutin::VirtualKeyCode::Apostrophe,
            Key::Apps => ::glutin::VirtualKeyCode::Apps,
            Key::At => ::glutin::VirtualKeyCode::At,
            Key::Ax => ::glutin::VirtualKeyCode::Ax,
            Key::Backslash => ::glutin::VirtualKeyCode::Backslash,
            Key::Calculator => ::glutin::VirtualKeyCode::Calculator,
            Key::Capital => ::glutin::VirtualKeyCode::Capital,
            Key::Colon => ::glutin::VirtualKeyCode::Colon,
            Key::Comma => ::glutin::VirtualKeyCode::Comma,
            Key::Convert => ::glutin::VirtualKeyCode::Convert,
            Key::Decimal => ::glutin::VirtualKeyCode::Decimal,
            Key::Divide => ::glutin::VirtualKeyCode::Divide,
            Key::Equals => ::glutin::VirtualKeyCode::Equals,
            Key::Grave => ::glutin::VirtualKeyCode::Grave,
            Key::Kana => ::glutin::VirtualKeyCode::Kana,
            Key::Kanji => ::glutin::VirtualKeyCode::Kanji,
            Key::LAlt => ::glutin::VirtualKeyCode::LAlt,
            Key::LBracket => ::glutin::VirtualKeyCode::LBracket,
            Key::LControl => ::glutin::VirtualKeyCode::LControl,
            Key::LMenu => ::glutin::VirtualKeyCode::LMenu,
            Key::LShift => ::glutin::VirtualKeyCode::LShift,
            Key::LWin => ::glutin::VirtualKeyCode::LWin,
            Key::Mail => ::glutin::VirtualKeyCode::Mail,
            Key::MediaSelect => ::glutin::VirtualKeyCode::MediaSelect,
            Key::MediaStop => ::glutin::VirtualKeyCode::MediaStop,
            Key::Minus => ::glutin::VirtualKeyCode::Minus,
            Key::Multiply => ::glutin::VirtualKeyCode::Multiply,
            Key::Mute => ::glutin::VirtualKeyCode::Mute,
            Key::MyComputer => ::glutin::VirtualKeyCode::MyComputer,
            Key::NavigateForward => ::glutin::VirtualKeyCode::NavigateForward,
            Key::NavigateBackward => ::glutin::VirtualKeyCode::NavigateBackward,
            Key::NextTrack => ::glutin::VirtualKeyCode::NextTrack,
            Key::NoConvert => ::glutin::VirtualKeyCode::NoConvert,
            Key::NumpadComma => ::glutin::VirtualKeyCode::NumpadComma,
            Key::NumpadEnter => ::glutin::VirtualKeyCode::NumpadEnter,
            Key::NumpadEquals => ::glutin::VirtualKeyCode::NumpadEquals,
            Key::OEM102 => ::glutin::VirtualKeyCode::OEM102,
            Key::Period => ::glutin::VirtualKeyCode::Period,
            Key::PlayPause => ::glutin::VirtualKeyCode::PlayPause,
            Key::Power => ::glutin::VirtualKeyCode::Power,
            Key::PrevTrack => ::glutin::VirtualKeyCode::PrevTrack,
            Key::RAlt => ::glutin::VirtualKeyCode::RAlt,
            Key::RBracket => ::glutin::VirtualKeyCode::RBracket,
            Key::RControl => ::glutin::VirtualKeyCode::RControl,
            Key::RMenu => ::glutin::VirtualKeyCode::RMenu,
            Key::RShift => ::glutin::VirtualKeyCode::RShift,
            Key::RWin => ::glutin::VirtualKeyCode::RWin,
            Key::Semicolon => ::glutin::VirtualKeyCode::Semicolon,
            Key::Slash => ::glutin::VirtualKeyCode::Slash,
            Key::Sleep => ::glutin::VirtualKeyCode::Sleep,
            Key::Stop => ::glutin::VirtualKeyCode::Stop,
            Key::Subtract => ::glutin::VirtualKeyCode::Subtract,
            Key::Sysrq => ::glutin::VirtualKeyCode::Sysrq,
            Key::Tab => ::glutin::VirtualKeyCode::Tab,
            Key::Underline => ::glutin::VirtualKeyCode::Underline,
            Key::Unlabeled => ::glutin::VirtualKeyCode::Unlabeled,
            Key::VolumeDown => ::glutin::VirtualKeyCode::VolumeDown,
            Key::VolumeUp => ::glutin::VirtualKeyCode::VolumeUp,
            Key::Wake => ::glutin::VirtualKeyCode::Wake,
            Key::WebBack => ::glutin::VirtualKeyCode::WebBack,
            Key::WebFavorites => ::glutin::VirtualKeyCode::WebFavorites,
            Key::WebForward => ::glutin::VirtualKeyCode::WebForward,
            Key::WebHome => ::glutin::VirtualKeyCode::WebHome,
            Key::WebRefresh => ::glutin::VirtualKeyCode::WebRefresh,
            Key::WebSearch => ::glutin::VirtualKeyCode::WebSearch,
            Key::WebStop => ::glutin::VirtualKeyCode::WebStop,
            Key::Yen => ::glutin::VirtualKeyCode::Yen,
        }
    }
}
