use std::sync::{Arc, Mutex, OnceLock};

use log::{Level, Log, Metadata, Record};
use serde::Deserialize;

use alacritty_config::SerdeReplace as _;
use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};

#[derive(ConfigDeserialize, Debug, PartialEq, Eq)]
enum TestEnum {
    One,
    Two,
    Three,
    #[config(skip)]
    Nine(String),
}

impl Default for TestEnum {
    fn default() -> Self {
        Self::Nine(String::from("nine"))
    }
}

#[derive(ConfigDeserialize)]
struct Test {
    #[config(alias = "field1_alias")]
    #[config(deprecated = "use field2 instead")]
    field1: usize,
    #[config(deprecated = "shouldn't be hit")]
    field2: String,
    field3: Option<u8>,
    #[doc(hidden)]
    nesting: Test2<usize>,
    #[config(flatten)]
    flatten: Test3,
    enom_small: TestEnum,
    enom_big: TestEnum,
    #[config(deprecated)]
    enom_error: TestEnum,
    #[config(removed = "it's gone")]
    gone: bool,
    #[config(alias = "multiple_alias1")]
    #[config(alias = "multiple_alias2")]
    multiple_alias_field: usize,
}

impl Default for Test {
    fn default() -> Self {
        Self {
            field1: 13,
            field2: String::from("field2"),
            field3: Some(23),
            nesting: Test2::default(),
            flatten: Test3::default(),
            enom_small: TestEnum::default(),
            enom_big: TestEnum::default(),
            enom_error: TestEnum::default(),
            gone: false,
            multiple_alias_field: 0,
        }
    }
}

#[derive(ConfigDeserialize, Default)]
struct Test2<T: Default> {
    field1: T,
    field2: Option<usize>,
    #[config(skip)]
    field3: usize,
    #[config(alias = "aliased")]
    field4: u8,
    newtype: NewType,
}

#[derive(ConfigDeserialize, Default)]
struct Test3 {
    #[config(alias = "flatty_alias")]
    flatty: usize,
}

#[derive(SerdeReplace, Deserialize, Default, PartialEq, Eq, Debug)]
struct NewType(usize);

#[test]
fn config_deserialize() {
    static LOGGER: OnceLock<Logger> = OnceLock::new();
    let logger = LOGGER.get_or_init(Logger::default);

    log::set_logger(logger).unwrap();
    log::set_max_level(log::LevelFilter::Warn);

    let test: Test = toml::from_str(
        r#"
        field1 = 3
        field3 = 32

        flatty = 123
        enom_small = "one"
        enom_big = "THREE"
        enom_error = "HugaBuga"
        gone = false

        [nesting]
        field1 = "testing"
        field2 = "None"
        field3 = 99
        aliased = 8
    "#,
    )
    .unwrap();

    // Verify fields were deserialized correctly.
    assert_eq!(test.field1, 3);
    assert_eq!(test.field2, Test::default().field2);
    assert_eq!(test.field3, Some(32));
    assert_eq!(test.enom_small, TestEnum::One);
    assert_eq!(test.enom_big, TestEnum::Three);
    assert_eq!(test.enom_error, Test::default().enom_error);
    assert!(!test.gone);
    assert_eq!(test.nesting.field1, Test::default().nesting.field1);
    assert_eq!(test.nesting.field2, None);
    assert_eq!(test.nesting.field3, Test::default().nesting.field3);
    assert_eq!(test.nesting.field4, 8);
    assert_eq!(test.flatten.flatty, 123);

    // Verify all log messages are correct.
    let mut error_logs = logger.error_logs.lock().unwrap();
    error_logs.sort_unstable();
    assert_eq!(error_logs.as_slice(), [
        "Config error: enom_error: unknown variant `HugaBuga`, expected one of `One`, `Two`, \
         `Three`",
        "Config error: field1: invalid type: string \"testing\", expected usize",
    ]);
    let mut warn_logs = logger.warn_logs.lock().unwrap();
    warn_logs.sort_unstable();
    assert_eq!(warn_logs.as_slice(), [
        "Config warning: enom_error has been deprecated\nUse `alacritty migrate` to automatically \
         resolve it",
        "Config warning: field1 has been deprecated; use field2 instead\nUse `alacritty migrate` \
         to automatically resolve it",
        "Config warning: gone has been removed; it's gone\nUse `alacritty migrate` to \
         automatically resolve it",
        "Unused config key: field3",
    ]);
}

/// Logger storing all messages for later validation.
#[derive(Default)]
struct Logger {
    error_logs: Arc<Mutex<Vec<String>>>,
    warn_logs: Arc<Mutex<Vec<String>>>,
}

impl Log for Logger {
    fn log(&self, record: &Record<'_>) {
        assert_eq!(record.target(), env!("CARGO_PKG_NAME"));

        match record.level() {
            Level::Error => {
                let mut error_logs = self.error_logs.lock().unwrap();
                error_logs.push(record.args().to_string());
            },
            Level::Warn => {
                let mut warn_logs = self.warn_logs.lock().unwrap();
                warn_logs.push(record.args().to_string());
            },
            _ => unreachable!(),
        }
    }

    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn flush(&self) {}
}

#[test]
fn field_replacement() {
    let mut test = Test::default();

    let value = toml::from_str("nesting.field2=13").unwrap();
    test.replace(value).unwrap();

    assert_eq!(test.nesting.field2, Some(13));
}

#[test]
fn replace_derive() {
    let mut test = Test::default();

    let value = toml::from_str("nesting.newtype=9").unwrap();
    test.replace(value).unwrap();

    assert_eq!(test.nesting.newtype, NewType(9));
}

#[test]
fn replace_derive_using_alias() {
    let mut test = Test::default();

    assert_ne!(test.field1, 9);

    let value = toml::from_str("field1_alias=9").unwrap();
    test.replace(value).unwrap();

    assert_eq!(test.field1, 9);
}

#[test]
fn replace_derive_using_multiple_aliases() {
    let mut test = Test::default();

    let toml_value = toml::from_str("multiple_alias1=6").unwrap();
    test.replace(toml_value).unwrap();

    assert_eq!(test.multiple_alias_field, 6);

    let toml_value = toml::from_str("multiple_alias1=7").unwrap();
    test.replace(toml_value).unwrap();

    assert_eq!(test.multiple_alias_field, 7);
}

#[test]
fn replace_flatten() {
    let mut test = Test::default();

    let value = toml::from_str("flatty=7").unwrap();
    test.replace(value).unwrap();

    assert_eq!(test.flatten.flatty, 7);
}

#[test]
fn replace_flatten_using_alias() {
    let mut test = Test::default();

    assert_ne!(test.flatten.flatty, 7);

    let value = toml::from_str("flatty_alias=7").unwrap();
    test.replace(value).unwrap();

    assert_eq!(test.flatten.flatty, 7);
}
