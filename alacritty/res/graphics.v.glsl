#if defined(GLES2_RENDERER) ///////////////////////////////////////////

precision highp float;

#define LAYOUT_LOCATION(N)      attribute
#define IN
#define FLAT
#define OUT                     varying

#define INT                     float

#define IS_RIGHT_SIDE           (int(sides) == 1 || int(sides) == 3)
#define IS_BOTTOM_SIDE          (int(sides) == 2 || int(sides) == 3)

#else // GLSL3_RENDERER ///////////////////////////////////////////////

#define LAYOUT_LOCATION(N)      layout(location = N)
#define IN                      in
#define FLAT                    flat
#define OUT                     out

#define INT                     int

#define IS_RIGHT_SIDE           ((sides & 1) == 1)
#define IS_BOTTOM_SIDE          ((sides & 2) == 2)

#endif ////////////////////////////////////////////////////////////////

// ------
// INPUTS

// Texture associated to the graphic.
LAYOUT_LOCATION(0) IN INT textureId;

// Sides where the vertex is located.
//
// Bit 0 (LSB) is 0 for top and 1 for bottom.
// Bit 1 is 0 for left and 1 for right.
LAYOUT_LOCATION(1) IN INT sides;

// Column number in the grid where the left vertex is set.
LAYOUT_LOCATION(2) IN float column;

// Line number in the grid where the left vertex is set.
LAYOUT_LOCATION(3) IN float line;

// Height in pixels of the texture.
LAYOUT_LOCATION(4) IN float height;

// Width in pixels of the texture.
LAYOUT_LOCATION(5) IN float width;

// Offset in the X direction.
LAYOUT_LOCATION(6) IN float offsetX;

// Offset in the Y direction.
LAYOUT_LOCATION(7) IN float offsetY;

// Height in pixels of a single cell when the graphic was added.
LAYOUT_LOCATION(8) IN float baseCellHeight;

// -------
// OUTPUTS

// Texture sent to the fragment shader.
FLAT OUT INT texId;

// Coordinates sent to the fragment shader.
OUT vec2 texCoords;

// --------
// UNIFORMS

// Width and height of a single cell.
uniform vec2 cellDimensions;

// Width and height of the view.
uniform vec2 viewDimensions;

void main() {
    float scale = cellDimensions.y / baseCellHeight;
    float x = (column * cellDimensions.x - offsetX * scale) / (viewDimensions.x / 2.) - 1.;
    float y = -(line * cellDimensions.y - offsetY * scale) / (viewDimensions.y / 2.) + 1.;

    vec4 position = vec4(x, y, 0., 1.);
    vec2 coords = vec2(0., 0.);

    if(IS_RIGHT_SIDE) {
        position.x += scale * width / (viewDimensions.x / 2.);
        coords.x = 1.;
    }

    if(IS_BOTTOM_SIDE) {
        position.y += -scale * height / (viewDimensions.y / 2.);
        coords.y = 1.;
    }

    gl_Position = position;
    texCoords = coords;
    texId = textureId;
}
