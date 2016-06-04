#version 330 core
layout (location = 0) in vec2 position;

out vec2 TexCoords;

// Terminal properties
uniform vec2 termDim;
uniform vec2 cellDim;
uniform vec2 cellSep;

// Cell properties
uniform vec2 gridCoords;

// glyph properties
uniform vec2 glyphScale;
uniform vec2 glyphOffset;

// uv mapping
uniform vec2 uvScale;
uniform vec2 uvOffset;

// Orthographic projection
uniform mat4 projection;

void main()
{
    // Position of cell from top-left
    vec2 cellPosition = (cellDim + cellSep) * gridCoords;

    // Invert Y since framebuffer origin is bottom-left
    cellPosition.y = termDim.y - cellPosition.y - cellDim.y;

    // Glyphs are offset within their cell; account for y-flip
    vec2 cellOffset = vec2(glyphOffset.x, glyphOffset.y - glyphScale.y);

    // position coordinates are normalized on [0, 1]
    vec2 finalPosition = glyphScale * position + cellPosition + cellOffset;

    gl_Position = projection * vec4(finalPosition.xy, 0.0, 1.0);
    TexCoords = vec2(position.x, 1 - position.y) * uvScale + uvOffset;
}
