use std::collections::HashMap;
use std::error::Error;

use log::LevelFilter;
use serde::Deserialize;
use serde_yaml::Value;

pub trait SerdeReplace {
    fn replace(&mut self, key: &str, value: Value) -> Result<(), Box<dyn Error>>;
}

macro_rules! impl_replace {
    ($($ty:ty),*$(,)*) => {
        $(
            impl SerdeReplace for $ty {
                fn replace(&mut self, key: &str, value: Value) -> Result<(), Box<dyn Error>> {
                    replace_simple(self, key, value)
                }
            }
        )*
    };
}

#[rustfmt::skip]
impl_replace!(
    usize, u8, u16, u32, u64, u128,
    isize, i8, i16, i32, i64, i128,
    f32, f64,
    bool,
    char,
    String,
    LevelFilter,
);

#[cfg(target_os = "macos")]
impl_replace!(winit::platform::macos::OptionAsAlt,);

fn replace_simple<'de, D>(data: &mut D, key: &str, value: Value) -> Result<(), Box<dyn Error>>
where
    D: Deserialize<'de>,
{
    if !key.is_empty() {
        let error = format!("Fields \"{key}\" do not exist");
        return Err(error.into());
    }
    *data = D::deserialize(value)?;

    Ok(())
}

impl<'de, T: Deserialize<'de>> SerdeReplace for Vec<T> {
    fn replace(&mut self, key: &str, value: Value) -> Result<(), Box<dyn Error>> {
        replace_simple(self, key, value)
    }
}

impl<'de, T: Deserialize<'de>> SerdeReplace for Option<T> {
    fn replace(&mut self, key: &str, value: Value) -> Result<(), Box<dyn Error>> {
        replace_simple(self, key, value)
    }
}

impl<'de, T: Deserialize<'de>> SerdeReplace for HashMap<String, T> {
    fn replace(&mut self, key: &str, value: Value) -> Result<(), Box<dyn Error>> {
        replace_simple(self, key, value)
    }
}
