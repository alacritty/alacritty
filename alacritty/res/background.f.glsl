#version 330 core

layout(location = 0) out vec4 o_Color;
in vec2 v_UV;

uniform sampler2D u_Sampler;
uniform float u_Intensity;

void main()
{
    vec4 color = texture(u_Sampler, v_UV);
    o_Color = vec4(u_Intensity * color.rgb, 0.5f);
}
