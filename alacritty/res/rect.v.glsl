#if defined(GLES2_RENDERER)
attribute vec2 aPos;
attribute vec4 aColor;

varying mediump vec4 color;
#else
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec4 aColor;

flat out vec4 color;
#endif

void main() {
    color = aColor;
    gl_Position = vec4(aPos.x, aPos.y, 0.0, 1.0);
}
