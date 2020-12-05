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
}

fn main() {
    env_logger::init();

    let test: Test = serde_yaml::from_str(
        r#"
        field1: 3
        field3: 32
        nesting:
          field1: "testing"
    "#,
    )
    .unwrap();

    assert_eq!(test.field1, 3);
    assert_eq!(test.field2, Test::default().field2);
    assert_eq!(test.field3, Some(32));
    assert_eq!(test.nesting.field1, Test::default().nesting.field1);
}
