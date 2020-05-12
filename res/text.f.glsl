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
flat in vec3 fg;
flat in vec4 bg;
flat in int colored;
uniform int backgroundPass;

layout(location = 0, index = 0) out vec4 color;
layout(location = 0, index = 1) out vec4 alphaMask;

uniform sampler2D mask;

void main()
{
    if (backgroundPass != 0) {
        if (bg.a == 0.0)
            discard;

        alphaMask = vec4(1.0);
        color = vec4(bg.rgb, 1.0);
    } else {
        if (colored != 0) {
            // Color glyphs, like emojis.
            vec4 glyphColor = texture(mask, TexCoords);
            alphaMask = vec4(glyphColor.a);

            // Revert alpha premultiplication.
            if (glyphColor.a != 0) {
                glyphColor.rgb = vec3(glyphColor.rgb / glyphColor.a);
            }

            color = vec4(glyphColor.rgb, 1.0);
        } else {
            // Regular text glyphs.
            vec3 textColor = texture(mask, TexCoords).rgb;
            alphaMask = vec4(textColor, textColor.r);
            color = vec4(fg, 1.0);
        }
    }
}
