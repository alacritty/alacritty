// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//! Constants for bitmap byte order
#![allow(non_upper_case_globals)]
pub const kCGBitmapByteOrder32Little: u32 = 2 << 12;
pub const kCGBitmapByteOrder32Big: u32 = 4 << 12;

#[cfg(target_endian = "little")]
pub const kCGBitmapByteOrder32Host: u32 = kCGBitmapByteOrder32Little;

#[cfg(target_endian = "big")]
pub const kCGBitmapByteOrder32Host: u32 = kCGBitmapByteOrder32Big;

#[cfg(target_endian = "little")]
pub fn extract_rgb(bytes: &[u8]) -> Vec<u8> {
    let pixels = bytes.len() / 4;
    let mut rgb = Vec::with_capacity(pixels * 3);

    for i in 0..pixels {
        let offset = i * 4;
        rgb.push(bytes[offset + 2]);
        rgb.push(bytes[offset + 1]);
        rgb.push(bytes[offset]);
    }

    rgb
}

#[cfg(target_endian = "little")]
pub fn extract_rgba(bytes: &[u8]) -> Vec<u8> {
    let pixels = bytes.len() / 4;
    let mut rgb = Vec::with_capacity(pixels * 3);

    for i in 0..pixels {
        let offset = i * 4;
        rgb.push(bytes[offset + 2]);
        rgb.push(bytes[offset + 1]);
        rgb.push(bytes[offset]);
        rgb.push(bytes[offset + 3]);
    }

    rgb
}

#[cfg(target_endian = "big")]
pub fn extract_rgba(bytes: Vec<u8>) -> Vec<u8> {
    bytes
}

#[cfg(target_endian = "big")]
pub fn extract_rgb(bytes: Vec<u8>) -> Vec<u8> {
    bytes
        .into_iter()
        .enumerate()
        .filter(|&(index, _)| ((index) % 4) != 0)
        .map(|(_, val)| val)
        .collect::<Vec<_>>()
}
