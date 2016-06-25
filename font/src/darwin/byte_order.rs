//! Constants for bitmap byte order
#![allow(non_upper_case_globals)]
pub const kCGBitmapByteOrder32Little: u32 = 2 << 12;
pub const kCGBitmapByteOrder32Big: u32 = 4 << 12;

#[cfg(target_endian = "little")]
pub const kCGBitmapByteOrder32Host: u32 = kCGBitmapByteOrder32Little;

#[cfg(target_endian = "big")]
pub const kCGBitmapByteOrder32Host: u32 = kCGBitmapByteOrder32Big;

#[cfg(target_endian = "little")]
pub fn extract_rgb(bytes: Vec<u8>) -> Vec<u8> {
    let pixels = bytes.len() / 4;
    let mut rgb = Vec::with_capacity(pixels * 3);

    for i in 0..pixels {
        let offset = i * 4;
        rgb.push(bytes[offset + 2]);
        rgb.push(bytes[offset + 1]);
        rgb.push(bytes[offset + 0]);
    }

    rgb
}

#[cfg(target_endian = "big")]
pub fn extract_rgb(bytes: Vec<u8>) -> Vec<u8> {
    bytes.into_iter()
         .enumerate()
         .filter(|&(index, _)| ((index) % 4) != 0)
         .map(|(_, val)| val)
         .collect::<Vec<_>>()
}
