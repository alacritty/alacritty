#version 330 core
in vec2 TexCoords;
in vec3 fg;
in vec3 bg;
flat in int background;

layout(location = 0, index = 0) out vec4 color;
layout(location = 0, index = 1) out vec4 alphaMask;

uniform sampler2D mask;

void main()
{
    if (background != 0) {
        alphaMask = vec4(1.0, 1.0, 1.0, 1.0);
        color = vec4(bg, 1.0);
    } else {
        alphaMask = vec4(texture(mask, TexCoords).rgb, 1.0);
        color = vec4(fg, 1.0);
    }

}
