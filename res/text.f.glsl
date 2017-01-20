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
flat in float vb;
flat in int background;

layout(location = 0, index = 0) out vec4 color;
layout(location = 0, index = 1) out vec4 alphaMask;

uniform float bgOpacity;
uniform sampler2D mask;

void main()
{
    if (background != 0) {
        alphaMask = vec4(1.0);
        color = vec4(bg + vb, 1.0) * bgOpacity;
    } else {
        vec3 textColor = texture(mask, TexCoords).rgb;
        alphaMask = vec4(textColor, textColor.r);
        color = vec4(fg, 1.0);
    }
}
