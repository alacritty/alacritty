#if defined(GLES2_RENDERER) ///////////////////////////////////////////

precision highp float;

#define FLAT
#define IN                      varying

#define INT                     float

#define SWITCH(X)               int ind = int(X);
#define CASE(N)                 if (ind == N) {gl_FragColor = texture2D(textures[N], texCoords); return;}
#define DEFAULT(X)              X;
#define END_SWITCH

#else // GLSL3_RENDERER ///////////////////////////////////////////////

#define FLAT                    flat
#define IN                      in

#define INT                     int

#define SWITCH(X)               switch(X) {
#define CASE(N)                 case N: color = texture(textures[N], texCoords); break;
#define DEFAULT(X)              default: X;
#define END_SWITCH              }

out vec4 color;

#endif ////////////////////////////////////////////////////////////////

// Index in the textures[] uniform.
FLAT IN INT texId;

// Texture coordinates.
IN vec2 texCoords;

// Array with graphics data.
uniform sampler2D textures[16];

void main() {
    // The expression `textures[texId]` can't be used in OpenGL 3.3.
    // If we try to use it, the compiler throws this error:
    //
    //     sampler arrays indexed with non-constant expressions
    //     are forbidden in GLSL 1.30 and later
    //
    // To overcome this limitation we use a switch for every valid
    // value of `texId`.
    //
    // The first expression (`textures[texId]`) works with OpenGL 4.0
    // or later (using `#version 400 core`). If Alacritty drops support
    // for OpenGL 3.3, this switch block can be replaced with it.

    SWITCH(texId)
        CASE(0)
        CASE(1)
        CASE(2)
        CASE(3)
        CASE(4)
        CASE(5)
        CASE(6)
        CASE(7)
        CASE(8)
        CASE(9)
        CASE(10)
        CASE(11)
        CASE(12)
        CASE(13)
        CASE(14)
        CASE(15)
        DEFAULT(discard)
    END_SWITCH
}
