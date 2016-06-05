#version 330 core
layout (location = 0) in vec2 position;

out vec2 TexCoords;
flat out int InstanceId;

// Terminal properties
uniform vec2 termDim;
uniform vec2 cellDim;
uniform vec2 cellSep;

// Cell properties
uniform vec2 gridCoords[32];

// glyph properties
uniform vec2 glyphScale[32];
uniform vec2 glyphOffset[32];

// uv mapping
uniform vec2 uvScale[32];
uniform vec2 uvOffset[32];

// Orthographic projection
uniform mat4 projection;

void main()
{
    int i = gl_InstanceID;

    // Position of cell from top-left
    vec2 cellPosition = (cellDim + cellSep) * gridCoords[i];

    // Invert Y since framebuffer origin is bottom-left
    cellPosition.y = termDim.y - cellPosition.y - cellDim.y;

    // Glyphs are offset within their cell; account for y-flip
    vec2 cellOffset = vec2(glyphOffset[i].x,
                           glyphOffset[i].y - glyphScale[i].y);

    // position coordinates are normalized on [0, 1]
    vec2 finalPosition = glyphScale[i] * position + cellPosition + cellOffset;

    gl_Position = projection * vec4(finalPosition.xy, 0.0, 1.0);
    TexCoords = vec2(position.x, 1 - position.y) * uvScale[i] + uvOffset[i];
    InstanceId = i;
}
