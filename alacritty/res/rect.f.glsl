#version 330
precision mediump float;

flat in vec4 color;

out vec4 FragColor;

void main()
{
    FragColor = color;
}
