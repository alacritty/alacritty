use std::sync::{Arc, Mutex};

use log::{Level, Log, Metadata, Record};

use alacritty_config_derive::ConfigDeserialize;

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
    #[config(alias = "noalias")]
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
}

#[derive(ConfigDeserialize, Default)]
struct Test3 {
    flatty: usize,
}

#[test]
fn config_deserialize() {
    let logger = unsafe {
        LOGGER = Some(Logger::default());
        LOGGER.as_mut().unwrap()
    };

    log::set_logger(logger).unwrap();
    log::set_max_level(log::LevelFilter::Warn);

    let test: Test = serde_yaml::from_str(
        r#"
        field1: 3
        field3: 32
        nesting:
          field1: "testing"
          field2: None
          field3: 99
          aliased: 8
        flatty: 123
        enom_small: "one"
        enom_big: "THREE"
        enom_error: "HugaBuga"
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
    assert_eq!(test.nesting.field1, Test::default().nesting.field1);
    assert_eq!(test.nesting.field2, None);
    assert_eq!(test.nesting.field3, Test::default().nesting.field3);
    assert_eq!(test.nesting.field4, 8);
    assert_eq!(test.flatten.flatty, 123);

    // Verify all log messages are correct.
    let error_logs = logger.error_logs.lock().unwrap();
    assert_eq!(error_logs.as_slice(), [
        "Config error: field1: invalid type: string \"testing\", expected usize",
        "Config error: enom_error: unknown variant `HugaBuga`, expected one of `One`, `Two`, \
         `Three`",
    ]);
    let warn_logs = logger.warn_logs.lock().unwrap();
    assert_eq!(warn_logs.as_slice(), [
        "Config warning: field1 is deprecated; use field2 instead",
        "Config warning: enom_error is deprecated",
    ]);
}

static mut LOGGER: Option<Logger> = None;

/// Logger storing all messages for later validation.
#[derive(Default)]
struct Logger {
    error_logs: Arc<Mutex<Vec<String>>>,
    warn_logs: Arc<Mutex<Vec<String>>>,
}

impl Log for Logger {
    fn log(&self, record: &Record) {
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

    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn flush(&self) {}
}
