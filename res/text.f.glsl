#version 330 core
in vec2 TexCoords;

uniform sampler2D mask;
uniform vec3 textColor;
uniform vec3 bgColor;

// SRC = SRC_ALPHA; DST = 1 - SRC_ALPHA
void MyBlend(in vec3 srcValue,
             in vec3 dstValue,
             in vec3 srcAlpha,
             out vec3 blended)
{
    vec3 dstAlpha = vec3(1.0, 1.0, 1.0) - srcAlpha;
    vec3 preBlended = (srcValue * srcAlpha + dstValue * dstAlpha);

    blended = vec3(min(1.0, preBlended.x),
                   min(1.0, preBlended.y),
                   min(1.0, preBlended.z));
}

void main()
{
    // vec4 red = vec4(sampled.rgb, sampled.r * sampled.g * sampled.b);
    // vec4 sampled = vec4(1.0, 1.0, 1.0, texture(mask, TexCoords));
    vec3 blended = vec3(1.0, 1.0, 1.0);
    MyBlend(textColor, bgColor, texture(mask, TexCoords).rgb, blended);

    gl_FragColor = vec4(blended, 1.0);
}
