use std::sync::{Arc, Mutex};

use log::{Level, Log, Metadata, Record};

use alacritty_config_derive::ConfigDeserialize;

#[derive(ConfigDeserialize)]
struct Test {
    field1: usize,
    field2: String,
    field3: Option<u8>,
    nesting: Test2<usize>,
}

impl Default for Test {
    fn default() -> Self {
        Self {
            field1: 13,
            field2: String::from("field2"),
            field3: Some(23),
            nesting: Test2::default(),
        }
    }
}

#[derive(ConfigDeserialize, Default)]
struct Test2<T: Default> {
    field1: T,
    field2: Option<usize>,
}

fn main() {
    let logger = unsafe {
        LOGGER = Some(Logger::default());
        LOGGER.as_mut().unwrap()
    };

    log::set_logger(logger).unwrap();
    log::set_max_level(log::LevelFilter::Error);

    let test: Test = serde_yaml::from_str(
        r#"
        field1: 3
        field3: 32
        nesting:
          field1: "testing"
          field2: None
    "#,
    )
    .unwrap();

    // Verify fields were deserialized correctly.
    assert_eq!(test.field1, 3);
    assert_eq!(test.field2, Test::default().field2);
    assert_eq!(test.field3, Some(32));
    assert_eq!(test.nesting.field1, Test::default().nesting.field1);
    assert_eq!(test.nesting.field2, None);

    // Verify all log messages are correct.
    let logs = logger.logs.lock().unwrap();
    assert_eq!(logs.as_slice(), ["Config error: invalid type: string \"testing\", expected usize"]);
}

static mut LOGGER: Option<Logger> = None;

/// Logger storing all messages for later validation.
#[derive(Default)]
struct Logger {
    logs: Arc<Mutex<Vec<String>>>,
}

impl Log for Logger {
    fn log(&self, record: &Record) {
        assert_eq!(record.level(), Level::Error);
        assert_eq!(record.target(), env!("CARGO_PKG_NAME"));

        let mut logs = self.logs.lock().unwrap();
        logs.push(record.args().to_string());
    }

    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn flush(&self) {}
}
