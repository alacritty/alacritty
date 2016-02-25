#version 330 core
in vec2 TexCoords;

uniform sampler2D mask;
uniform vec3 textColor;

void main()
{
    vec4 sampled = vec4(1.0, 1.0, 1.0, texture(mask, TexCoords).r);
    gl_FragColor = vec4(textColor, 1.0) * sampled;
}
