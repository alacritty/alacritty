//! Read user configuration from alacritty.toml + alacritree.toml.
//!
//! `alacritty.toml` is alacritty's own config — we share the file so the user
//! gets matching colors/cursor in both terminals.  `alacritree.toml` lives in
//! the same directory and overrides anything in `alacritty.toml` via a
//! deep-merge.  alacritree-specific options (sidebar colors, etc.) live under
//! a `[ui]` table and are only valid in `alacritree.toml`.

use std::collections::HashMap;
use std::path::PathBuf;

use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle, Rgb};
use egui::Color32;
use serde::Deserialize;

use crate::bindings::{self, KeyBinding};

#[derive(Debug, Clone)]
pub struct Config {
    pub palette: Palette,
    pub ui: UiTheme,
    pub font: FontConfig,
    pub cursor: CursorConfig,
    pub scrolling: ScrollingConfig,
    pub window: WindowConfig,
    pub env: HashMap<String, String>,
    pub shell: Option<ShellConfig>,
    pub selection: SelectionConfig,
    pub bindings: Vec<KeyBinding>,
}

#[derive(Debug, Clone)]
pub struct FontConfig {
    pub size: f32,
    pub normal_family: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct CursorConfig {
    pub shape: CursorShape,
    pub blinking: bool,
    pub unfocused_hollow: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollingConfig {
    pub history: usize,
    pub multiplier: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct WindowConfig {
    pub padding_x: f32,
    pub padding_y: f32,
    pub opacity: f32,
}

#[derive(Debug, Clone)]
pub struct ShellConfig {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SelectionConfig {
    pub semantic_escape_chars: String,
    /// Mirror auto-copy of selections to the regular clipboard.  Off by default
    /// (matches alacritty); when off, drag-select still writes to the X11
    /// PRIMARY / Wayland primary-selection buffer for middle-click paste.
    pub save_to_clipboard: bool,
}

impl Config {
    pub fn cursor_style(&self) -> CursorStyle {
        CursorStyle { shape: self.cursor.shape, blinking: self.cursor.blinking }
    }
}

#[derive(Debug, Clone)]
pub struct Palette {
    pub fg: Rgb,
    pub bg: Rgb,
    pub bright_fg: Option<Rgb>,
    pub dim_fg: Option<Rgb>,
    pub cursor_fg: Option<Rgb>,
    pub cursor_bg: Option<Rgb>,
    pub selection_bg: Option<Rgb>,
    pub selection_fg: Option<Rgb>,
    pub normal: [Rgb; 8],
    pub bright: [Rgb; 8],
    pub dim: Option<[Rgb; 8]>,
    pub indexed: Vec<(u8, Rgb)>,
    pub draw_bold_with_bright: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UiTheme {
    pub sidebar_background: Option<Color32>,
    pub sidebar_foreground: Option<Color32>,
    pub sidebar_border: Option<Color32>,
    pub sidebar_accent: Option<Color32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            palette: Palette::default(),
            ui: UiTheme::default(),
            font: FontConfig::default(),
            cursor: CursorConfig::default(),
            scrolling: ScrollingConfig::default(),
            window: WindowConfig::default(),
            env: HashMap::new(),
            shell: None,
            selection: SelectionConfig::default(),
            bindings: Vec::new(),
        }
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        // Match alacritty's default of 11.25pt; egui's logical points map
        // 1:1 to pt, modulo HiDPI scale.
        Self { size: 11.25, normal_family: None }
    }
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self { shape: CursorShape::Block, blinking: false, unfocused_hollow: true }
    }
}

impl Default for ScrollingConfig {
    fn default() -> Self {
        Self { history: 10_000, multiplier: 3 }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self { padding_x: 0.0, padding_y: 0.0, opacity: 1.0 }
    }
}

impl Default for SelectionConfig {
    fn default() -> Self {
        // Mirrors alacritty_terminal::term::SEMANTIC_ESCAPE_CHARS.
        Self {
            semantic_escape_chars: String::from(",│`|:\"' ()[]{}<>\t"),
            save_to_clipboard: false,
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        // Mirrors alacritty's built-in defaults so a user with no config sees
        // the same colors in both terminals.
        Self {
            fg: rgb(0xd8, 0xd8, 0xd8),
            bg: rgb(0x18, 0x18, 0x18),
            bright_fg: None,
            dim_fg: None,
            cursor_fg: None,
            cursor_bg: None,
            selection_bg: None,
            selection_fg: None,
            normal: [
                rgb(0x18, 0x18, 0x18),
                rgb(0xac, 0x42, 0x42),
                rgb(0x90, 0xa9, 0x59),
                rgb(0xf4, 0xbf, 0x75),
                rgb(0x6a, 0x9f, 0xb5),
                rgb(0xaa, 0x75, 0x9f),
                rgb(0x75, 0xb5, 0xaa),
                rgb(0xd8, 0xd8, 0xd8),
            ],
            bright: [
                rgb(0x6b, 0x6b, 0x6b),
                rgb(0xc5, 0x55, 0x55),
                rgb(0xaa, 0xc4, 0x74),
                rgb(0xfe, 0xca, 0x88),
                rgb(0x82, 0xb8, 0xc8),
                rgb(0xc2, 0x8c, 0xb8),
                rgb(0x93, 0xd3, 0xc3),
                rgb(0xf8, 0xf8, 0xf8),
            ],
            dim: None,
            indexed: Vec::new(),
            draw_bold_with_bright: false,
        }
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

/// Search order for `alacritty.{suffix}` (alacritty's own search order, see
/// `alacritty::config::installed_config`):
///   1. `$XDG_CONFIG_HOME/alacritty/alacritty.{suffix}`
///   2. `$XDG_CONFIG_HOME/alacritty.{suffix}`
///   3. `$HOME/.config/alacritty/alacritty.{suffix}`
///   4. `$HOME/.alacritty.{suffix}`
///   5. `/etc/alacritty/alacritty.{suffix}`
///
/// `alacritree.toml` is searched in the same locations and overrides whatever
/// `alacritty.toml` provided via the same merge semantics alacritty uses.
#[cfg(not(windows))]
fn installed_config(stem: &str, suffix: &str) -> Option<PathBuf> {
    let file_name = format!("{stem}.{suffix}");

    // Match alacritty: prefer XDG, then home fallbacks, then /etc.
    if let Some(p) = xdg::BaseDirectories::with_prefix("alacritty").find_config_file(&file_name) {
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(p) = xdg::BaseDirectories::new().find_config_file(&file_name) {
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(home) = home::home_dir() {
        let candidate = home.join(".config").join("alacritty").join(&file_name);
        if candidate.exists() {
            return Some(candidate);
        }
        let hidden = home.join(format!(".{file_name}"));
        if hidden.exists() {
            return Some(hidden);
        }
    }
    let etc = PathBuf::from("/etc/alacritty").join(&file_name);
    etc.exists().then_some(etc)
}

#[cfg(windows)]
fn installed_config(stem: &str, suffix: &str) -> Option<PathBuf> {
    let file_name = format!("{stem}.{suffix}");
    dirs::config_dir()
        .map(|p| p.join("alacritty").join(&file_name))
        .filter(|p| p.exists())
}

pub fn load() -> Config {
    let alacritty_path = installed_config("alacritty", "toml");
    let alacritree_path = installed_config("alacritree", "toml");

    let empty = || toml::Value::Table(toml::value::Table::new());
    let mut merged = match alacritty_path.as_deref().map(read_toml_value) {
        Some(Ok(Some(v))) => v,
        Some(Err(e)) => {
            log::warn!(
                "failed to load {}: {e}",
                alacritty_path.as_deref().unwrap().display()
            );
            empty()
        }
        _ => empty(),
    };

    if let Some(path) = alacritree_path.as_deref() {
        match read_toml_value(path) {
            Ok(Some(over)) => merged = merge(merged, over),
            Ok(None) => {}
            Err(e) => log::warn!("failed to load {}: {e}", path.display()),
        }
    }

    let raw: RawConfig = match merged.try_into() {
        Ok(r) => r,
        Err(e) => {
            log::warn!("invalid alacritty/alacritree config, using defaults: {e}");
            return Config::default();
        }
    };

    raw.into_config()
}

fn read_toml_value(path: &std::path::Path) -> std::io::Result<Option<toml::Value>> {
    // toml 0.9's `<Value as FromStr>::from_str` is broken; go through the
    // serde entry point instead.  This matches alacritty's `deserialize_config`.
    match std::fs::read_to_string(path) {
        Ok(mut contents) => {
            // Strip UTF-8 BOM the same way alacritty does.
            if contents.starts_with('\u{FEFF}') {
                contents = contents.split_off(3);
            }
            match toml::from_str::<toml::Value>(&contents) {
                Ok(v) => Ok(Some(v)),
                Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Merge two TOML values using alacritty's semantics: arrays are
/// **concatenated** (not replaced), tables are merged recursively, and
/// primitives are replaced.  This matches `alacritty::config::serde_utils::merge`
/// so a `[[keyboard.bindings]]` array in `alacritree.toml` adds to (rather
/// than replaces) the bindings from `alacritty.toml`.
fn merge(base: toml::Value, replacement: toml::Value) -> toml::Value {
    use toml::Value;
    match (base, replacement) {
        (Value::Array(mut base), Value::Array(mut over)) => {
            base.append(&mut over);
            Value::Array(base)
        }
        (Value::Table(base), Value::Table(over)) => Value::Table(merge_tables(base, over)),
        (_, value) => value,
    }
}

fn merge_tables(mut base: toml::value::Table, replacement: toml::value::Table) -> toml::value::Table {
    for (key, value) in replacement {
        let value = match base.remove(&key) {
            Some(existing) => merge(existing, value),
            None => value,
        };
        base.insert(key, value);
    }
    base
}

// --- Raw deserialization ---------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawConfig {
    colors: RawColors,
    ui: RawUi,
    font: RawFont,
    cursor: RawCursor,
    scrolling: RawScrolling,
    window: RawWindow,
    #[serde(default)]
    env: HashMap<String, String>,
    terminal: RawTerminal,
    selection: RawSelection,
    keyboard: RawKeyboard,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawKeyboard {
    bindings: Vec<bindings::RawBinding>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawFont {
    size: Option<f32>,
    normal: RawFontFace,
    bold: RawFontFace,
    italic: RawFontFace,
    bold_italic: RawFontFace,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawFontFace {
    family: Option<String>,
    style: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawCursor {
    style: Option<RawCursorStyle>,
    /// Older alacritty configs accept just `style = "Block"` rather than
    /// `style.shape = "Block"`.  We allow both via `RawCursorStyle`.
    unfocused_hollow: Option<bool>,
    blink_interval: Option<u64>,
    blink_timeout: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawCursorStyle {
    Shape(String),
    Detailed { shape: Option<String>, blinking: Option<String> },
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawScrolling {
    history: Option<u32>,
    multiplier: Option<u8>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawWindow {
    padding: Option<RawPadding>,
    opacity: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawPadding {
    x: Option<f32>,
    y: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawTerminal {
    shell: Option<RawShell>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawShell {
    Program(String),
    Detailed { program: String, #[serde(default)] args: Vec<String> },
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawSelection {
    semantic_escape_chars: Option<String>,
    save_to_clipboard: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawColors {
    #[serde(default)]
    primary: RawPrimary,
    #[serde(default)]
    cursor: RawInverted,
    #[serde(default)]
    selection: RawInverted,
    #[serde(default)]
    normal: RawSet,
    #[serde(default)]
    bright: RawSet,
    #[serde(default)]
    dim: Option<RawSet>,
    #[serde(default)]
    indexed_colors: Vec<RawIndexed>,
    #[serde(default)]
    draw_bold_text_with_bright_colors: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawPrimary {
    foreground: Option<RgbStr>,
    background: Option<RgbStr>,
    bright_foreground: Option<RgbStr>,
    dim_foreground: Option<RgbStr>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawInverted {
    /// Foreground glyph color.  Alacritty calls this `text`; we accept both.
    text: Option<RgbStr>,
    /// Background block color.  Alacritty calls this `cursor`; we accept both.
    cursor: Option<RgbStr>,
    foreground: Option<RgbStr>,
    background: Option<RgbStr>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawSet {
    black: Option<RgbStr>,
    red: Option<RgbStr>,
    green: Option<RgbStr>,
    yellow: Option<RgbStr>,
    blue: Option<RgbStr>,
    magenta: Option<RgbStr>,
    cyan: Option<RgbStr>,
    white: Option<RgbStr>,
}

#[derive(Debug, Deserialize)]
struct RawIndexed {
    index: u8,
    color: RgbStr,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawUi {
    sidebar_background: Option<RgbStr>,
    sidebar_foreground: Option<RgbStr>,
    sidebar_border: Option<RgbStr>,
    sidebar_accent: Option<RgbStr>,
}

/// Wrapper that parses `"0xrrggbb"`, `"#rrggbb"`, or `"rrggbb"` into an `Rgb`.
#[derive(Debug, Clone, Copy)]
struct RgbStr(Rgb);

impl<'de> Deserialize<'de> for RgbStr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        parse_hex_rgb(&s)
            .map(RgbStr)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid color string: {s:?}")))
    }
}

fn parse_hex_rgb(s: &str) -> Option<Rgb> {
    let stripped = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .or_else(|| s.strip_prefix('#'))
        .unwrap_or(s);
    if stripped.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&stripped[0..2], 16).ok()?;
    let g = u8::from_str_radix(&stripped[2..4], 16).ok()?;
    let b = u8::from_str_radix(&stripped[4..6], 16).ok()?;
    Some(Rgb { r, g, b })
}

impl RawConfig {
    fn into_config(self) -> Config {
        let config = Config::default();
        let mut palette = config.palette;
        let c = self.colors;

        if let Some(v) = c.primary.foreground { palette.fg = v.0; }
        if let Some(v) = c.primary.background { palette.bg = v.0; }
        palette.bright_fg = c.primary.bright_foreground.map(|v| v.0);
        palette.dim_fg = c.primary.dim_foreground.map(|v| v.0);

        // Cursor: alacritty's [colors.cursor] uses {text, cursor} for {fg, bg};
        // we accept the literal {foreground, background} too.
        palette.cursor_fg = c.cursor.text.map(|v| v.0).or_else(|| c.cursor.foreground.map(|v| v.0));
        palette.cursor_bg = c.cursor.cursor.map(|v| v.0).or_else(|| c.cursor.background.map(|v| v.0));

        palette.selection_fg =
            c.selection.text.map(|v| v.0).or_else(|| c.selection.foreground.map(|v| v.0));
        palette.selection_bg = c.selection.background.map(|v| v.0);

        apply_set(&mut palette.normal, c.normal);
        apply_set(&mut palette.bright, c.bright);
        if let Some(d) = c.dim {
            let mut dim = palette.normal;
            apply_set(&mut dim, d);
            palette.dim = Some(dim);
        }

        palette.indexed = c
            .indexed_colors
            .into_iter()
            .filter(|i| i.index >= 16)
            .map(|i| (i.index, i.color.0))
            .collect();

        palette.draw_bold_with_bright = c.draw_bold_text_with_bright_colors;

        let ui = UiTheme {
            sidebar_background: self.ui.sidebar_background.map(|v| rgb_to_color32(v.0)),
            sidebar_foreground: self.ui.sidebar_foreground.map(|v| rgb_to_color32(v.0)),
            sidebar_border: self.ui.sidebar_border.map(|v| rgb_to_color32(v.0)),
            sidebar_accent: self.ui.sidebar_accent.map(|v| rgb_to_color32(v.0)),
        };

        // ---- Font ----
        let mut font = config.font.clone();
        if let Some(s) = self.font.size {
            font.size = s.max(1.0);
        }
        font.normal_family = self.font.normal.family.clone();

        // ---- Cursor ----
        let mut cursor = config.cursor;
        if let Some(style) = self.cursor.style {
            apply_cursor_style(&mut cursor, style);
        }
        if let Some(v) = self.cursor.unfocused_hollow {
            cursor.unfocused_hollow = v;
        }

        // ---- Scrolling ----
        let mut scrolling = config.scrolling;
        if let Some(h) = self.scrolling.history {
            scrolling.history = h as usize;
        }
        if let Some(m) = self.scrolling.multiplier {
            scrolling.multiplier = m;
        }

        // ---- Window padding ----
        let mut window = config.window;
        if let Some(p) = self.window.padding {
            if let Some(x) = p.x {
                window.padding_x = x;
            }
            if let Some(y) = p.y {
                window.padding_y = y;
            }
        }
        if let Some(o) = self.window.opacity {
            window.opacity = o.clamp(0.0, 1.0);
        }

        // ---- Selection ----
        let mut selection = config.selection.clone();
        if let Some(s) = self.selection.semantic_escape_chars {
            selection.semantic_escape_chars = s;
        }
        if let Some(v) = self.selection.save_to_clipboard {
            selection.save_to_clipboard = v;
        }

        // ---- Shell ----
        let shell = self.terminal.shell.map(|s| match s {
            RawShell::Program(program) => ShellConfig { program, args: Vec::new() },
            RawShell::Detailed { program, args } => ShellConfig { program, args },
        });

        let bindings = bindings::parse_bindings(self.keyboard.bindings);

        Config {
            palette,
            ui,
            font,
            cursor,
            scrolling,
            window,
            env: self.env,
            shell,
            selection,
            bindings,
        }
    }
}

fn apply_cursor_style(cursor: &mut CursorConfig, style: RawCursorStyle) {
    let (shape, blinking) = match style {
        RawCursorStyle::Shape(s) => (Some(s), None),
        RawCursorStyle::Detailed { shape, blinking } => (shape, blinking),
    };
    if let Some(s) = shape.as_deref() {
        cursor.shape = match s {
            "Block" | "block" => CursorShape::Block,
            "Underline" | "underline" => CursorShape::Underline,
            "Beam" | "beam" => CursorShape::Beam,
            "HollowBlock" | "hollow_block" => CursorShape::HollowBlock,
            "Hidden" | "hidden" => CursorShape::Hidden,
            other => {
                log::warn!("unknown cursor shape: {other}");
                cursor.shape
            }
        };
    }
    if let Some(b) = blinking.as_deref() {
        cursor.blinking = matches!(b, "On" | "on" | "Always" | "always");
    }
}

fn apply_set(target: &mut [Rgb; 8], set: RawSet) {
    let names = [set.black, set.red, set.green, set.yellow, set.blue, set.magenta, set.cyan, set.white];
    for (slot, val) in target.iter_mut().zip(names) {
        if let Some(v) = val {
            *slot = v.0;
        }
    }
}

fn rgb_to_color32(r: Rgb) -> Color32 {
    Color32::from_rgb(r.r, r.g, r.b)
}
