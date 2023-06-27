#version 330 core

out vec2 v_UV;
layout(location = 0) in vec2 i_Position;

uniform vec3 u_Matrix0;
uniform vec3 u_Matrix1;

void main()
{
    vec3 uv = vec3((i_Position + 1.0) / 2.0, 1.0);
    uv.y = 1.0 - uv.y;
    v_UV = vec2(dot(u_Matrix0, uv), dot(u_Matrix1, uv));
    gl_Position = vec4(i_Position.x, i_Position.y, 0.0f, 1.0f);
}
