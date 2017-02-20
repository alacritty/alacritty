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
#version 330 core
in vec2 TexCoords;
in vec3 fg;
in vec3 bg;
flat in vec3 vbc;
flat in int vbe;
flat in float vbi;
flat in int background;

layout(location = 0, index = 0) out vec4 color;
layout(location = 0, index = 1) out vec4 alphaMask;

uniform sampler2D mask;

#define FLASH_TEXT 1
#define FLASH_BACKGROUND 2

void main()
{
    if (background != 0) {
        alphaMask = vec4(1.0, 1.0, 1.0, 1.0);
        color = vec4(mix(bg, vbc, vbe == FLASH_BACKGROUND ? vbi : 0.0), 1.0);
    } else {
        alphaMask = vec4(texture(mask, TexCoords).rgb, 1.0);
        color = vec4(mix(fg, vbc, vbe == FLASH_TEXT ? vbi : 0.0), 1.0);
    }
}
