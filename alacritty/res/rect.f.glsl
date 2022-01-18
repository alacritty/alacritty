#version 330 core

// We're using `origin_upper_left`, since we only known about padding from left
// and top. If we use default `origin_bottom_left` we won't be able to offset
// `gl_FragCoord` properly to align with the terminal grid.
layout(origin_upper_left) in vec4 gl_FragCoord;

flat in vec4 color;

out vec4 FragColor;

uniform int isUndercurl;

uniform float cellWidth;
uniform float cellHeight;
uniform float paddingY;
uniform float paddingX;

uniform float undercurlThickness;
uniform float undercurlPosition;

#define PI 3.1415926538

void main()
{
    if (isUndercurl == 0) {
        FragColor = color;
        return;
    }

    int x = int(gl_FragCoord.x - paddingX) % int(cellWidth);
    int y = int(gl_FragCoord.y - paddingY) % int(cellHeight);

    // We use `undercurlPosition` as amplitude, since it's half of the descent
    // value.
    float undercurl = -1. * undercurlPosition / 2.
        * cos(float(x) * 2 * PI / float(cellWidth))
        + cellHeight - undercurlPosition;

    float undercurl_top = undercurl + undercurlThickness / 2.;
    float undercurl_bottom = undercurl - undercurlThickness / 2.;


    // Compute resulted alpha based on distance from `gl_FragCoord.y` to the
    // cosine curve.
    float alpha = 1.;
    if (y > undercurl_top || y < undercurl_bottom) {
        alpha = 1. - min(abs(undercurl_top - y), abs(undercurl_bottom - y));
    }

    // The result is an alpha mask on a rect, which leaves only curve opaque.
    FragColor = vec4(color.xyz, alpha);
}
