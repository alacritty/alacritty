varying mediump vec2 TexCoords;
varying mediump vec3 fg;
varying highp float colored;
varying mediump vec4 bg;

uniform highp int renderingPass;
uniform sampler2D mask;

#define COLORED 1

mediump float max_rgb(mediump vec3 mask) {
    return max(max(mask.r, mask.g), mask.b);
}

void render_text() {
    mediump vec4 mask = texture2D(mask, TexCoords);
    mediump float m_rgb = max_rgb(mask.rgb);

    if (renderingPass == 1) {
        gl_FragColor = vec4(mask.rgb, m_rgb);
    } else if (renderingPass == 2) {
        gl_FragColor = bg * (vec4(m_rgb) - vec4(mask.rgb, m_rgb));
    } else {
        gl_FragColor = vec4(fg, 1.) * vec4(mask.rgb, m_rgb);
    }
}

// Render colored bitmaps.
void render_bitmap() {
    if (renderingPass == 2) {
        discard;
    }
    mediump vec4 mask = texture2D(mask, TexCoords);
    if (renderingPass == 1) {
        gl_FragColor = mask.aaaa;
    } else {
        gl_FragColor = mask;
    }
}

void main() {
    // Handle background pass drawing before anything else.
    if (renderingPass == 0) {
        if (bg.a == 0.0) {
            discard;
        }

        gl_FragColor = vec4(bg.rgb * bg.a, bg.a);
        return;
    }

    if (int(colored) == COLORED) {
        render_bitmap();
    } else {
        render_text();
    }
}
