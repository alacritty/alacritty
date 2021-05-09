#version 120
#extension GL_ARB_explicit_attrib_location : require
#extension GL_EXT_gpu_shader4 : require
in vec2 TexCoords;
flat in vec4 fg;
flat in vec4 bg;
uniform int backgroundPass;

layout(location = 0, index = 0) out vec4 color;
layout(location = 0, index = 1) out vec4 alphaMask;

uniform sampler2D mask;

#define COLORED 2

void main() {
    if (backgroundPass != 0) {
        if (bg.a == 0.0) {
            discard;
        }

        alphaMask = vec4(1.0);
        color = vec4(bg.rgb, 1.0);
    } else if ((int(fg.a) & COLORED) != 0) {
        // Color glyphs, like emojis.
        vec4 glyphColor = texture2D(mask, TexCoords);
        alphaMask = vec4(glyphColor.a);

        // Revert alpha premultiplication.
        if (glyphColor.a != 0) {
            glyphColor.rgb = vec3(glyphColor.rgb / glyphColor.a);
        }

        color = vec4(glyphColor.rgb, 1.0);
    } else {
        // Regular text glyphs.
        vec3 textColor = texture2D(mask, TexCoords).rgb;
        alphaMask = vec4(textColor, textColor.r);
        color = vec4(fg.rgb, 1.0);
    }
}
