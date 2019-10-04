use crate::config::{Config, ConfigValidationError};
use std::path::PathBuf;

#[test]
fn validate_config_existing_working_directory() {
    let default = Config::default();

    assert!(default.validate().is_none());
}

#[test]
fn validate_config_non_existing_working_directory() {
    let mut default = Config::default();

    let working_directory = PathBuf::new();

    assert!(!working_directory.exists());

    default.set_working_directory(Some(working_directory));

    let validation_result = default.validate();
    assert!(validation_result.is_some());
    assert_eq!(
        validation_result.unwrap(),
        ConfigValidationError {
            field: String::from("working_directory"),
            message: String::from("working directory does not exist"),
        }
    );
}
