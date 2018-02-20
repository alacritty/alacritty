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
use config::Delta;
use font::Size;

struct Font {
    family: String,
    options: Option<Options>,
}

enum AntiAlias {
    LCD,
    LCDV,
    GRAY,
    // TODO: Maybe change the name so it's not confused with Rust's None?
    NONE,
}

struct Options {
    size: Option<Size>,
    thin_strokes: Option<bool>,
    antialias: Option<AntiAlias>,
    hinting: Option<bool>,
    style: Option<String>,
    offset: Option<Delta>,
    range: Option<FontRange>,
}

struct FontRange {
    start: char,
    end: char,
}
