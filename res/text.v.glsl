#version 330 core
layout (location = 0) in vec2 position;

// Cell properties
layout (location = 1) in vec2 gridCoords;

// glyph properties
layout (location = 2) in vec4 glyph;

// uv mapping
layout (location = 3) in vec4 uv;

// text fg color
layout (location = 4) in vec3 textColor;
// Background color
layout (location = 5) in vec3 backgroundColor;

out vec2 TexCoords;
out vec3 fg;
out vec3 bg;

// Terminal properties
uniform vec2 termDim;
uniform vec2 cellDim;
uniform vec2 cellSep;

uniform int backgroundPass;

// Orthographic projection
uniform mat4 projection;
flat out int background;

void main()
{
    vec2 glyphOffset = glyph.xy;
    vec2 glyphSize = glyph.zw;
    vec2 uvOffset = uv.xy;
    vec2 uvSize = uv.zw;

    // Position of cell from top-left
    vec2 cellPosition = (cellDim + cellSep) * gridCoords;

    // Invert Y since framebuffer origin is bottom-left
    cellPosition.y = termDim.y - cellPosition.y - cellDim.y;

    if (backgroundPass != 0) {
        cellPosition.y = cellPosition.y - 3;
        vec2 finalPosition = (cellDim + cellSep) * position + cellPosition;
        gl_Position = projection * vec4(finalPosition.xy, 0.0, 1.0);
        TexCoords = vec2(0, 0);
    } else {

        // Glyphs are offset within their cell; account for y-flip
        vec2 cellOffset = vec2(glyphOffset.x, glyphOffset.y - glyphSize.y);

        // position coordinates are normalized on [0, 1]
        vec2 finalPosition = glyphSize * position + cellPosition + cellOffset;

        gl_Position = projection * vec4(finalPosition.xy, 0.0, 1.0);
        TexCoords = uvOffset + vec2(position.x, 1 - position.y) * uvSize;
    }

    background = backgroundPass;
    bg = backgroundColor / vec3(255.0, 255.0, 255.0);
    fg = textColor / vec3(255.0, 255.0, 255.0);
}
