#ifdef GLES2_RENDERER
#extension GL_EXT_blend_func_extended: require
#define MEDIUMP mediump
#define COLORED 1
varying mediump vec2 TexCoords;
varying mediump vec3 fg;
varying highp float colored;
varying mediump vec4 bg;

uniform highp int renderingPass;

#define FRAG_COLOR gl_FragColor
#define ALPHA_MASK gl_SecondaryFragColorEXT
#define texture texture2D
#else
#define MEDIUMP
#define COLORED 2
in vec2 TexCoords;
flat in vec4 fg;
flat in vec4 bg;
uniform int backgroundPass;

layout(location = 0, index = 0) out vec4 color;
layout(location = 0, index = 1) out vec4 alphaMask;
#define FRAG_COLOR color
#define ALPHA_MASK alphaMask
#endif
uniform sampler2D mask;

void main() {
#ifdef GLES2_RENDERER
    if (renderingPass == 0) {
#else
    if (backgroundPass != 0) {
#endif
        if (bg.a == 0.0) {
            discard;
        }

        ALPHA_MASK = vec4(1.0);

        // Premultiply background color by alpha.
        FRAG_COLOR = vec4(bg.rgb * bg.a, bg.a);
#ifdef GLES2_RENDERER
    } else if (int(colored) == COLORED) {
#else
    } else if ((int(fg.a) & COLORED) != 0) {
#endif
        // Color glyphs, like emojis.
        MEDIUMP vec4 glyphColor = texture(mask, TexCoords);
        ALPHA_MASK = vec4(glyphColor.a);

        // Revert alpha premultiplication.
        if (glyphColor.a != 0.0) {
            glyphColor.rgb = vec3(glyphColor.rgb / glyphColor.a);
        }

        FRAG_COLOR = vec4(glyphColor.rgb, 1.0);
    } else {
        // Regular text glyphs.
        MEDIUMP vec3 textColor = texture(mask, TexCoords).rgb;
        ALPHA_MASK = vec4(textColor, textColor.r);
        FRAG_COLOR = vec4(fg.rgb, 1.0);
    }
}
