//! Configuration definitions and file loading
//!
//! Alacritty reads from a config file at startup to determine various runtime
//! parameters including font family and style, font size, etc. In the future,
//! the config file will also hold user and platform specific keybindings.
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use ::Rgb;
use font::Size;
use serde_yaml;
use serde::{self, Error as SerdeError};

/// Top-level config type
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    /// Pixels per inch
    #[serde(default)]
    dpi: Dpi,

    /// Font configuration
    #[serde(default)]
    font: Font,

    /// Should show render timer
    #[serde(default)]
    render_timer: bool,

    /// The standard ANSI colors to use
    #[serde(default)]
    colors: Colors,
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
    normal: AnsiColors,
    bright: AnsiColors,
}

#[derive(Debug, Deserialize)]
pub struct PrimaryColors {
    background: Rgb,
    foreground: Rgb,
}

impl Default for Colors {
    fn default() -> Colors {
        Colors {
            primary: PrimaryColors {
                background: Rgb { r: 0, g: 0, b: 0 },
                foreground: Rgb { r: 0xea, g: 0xea, b: 0xea },
            },
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
    black: Rgb,
    red: Rgb,
    green: Rgb,
    yellow: Rgb,
    blue: Rgb,
    magenta: Rgb,
    cyan: Rgb,
    white: Rgb,
}

impl serde::de::Deserialize for Rgb {
    fn deserialize<D>(deserializer: &mut D) -> ::std::result::Result<Self, D::Error>
        where D: serde::de::Deserializer
    {
        use std::marker::PhantomData;

        struct StringVisitor<__D> {
            _marker: PhantomData<__D>,
        }

        impl<__D> ::serde::de::Visitor for StringVisitor<__D>
            where __D: ::serde::de::Deserializer
        {
            type Value = String;

            fn visit_str<E>(&mut self, value: &str) -> ::std::result::Result<Self::Value, E>
                where E: ::serde::de::Error
            {
                Ok(value.to_owned())
            }
        }

        deserializer
            .deserialize_f64(StringVisitor::<D>{ _marker: PhantomData })
            .and_then(|v| {
                Rgb::from_str(&v[..])
                    .map_err(|_| D::Error::custom("failed to parse rgb; expect 0xrrggbb"))
            })
    }
}

impl Rgb {
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
    /// 1. `$HOME/.config/alacritty.yml`
    /// 2. `$HOME/.alacritty.yml`
    pub fn load() -> Result<Config> {
        let home = env::var("HOME")?;

        // First path
        let mut path = PathBuf::from(&home);
        path.push(".config");
        path.push("alacritty.yml");

        // Fallback path
        let mut alt_path = PathBuf::from(&home);
        alt_path.push(".alacritty.yml");

        match Config::load_from(&path) {
            Ok(c) => Ok(c),
            Err(e) => {
                match e {
                    Error::NotFound => Config::load_from(&alt_path),
                    _ => Err(e),
                }
            }
        }
    }

    /// Get list of colors
    ///
    /// The ordering returned here is expected by the terminal. Colors are simply indexed in this
    /// array for performance.
    pub fn color_list(&self) -> [Rgb; 18] {
        let colors = &self.colors;

        [
            // Normals
            colors.normal.black,
            colors.normal.red,
            colors.normal.green,
            colors.normal.yellow,
            colors.normal.blue,
            colors.normal.magenta,
            colors.normal.cyan,
            colors.normal.white,

            // Brights
            colors.bright.black,
            colors.bright.red,
            colors.bright.green,
            colors.bright.yellow,
            colors.bright.blue,
            colors.bright.magenta,
            colors.bright.cyan,
            colors.bright.white,

            // Foreground and background
            colors.primary.foreground,
            colors.primary.background,
        ]
    }

    pub fn fg_color(&self) -> Rgb {
        self.colors.primary.foreground
    }

    pub fn bg_color(&self) -> Rgb {
        self.colors.primary.background
    }

    /// Get font config
    #[inline]
    pub fn font(&self) -> &Font {
        &self.font
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

    fn load_from<P: AsRef<Path>>(path: P) -> Result<Config> {
        let raw = Config::read_file(path)?;
        Ok(serde_yaml::from_str(&raw[..])?)
    }

    fn read_file<P: AsRef<Path>>(path: P) -> Result<String> {
        let mut f = fs::File::open(path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;

        Ok(contents)
    }
}

/// Pixels per inch
///
/// This is only used on FreeType systems
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
    fn deserialize_from_f32<D>(&mut D) -> ::std::result::Result<Self, D::Error>
        where D: serde::de::Deserializer;
}

impl DeserializeFromF32 for Size {
    fn deserialize_from_f32<D>(deserializer: &mut D) -> ::std::result::Result<Self, D::Error>
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

            fn visit_f64<E>(&mut self, value: f64) -> ::std::result::Result<Self::Value, E>
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
    family: String,

    /// Font style
    style: String,

    /// Bold font style
    bold_style: Option<String>,

    /// Italic font style
    italic_style: Option<String>,

    // Font size in points
    #[serde(deserialize_with="DeserializeFromF32::deserialize_from_f32")]
    size: Size,

    /// Extra spacing per character
    offset: FontOffset,
}

impl Font {
    /// Get the font family
    #[inline]
    pub fn family(&self) -> &str {
        &self.family[..]
    }

    /// Get the font style
    #[inline]
    pub fn style(&self) -> &str {
        &self.style[..]
    }

    /// Get italic font style; assumes same family
    #[inline]
    pub fn italic_style(&self) -> Option<&str> {
        self.italic_style
            .as_ref()
            .map(|s| s.as_str())
    }

    /// Get bold font style; assumes same family
    #[inline]
    pub fn bold_style(&self) -> Option<&str> {
        self.bold_style
            .as_ref()
            .map(|s| s.as_str())
    }

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
            family: String::from("Menlo"),
            style: String::from("Regular"),
            size: Size::new(11.0),
            bold_style: Some(String::from("Bold")),
            italic_style: Some(String::from("Italic")),
            offset: FontOffset {
                x: 0.0,
                y: 0.0
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl Default for Font {
    fn default() -> Font {
        Font {
            family: String::from("DejaVu Sans Mono"),
            style: String::from("Book"),
            size: Size::new(11.0),
            bold_style: Some(String::from("Bold")),
            italic_style: Some(String::from("Italic")),
            offset: FontOffset {
                // TODO should improve freetype metrics... shouldn't need such
                // drastic offsets for the default!
                x: 2.0,
                y: -7.0
            }
        }
    }
}
