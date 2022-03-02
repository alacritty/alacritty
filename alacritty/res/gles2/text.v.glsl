// Cell coords.
attribute vec2 cellCoords;

// Glyph coords.
attribute vec2 glyphCoords;

// uv mapping.
attribute vec2 uv;

// Text foreground rgb packed together with cell flags. textColor.a
// are the bitflags; consult RenderingGlyphFlags in renderer/mod.rs
// for the possible values.
attribute vec4 textColor;

// Background color.
attribute vec4 backgroundColor;

varying vec2 TexCoords;
varying vec3 fg;
varying float colored;
varying vec4 bg;

uniform highp int renderingPass;
uniform vec4 projection;

void main() {
    vec2 projectionOffset = projection.xy;
    vec2 projectionScale = projection.zw;

    vec2 position;
    if (renderingPass == 0) {
        TexCoords = vec2(0, 0);
        position = cellCoords;
    } else {
        TexCoords = uv;
        position = glyphCoords;
    }

    fg = vec3(float(textColor.r), float(textColor.g), float(textColor.b)) / 255.;
    colored = float(textColor.a);
    bg = vec4(float(backgroundColor.r), float(backgroundColor.g), float(backgroundColor.b),
            float(backgroundColor.a)) / 255.;

    vec2 finalPosition = projectionOffset + position * projectionScale;
    gl_Position = vec4(finalPosition, 0., 1.);
}
