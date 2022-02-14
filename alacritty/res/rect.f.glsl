#version 330 core

// We're using `origin_upper_left`, since we only known about padding from left
// and top. If we use default `origin_bottom_left` we won't be able to offset
// `gl_FragCoord` properly to align with the terminal grid.
layout(origin_upper_left) in vec4 gl_FragCoord;

flat in vec4 color;

out vec4 FragColor;

uniform int rectKind;

uniform float cellWidth;
uniform float cellHeight;
uniform float paddingY;
uniform float paddingX;

uniform float underlinePosition;
uniform float underlineThickness;

uniform float undercurlPosition;

#define UNDERCURL 1
#define DOTTED 2
#define DASHED 3

#define PI 3.1415926538

vec4 draw_undercurl(int x, int y) {
  // We use `undercurlPosition` as an amplitude, since it's half of the descent
  // value.
  float undercurl =
      -1. * undercurlPosition / 2. * cos(float(x) * 2 * PI / cellWidth) +
      cellHeight - undercurlPosition;

  float undercurlTop = undercurl + max((underlineThickness - 1), 0);
  float undercurlBottom = undercurl - max((underlineThickness - 1), 0);

  // Compute resulted alpha based on distance from `gl_FragCoord.y` to the
  // cosine curve.
  float alpha = 1.;
  if (y > undercurlTop || y < undercurlBottom) {
    alpha = 1. - min(abs(undercurlTop - y), abs(undercurlBottom - y));
  }

  // The result is an alpha mask on a rect, which leaves only curve opaque.
  return vec4(color.rgb, alpha);
}

// When the dot size increases we can use AA to make spacing look even and the
// dots rounded.
vec4 draw_dotted_aliased(float x, float y) {
  int dotNumber = int(x / underlineThickness);

  float radius = underlineThickness / 2.;
  float centerY = cellHeight - underlinePosition;

  float leftCenter = (dotNumber - dotNumber % 2) * underlineThickness + radius;
  float rightCenter = leftCenter + 2 * underlineThickness;

  float distanceLeft = sqrt(pow(x - leftCenter, 2) + pow(y - centerY, 2));
  float distanceRight = sqrt(pow(x - rightCenter, 2) + pow(y - centerY, 2));

  float alpha = max(1 - (min(distanceLeft, distanceRight) - radius), 0);
  return vec4(color.rgb, alpha);
}

/// Draw dotted line when dot is just a single pixel.
vec4 draw_dotted(int x, int y) {
  int cellEven = 0;

  // Since the size of the dot and its gap combined is 2px we should ensure that
  // spacing will be even. If the cellWidth is even it'll work since we start
  // with dot and end with gap. However if cellWidth is odd, the cell will start
  // and end with a dot, creating a dash. To resolve this issue, we invert the
  // pattern every two cells.
  if (int(cellWidth) % 2 != 0) {
    cellEven = int((gl_FragCoord.x - paddingX) / cellWidth) % 2;
  }

  // Since we use the entire descent area for dotted underlines, we limit its
  // height to a single pixel so we don't draw bars instead of dots.
  float alpha = 1. - abs(round(cellHeight - underlinePosition) - y);
  if (x % 2 != cellEven) {
    alpha = 0;
  }

  return vec4(color.rgb, alpha);
}

vec4 draw_dashed(int x) {
  // Since dashes of adjacent cells connect with each other our dash length is
  // half of the desired total length.
  int halfDashLen = int(cellWidth) / 4;

  float alpha = 1.;

  // Check if `x` coordinate is where we should draw gap.
  if (x > halfDashLen && x < cellWidth - halfDashLen - 1) {
    alpha = 0.;
  }

  return vec4(color.rgb, alpha);
}

void main() {
  int x = int(gl_FragCoord.x - paddingX) % int(cellWidth);
  int y = int(gl_FragCoord.y - paddingY) % int(cellHeight);

  switch (rectKind) {
  case UNDERCURL:
    FragColor = draw_undercurl(x, y);
    break;
  case DOTTED:
    if (underlineThickness < 2) {
      FragColor = draw_dotted(x, y);
    } else {
      FragColor = draw_dotted_aliased(x, y);
    }
    break;
  case DASHED:
    FragColor = draw_dashed(x);
    break;
  default:
    FragColor = color;
    break;
  }
}
