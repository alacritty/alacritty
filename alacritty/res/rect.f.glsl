#if defined(GLES2_RENDERER)
#define float_t mediump float
#define color_t mediump vec4
#define FRAG_COLOR gl_FragColor

varying color_t color;

#else
#define float_t float
#define color_t vec4

out vec4 FragColor;
#define FRAG_COLOR FragColor

flat in color_t color;

#endif

uniform int rectKind;
uniform float_t cellWidth;
uniform float_t cellHeight;
uniform float_t paddingY;
uniform float_t paddingX;

uniform float_t underlinePosition;
uniform float_t underlineThickness;

uniform float_t undercurlPosition;

#define UNDERCURL 1
#define DOTTED 2
#define DASHED 3

#define PI 3.1415926538

color_t draw_undercurl(float_t x, float_t y) {
  // We use `undercurlPosition` as an amplitude, since it's half of the descent
  // value.
  //
  // The `x` represents the left bound of pixel we should add `1/2` to it, so
  // we compute the undercurl position for the center of the pixel.
  float_t undercurl = undercurlPosition / 2. * cos((x + 0.5) * 2.
                    * PI / cellWidth) + undercurlPosition - 1.;

  float_t undercurlTop = undercurl + max((underlineThickness - 1.), 0.) / 2.;
  float_t undercurlBottom = undercurl - max((underlineThickness - 1.), 0.) / 2.;

  // Compute resulted alpha based on distance from `gl_FragCoord.y` to the
  // cosine curve.
  float_t alpha = 1.;
  if (y > undercurlTop || y < undercurlBottom) {
    alpha = 1. - min(abs(undercurlTop - y), abs(undercurlBottom - y));
  }

  // The result is an alpha mask on a rect, which leaves only curve opaque.
  return vec4(color.rgb, alpha);
}

// When the dot size increases we can use AA to make spacing look even and the
// dots rounded.
color_t draw_dotted_aliased(float_t x, float_t y) {
  float_t dotNumber = floor(x / underlineThickness);

  float_t radius = underlineThickness / 2.;
  float_t centerY = underlinePosition - 1.;

  float_t leftCenter = (dotNumber - mod(dotNumber, 2.)) * underlineThickness + radius;
  float_t rightCenter = leftCenter + 2. * underlineThickness;

  float_t distanceLeft = sqrt(pow(x - leftCenter, 2.) + pow(y - centerY, 2.));
  float_t distanceRight = sqrt(pow(x - rightCenter, 2.) + pow(y - centerY, 2.));

  float_t alpha = max(1. - (min(distanceLeft, distanceRight) - radius), 0.);
  return vec4(color.rgb, alpha);
}

/// Draw dotted line when dot is just a single pixel.
color_t draw_dotted(float_t x, float_t y) {
  float_t cellEven = 0.;

  // Since the size of the dot and its gap combined is 2px we should ensure that
  // spacing will be even. If the cellWidth is even it'll work since we start
  // with dot and end with gap. However if cellWidth is odd, the cell will start
  // and end with a dot, creating a dash. To resolve this issue, we invert the
  // pattern every two cells.
  if (int(mod(cellWidth, 2.)) != 0) {
    cellEven = mod((gl_FragCoord.x - paddingX) / cellWidth, 2.);
  }

  // Since we use the entire descent area for dotted underlines, we limit its
  // height to a single pixel so we don't draw bars instead of dots.
  float_t alpha = 1. - abs(floor(underlinePosition) - y);
  if (int(mod(x, 2.)) != int(cellEven)) {
    alpha = 0.;
  }

  return vec4(color.rgb, alpha);
}

color_t draw_dashed(float_t x) {
  // Since dashes of adjacent cells connect with each other our dash length is
  // half of the desired total length.
  float_t halfDashLen = floor(cellWidth / 4. + 0.5);

  float_t alpha = 1.;

  // Check if `x` coordinate is where we should draw gap.
  if (x > halfDashLen - 1. && x < cellWidth - halfDashLen) {
    alpha = 0.;
  }

  return vec4(color.rgb, alpha);
}

void main() {
  float_t x = floor(mod(gl_FragCoord.x - paddingX, cellWidth));
  float_t y = floor(mod(gl_FragCoord.y - paddingY, cellHeight));

  if (rectKind == UNDERCURL) {
    FRAG_COLOR = draw_undercurl(x, y);
  } else if (rectKind == DOTTED) {
    if (underlineThickness < 2.) {
      FRAG_COLOR = draw_dotted(x, y);
    } else {
      FRAG_COLOR = draw_dotted_aliased(x, y);
    }
  } else if (rectKind == DASHED) {
    FRAG_COLOR = draw_dashed(x);
  } else {
    FRAG_COLOR = color;
  }
}
