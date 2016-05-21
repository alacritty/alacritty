#version 330 core
in vec2 TexCoords;

layout(location = 0, index = 0) out vec4 color;
layout(location = 0, index = 1) out vec4 alphaMask;

uniform sampler2D mask;
uniform vec3 textColor;

void main()
{
    alphaMask = vec4(texture(mask, TexCoords).rgb, 1.);
    color = vec4(textColor, 1.);
}
