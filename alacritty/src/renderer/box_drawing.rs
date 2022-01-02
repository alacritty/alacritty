//! Hand rolled drawing of unicode [box drawing](http://www.unicode.org/charts/PDF/U2500.pdf)
//! and [block elements](https://www.unicode.org/charts/PDF/U2580.pdf).

use std::cmp::{max, min};

use crossfont::{BitmapBuffer, Metrics, RasterizedGlyph};

/// Stroke size of a line as a part of the cell width.
const LINE_STROKE_SIZE: usize = 6;

/// Stroke size of a heavy line as a part of the cell width.
const HEAVY_LINE_STROKE_SIZE: usize = 3;

// Colors which are used for filling shade variants.
const COLOR_FILL_ALPHA_STEP_1: RGBPixel = RGBPixel { _r: 50, _g: 50, _b: 50 };
const COLOR_FILL_ALPHA_STEP_2: RGBPixel = RGBPixel { _r: 125, _g: 125, _b: 125 };
const COLOR_FILL_ALPHA_STEP_3: RGBPixel = RGBPixel { _r: 200, _g: 200, _b: 200 };

/// Default color used for filling.
const COLOR_FILL: RGBPixel = RGBPixel { _r: 255, _g: 255, _b: 255 };

/// Returns `Some(RasterizedGlyph)` if character could be drawn with Alacritty's builtin set of
/// glyphs otherwise `None`.
pub fn builtin_glyph(character: char, metrics: &Metrics) -> Option<RasterizedGlyph> {
    match character {
        // Box drawing characters and block elements.
        '\u{2500}'..='\u{259f}' => Some(box_drawing(character, metrics)),
        _ => None,
    }
}

fn box_drawing(character: char, metrics: &Metrics) -> RasterizedGlyph {
    let height = metrics.line_height as usize;
    let width = metrics.average_advance as usize;
    let stroke_size = max(width / LINE_STROKE_SIZE, 1);
    let heavy_stroke_size = max(width / HEAVY_LINE_STROKE_SIZE, 1);
    let mut canvas = Canvas::new(width, height);

    match character {
        // Horizonatal dashes: '┄', '┅', '┈', '┉', '╌', '╍'.
        '\u{2504}' | '\u{2505}' | '\u{2508}' | '\u{2509}' | '\u{254c}' | '\u{254d}' => {
            let (num_gaps, stroke_size) = match character {
                '\u{2504}' => (2, stroke_size),
                '\u{2505}' => (2, heavy_stroke_size),
                '\u{2508}' => (3, stroke_size),
                '\u{2509}' => (3, heavy_stroke_size),
                '\u{254c}' => (1, stroke_size),
                '\u{254d}' => (1, heavy_stroke_size),
                _ => unreachable!(),
            };

            let dash_gap_len = max(width / 8, 1);
            let dash_len = max(width.saturating_sub(dash_gap_len * num_gaps) / (num_gaps + 1), 1);
            let y = canvas.center_v();
            for gap in 0..=num_gaps {
                let x = min(gap * (dash_len + dash_gap_len), width);
                canvas.draw_h_line(x as f32, y, dash_len as f32, stroke_size);
            }
        },
        // Vertical dashes: '┆', '┇', '┊', '┋', '╎', '╏'.
        '\u{2506}' | '\u{2507}' | '\u{250a}' | '\u{250b}' | '\u{254e}' | '\u{254f}' => {
            let (num_gaps, stroke_size) = match character {
                '\u{2506}' => (2, stroke_size),
                '\u{2507}' => (2, heavy_stroke_size),
                '\u{250a}' => (3, stroke_size),
                '\u{250b}' => (3, heavy_stroke_size),
                '\u{254e}' => (1, stroke_size),
                '\u{254f}' => (1, heavy_stroke_size),
                _ => unreachable!(),
            };

            let dash_gap_len = max(height / 8, 1);
            let dash_len = max(height.saturating_sub(dash_gap_len * num_gaps) / (num_gaps + 1), 1);
            let x = canvas.center_h();
            for gap in 0..=num_gaps {
                let y = min(gap * (dash_len + dash_gap_len), height);
                canvas.draw_v_line(x, y as f32, dash_len as f32, stroke_size);
            }
        },
        // Horizontal lines: '─', '━', '╴', '╶', '╸', '╺'.
        // Vertical lines: '│', '┃', '╵', '╷', '╹', '╻'.
        // Light and heavy line box components:
        // '┌','┍','┎','┏','┐','┑','┒','┓','└','┕','┖','┗','┘','┙','┚','┛',├','┝','┞','┟','┠','┡',
        // '┢','┣','┤','┥','┦','┧','┨','┩','┪','┫','┬','┭','┮','┯','┰','┱','┲','┳','┴','┵','┶','┷',
        // '┸','┹','┺','┻','┼','┽','┾','┿','╀','╁','╂','╃','╄','╅','╆','╇','╈','╉','╊','╋'.
        // Mixed light and heavy lines: '╼', '╽', '╾', '╿'.
        '\u{2500}'..='\u{2503}' | '\u{250c}'..='\u{254b}' | '\u{2574}'..='\u{257f}' => {
            // Left horizontal line.
            let stroke_size_h1 = match character {
                '\u{2500}' | '\u{2510}' | '\u{2512}' | '\u{2518}' | '\u{251a}' | '\u{2524}'
                | '\u{2526}' | '\u{2527}' | '\u{2528}' | '\u{252c}' | '\u{252e}' | '\u{2530}'
                | '\u{2532}' | '\u{2534}' | '\u{2536}' | '\u{2538}' | '\u{253a}' | '\u{253c}'
                | '\u{253e}' | '\u{2540}' | '\u{2541}' | '\u{2542}' | '\u{2544}' | '\u{2546}'
                | '\u{254a}' | '\u{2574}' | '\u{257c}' => stroke_size,
                '\u{2501}' | '\u{2511}' | '\u{2513}' | '\u{2519}' | '\u{251b}' | '\u{2525}'
                | '\u{2529}' | '\u{252a}' | '\u{252b}' | '\u{252d}' | '\u{252f}' | '\u{2531}'
                | '\u{2533}' | '\u{2535}' | '\u{2537}' | '\u{2539}' | '\u{253b}' | '\u{253d}'
                | '\u{253f}' | '\u{2543}' | '\u{2545}' | '\u{2547}' | '\u{2548}' | '\u{2549}'
                | '\u{254b}' | '\u{2578}' | '\u{257e}' => heavy_stroke_size,
                _ => 0,
            };
            // Right horizontal line.
            let stroke_size_h2 = match character {
                '\u{2500}' | '\u{250c}' | '\u{250e}' | '\u{2514}' | '\u{2516}' | '\u{251c}'
                | '\u{251e}' | '\u{251f}' | '\u{2520}' | '\u{252c}' | '\u{252d}' | '\u{2530}'
                | '\u{2531}' | '\u{2534}' | '\u{2535}' | '\u{2538}' | '\u{2539}' | '\u{253c}'
                | '\u{253d}' | '\u{2540}' | '\u{2541}' | '\u{2542}' | '\u{2543}' | '\u{2545}'
                | '\u{2549}' | '\u{2576}' | '\u{257e}' => stroke_size,
                '\u{2501}' | '\u{250d}' | '\u{250f}' | '\u{2515}' | '\u{2517}' | '\u{251d}'
                | '\u{2521}' | '\u{2522}' | '\u{2523}' | '\u{252e}' | '\u{252f}' | '\u{2532}'
                | '\u{2533}' | '\u{2536}' | '\u{2537}' | '\u{253a}' | '\u{253b}' | '\u{253e}'
                | '\u{253f}' | '\u{2544}' | '\u{2546}' | '\u{2547}' | '\u{2548}' | '\u{254a}'
                | '\u{254b}' | '\u{257a}' | '\u{257c}' => heavy_stroke_size,
                _ => 0,
            };
            // Top vertical line.
            let stroke_size_v1 = match character {
                '\u{2502}' | '\u{2514}' | '\u{2515}' | '\u{2518}' | '\u{2519}' | '\u{251c}'
                | '\u{251d}' | '\u{251f}' | '\u{2522}' | '\u{2524}' | '\u{2525}' | '\u{2527}'
                | '\u{252a}' | '\u{2534}' | '\u{2535}' | '\u{2536}' | '\u{2537}' | '\u{253c}'
                | '\u{253d}' | '\u{253e}' | '\u{253f}' | '\u{2541}' | '\u{2545}' | '\u{2546}'
                | '\u{2548}' | '\u{2575}' | '\u{257d}' => stroke_size,
                '\u{2503}' | '\u{2516}' | '\u{2517}' | '\u{251a}' | '\u{251b}' | '\u{251e}'
                | '\u{2520}' | '\u{2521}' | '\u{2523}' | '\u{2526}' | '\u{2528}' | '\u{2529}'
                | '\u{252b}' | '\u{2538}' | '\u{2539}' | '\u{253a}' | '\u{253b}' | '\u{2540}'
                | '\u{2542}' | '\u{2543}' | '\u{2544}' | '\u{2547}' | '\u{2549}' | '\u{254a}'
                | '\u{254b}' | '\u{2579}' | '\u{257f}' => heavy_stroke_size,
                _ => 0,
            };
            // Bottom vertical line.
            let stroke_size_v2 = match character {
                '\u{2502}' | '\u{250c}' | '\u{250d}' | '\u{2510}' | '\u{2511}' | '\u{251c}'
                | '\u{251d}' | '\u{251e}' | '\u{2521}' | '\u{2524}' | '\u{2525}' | '\u{2526}'
                | '\u{2529}' | '\u{252c}' | '\u{252d}' | '\u{252e}' | '\u{252f}' | '\u{253c}'
                | '\u{253d}' | '\u{253e}' | '\u{253f}' | '\u{2540}' | '\u{2543}' | '\u{2544}'
                | '\u{2547}' | '\u{2577}' | '\u{257f}' => stroke_size,
                '\u{2503}' | '\u{250e}' | '\u{250f}' | '\u{2512}' | '\u{2513}' | '\u{251f}'
                | '\u{2520}' | '\u{2522}' | '\u{2523}' | '\u{2527}' | '\u{2528}' | '\u{252a}'
                | '\u{252b}' | '\u{2530}' | '\u{2531}' | '\u{2532}' | '\u{2533}' | '\u{2541}'
                | '\u{2542}' | '\u{2545}' | '\u{2546}' | '\u{2548}' | '\u{2549}' | '\u{254a}'
                | '\u{254b}' | '\u{257b}' | '\u{257d}' => heavy_stroke_size,
                _ => 0,
            };

            let x_v = canvas.center_h();
            let y_h = canvas.center_v();

            let v_line_bounds_top = canvas.v_line_bounds(x_v, stroke_size_v1);
            let v_line_bounds_bot = canvas.v_line_bounds(x_v, stroke_size_v2);
            let h_line_bounds_left = canvas.h_line_bounds(y_h, stroke_size_h1);
            let h_line_bounds_right = canvas.h_line_bounds(y_h, stroke_size_h2);

            let size_h1 = max(v_line_bounds_top.1 as i32, v_line_bounds_bot.1 as i32) as f32;
            let x_h = min(v_line_bounds_top.0 as i32, v_line_bounds_bot.0 as i32) as f32;
            let size_h2 = width as f32 - x_h;

            let size_v1 = max(h_line_bounds_left.1 as i32, h_line_bounds_right.1 as i32) as f32;
            let y_v = min(h_line_bounds_left.0 as i32, h_line_bounds_right.0 as i32) as f32;
            let size_v2 = height as f32 - y_v;

            // Left horizontal line.
            canvas.draw_h_line(0., y_h, size_h1, stroke_size_h1);
            // Right horizontal line.
            canvas.draw_h_line(x_h, y_h, size_h2, stroke_size_h2);
            // Top vertical line.
            canvas.draw_v_line(x_v, 0., size_v1, stroke_size_v1);
            // Bottom vertical line.
            canvas.draw_v_line(x_v, y_v, size_v2, stroke_size_v2);
        },
        // Light and double line box components:
        // '═','║','╒','╓','╔','╕','╖','╗','╘','╙','╚','╛','╜','╝','╞','╟','╠','╡','╢','╣','╤','╥',
        // '╦','╧','╨','╩','╪','╫','╬'.
        '\u{2550}'..='\u{256c}' => {
            let v_lines = match character {
                '\u{2552}' | '\u{2555}' | '\u{2558}' | '\u{255b}' | '\u{255e}' | '\u{2561}'
                | '\u{2564}' | '\u{2567}' | '\u{256a}' => (canvas.center_h(), canvas.center_h()),
                _ => {
                    let v_line_bounds = canvas.v_line_bounds(canvas.center_h(), stroke_size);
                    let left_line = max(v_line_bounds.0 as i32 - 1, 0) as f32;
                    let right_line = min(v_line_bounds.1 as i32 + 1, width as i32) as f32;

                    (left_line, right_line)
                },
            };
            let h_lines = match character {
                '\u{2553}' | '\u{2556}' | '\u{2559}' | '\u{255c}' | '\u{255f}' | '\u{2562}'
                | '\u{2565}' | '\u{2568}' | '\u{256b}' => (canvas.center_v(), canvas.center_v()),
                _ => {
                    let h_line_bounds = canvas.h_line_bounds(canvas.center_v(), stroke_size);
                    let top_line = max(h_line_bounds.0 as i32 - 1, 0) as f32;
                    let bottom_line = min(h_line_bounds.1 as i32 + 1, height as i32) as f32;

                    (top_line, bottom_line)
                },
            };

            // Get bounds for each double line we could have.
            let v_left_bounds = canvas.v_line_bounds(v_lines.0, stroke_size);
            let v_right_bounds = canvas.v_line_bounds(v_lines.1, stroke_size);
            let h_top_bounds = canvas.h_line_bounds(h_lines.0, stroke_size);
            let h_bot_bounds = canvas.h_line_bounds(h_lines.1, stroke_size);

            let height = height as f32;
            let width = width as f32;

            // Left horizontal part.
            let (top_left_size, bot_left_size) = match character {
                '\u{2550}' | '\u{256b}' => (canvas.center_h(), canvas.center_h()),
                '\u{2555}'..='\u{2557}' => (v_right_bounds.1, v_left_bounds.1),
                '\u{255b}'..='\u{255d}' => (v_left_bounds.1, v_right_bounds.1),
                '\u{2561}'..='\u{2563}' | '\u{256a}' | '\u{256c}' => {
                    (v_left_bounds.1, v_left_bounds.1)
                },
                '\u{2564}'..='\u{2566}' => (canvas.center_h(), v_left_bounds.1),
                '\u{2569}'..='\u{2569}' => (v_left_bounds.1, canvas.center_h()),
                _ => (0., 0.),
            };

            // Right horizontal part.
            let (top_right_x, bot_right_x, right_size) = match character {
                '\u{2550}' | '\u{2565}' | '\u{256b}' => {
                    (canvas.center_h(), canvas.center_h(), width)
                },
                '\u{2552}'..='\u{2554}' | '\u{2568}' => (v_left_bounds.0, v_right_bounds.0, width),
                '\u{2558}'..='\u{255a}' => (v_right_bounds.0, v_left_bounds.0, width),
                '\u{255e}'..='\u{2560}' | '\u{256a}' | '\u{256c}' => {
                    (v_right_bounds.0, v_right_bounds.0, width)
                },
                '\u{2564}' | '\u{2566}' => (canvas.center_h(), v_right_bounds.0, width),
                '\u{2567}' | '\u{2569}' => (v_right_bounds.0, canvas.center_h(), width),
                _ => (0., 0., 0.),
            };

            // Top vertical part.
            let (left_top_size, right_top_size) = match character {
                '\u{2551}' | '\u{256a}' => (canvas.center_v(), canvas.center_v()),
                '\u{2558}'..='\u{255c}' | '\u{2567}' | '\u{2568}' => {
                    (h_bot_bounds.1, h_top_bounds.1)
                },
                '\u{255d}' => (h_top_bounds.1, h_bot_bounds.1),
                '\u{255e}'..='\u{2560}' => (canvas.center_v(), h_top_bounds.1),
                '\u{2561}'..='\u{2563}' => (h_top_bounds.1, canvas.center_v()),
                '\u{2569}' | '\u{256b}' | '\u{256c}' => (h_top_bounds.1, h_top_bounds.1),
                _ => (0., 0.),
            };

            // Bottom vertical part.
            let (left_bot_y, right_bot_y, bottom_size) = match character {
                '\u{2551}' | '\u{256a}' => (canvas.center_v(), canvas.center_v(), height),
                '\u{2552}'..='\u{2554}' => (h_top_bounds.0, h_bot_bounds.0, height),
                '\u{2555}'..='\u{2557}' => (h_bot_bounds.0, h_top_bounds.0, height),
                '\u{255e}'..='\u{2560}' => (canvas.center_v(), h_bot_bounds.0, height),
                '\u{2561}'..='\u{2563}' => (h_bot_bounds.0, canvas.center_v(), height),
                '\u{2564}'..='\u{2566}' | '\u{256b}' | '\u{256c}' => {
                    (h_bot_bounds.0, h_bot_bounds.0, height)
                },
                _ => (0., 0., 0.),
            };

            // Left horizontal line.
            canvas.draw_h_line(0., h_lines.0, top_left_size, stroke_size);
            canvas.draw_h_line(0., h_lines.1, bot_left_size, stroke_size);

            // Right horizontal line.
            canvas.draw_h_line(top_right_x, h_lines.0, right_size, stroke_size);
            canvas.draw_h_line(bot_right_x, h_lines.1, right_size, stroke_size);

            // Top vertical line.
            canvas.draw_v_line(v_lines.0, 0., left_top_size, stroke_size);
            canvas.draw_v_line(v_lines.1, 0., right_top_size, stroke_size);

            // Bottom vertical line.
            canvas.draw_v_line(v_lines.0, left_bot_y, bottom_size, stroke_size);
            canvas.draw_v_line(v_lines.1, right_bot_y, bottom_size, stroke_size);
        },
        // Arcs: '╭', '╮', '╯', '╰'.
        '\u{256d}' | '\u{256e}' | '\u{256f}' | '\u{2570}' => {
            canvas.draw_arc_centered(stroke_size);
            // Mirror `X` axis.
            if character == '\u{256d}' || character == '\u{2570}' {
                let center = canvas.center_h() as usize;
                for y in 1..height {
                    let left = (y - 1) * width;
                    let right = y * width - 1;
                    for offset in 0..center {
                        canvas.buffer_mut().swap(left + offset, right - offset);
                    }
                }
            }
            // Mirror `Y` axis.
            if character == '\u{256d}' || character == '\u{256e}' {
                let center = canvas.center_v() as usize;
                for offset in 1..=center {
                    let top_row = (offset - 1) * width;
                    let bottom_row = (height - offset) * width;
                    for index in 0..width {
                        canvas.buffer_mut().swap(top_row + index, bottom_row + index);
                    }
                }
            }
        },
        // Diagonals: '╱', '╲', '╳'.
        '\u{2571}' | '\u{2572}' | '\u{2573}' => {
            let width = width as f32;
            let height = height as f32;
            for stroke_size in 0..=stroke_size {
                let stroke_size = stroke_size as f32 / 2.;
                if character == '\u{2571}' || character == '\u{2573}' {
                    canvas.draw_line(stroke_size, height - 1., width - 1., stroke_size);
                    canvas.draw_line(0., height - 1. - stroke_size, width - 1. - stroke_size, 0.);
                }
                if character == '\u{2572}' || character == '\u{2573}' {
                    canvas.draw_line(stroke_size, 0., width - 1., height - 1. - stroke_size);
                    canvas.draw_line(0., stroke_size, width - 1. - stroke_size, height - 1.);
                }
            }
        },
        // Parts of full block: '▀', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '▔', '▉', '▊', '▋', '▌',
        // '▍', '▎', '▏', '▐', '▕'.
        '\u{2580}'..='\u{2587}' | '\u{2589}'..='\u{2590}' | '\u{2594}' | '\u{2595}' => {
            let width = width as f32;
            let height = height as f32;
            let rect_width = match character {
                '\u{2589}' => width * 7. / 8.,
                '\u{258a}' => width * 3. / 4.,
                '\u{258b}' => width * 5. / 8.,
                '\u{258c}' | '\u{2590}' => width / 2.,
                '\u{258d}' => width * 3. / 8.,
                '\u{258e}' => width / 4.,
                '\u{258f}' | '\u{2595}' => width / 8.,
                _ => width,
            };
            let (rect_height, y) = match character {
                // Upper half.
                '\u{2580}' => (height / 2., height),
                // One eight.
                '\u{2581}' => (height / 8., height / 8.),
                // Quarter.
                '\u{2582}' => (height / 4., height / 4.),
                // Three eights.
                '\u{2583}' => (height * 3. / 8., height * 3. / 8.),
                // Lower half.
                '\u{2584}' => (height / 2., canvas.center_v()),
                // Five eights.
                '\u{2585}' => (height * 5. / 8., height * 5. / 8.),
                // Three quarters.
                '\u{2586}' => (height * 3. / 4., height * 3. / 4.),
                // Seven eights.
                '\u{2587}' => (height * 7. / 8., height * 7. / 8.),
                // Upper one eight.
                '\u{2594}' => (height / 8., height),
                _ => (height, height),
            };
            // Fixup `y` coordinates.
            let y = height - y;

            let x = match character {
                '\u{2590}' => canvas.center_h(),
                '\u{2595}' => width as f32 - width / 8.,
                _ => 0.,
            };

            canvas.draw_rect(x, y, rect_width, rect_height, COLOR_FILL);
        },
        // Shades: '░', '▒', '▓', '█'.
        '\u{2588}' | '\u{2591}' | '\u{2592}' | '\u{2593}' => {
            let color = match character {
                '\u{2588}' => COLOR_FILL,
                '\u{2591}' => COLOR_FILL_ALPHA_STEP_1,
                '\u{2592}' => COLOR_FILL_ALPHA_STEP_2,
                '\u{2593}' => COLOR_FILL_ALPHA_STEP_3,
                _ => unreachable!(),
            };
            canvas.fill(color);
        },
        // Quadrants: '▖', '▗', '▘', '▙', '▚', '▛', '▜', '▝', '▞', '▟'.
        '\u{2596}'..='\u{259F}' => {
            let (w_second, h_second) = match character {
                '\u{2598}' | '\u{2599}' | '\u{259a}' | '\u{259b}' | '\u{259c}' => {
                    (canvas.center_h(), canvas.center_v())
                },
                _ => (0., 0.),
            };
            let (w_first, h_first) = match character {
                '\u{259b}' | '\u{259c}' | '\u{259d}' | '\u{259e}' | '\u{259f}' => {
                    (canvas.center_h(), canvas.center_v())
                },
                _ => (0., 0.),
            };
            let (w_third, h_third) = match character {
                '\u{2596}' | '\u{2599}' | '\u{259b}' | '\u{259e}' | '\u{259f}' => {
                    (canvas.center_h(), canvas.center_v())
                },
                _ => (0., 0.),
            };
            let (w_fourth, h_fourth) = match character {
                '\u{2597}' | '\u{2599}' | '\u{259a}' | '\u{259c}' | '\u{259f}' => {
                    (canvas.center_h(), canvas.center_v())
                },
                _ => (0., 0.),
            };

            // Second quadrant.
            canvas.draw_rect(0., 0., w_second, h_second, COLOR_FILL);
            // First quadrant.
            canvas.draw_rect(canvas.center_h(), 0., w_first, h_first, COLOR_FILL);
            // Third quadrant.
            canvas.draw_rect(0., canvas.center_v(), w_third, h_third, COLOR_FILL);
            // Fourth quadrant.
            canvas.draw_rect(canvas.center_h(), canvas.center_v(), w_fourth, h_fourth, COLOR_FILL);
        },
        _ => unreachable!(),
    }

    let top = height as i32 + metrics.descent as i32;
    let buffer = BitmapBuffer::Rgb(canvas.into_raw_buffer());
    RasterizedGlyph { character, top, left: 0, height: height as i32, width: width as i32, buffer }
}

#[repr(packed)]
#[derive(Clone, Copy, Debug, Default)]
struct RGBPixel {
    _r: u8,
    _g: u8,
    _b: u8,
}

/// Canvas which is used for simple line drawing operations.
struct Canvas {
    /// Canvas width.
    width: usize,

    /// Canvas height.
    height: usize,

    /// Canvas buffer we draw on.
    buffer: Vec<RGBPixel>,
}

impl Canvas {
    /// Builds new `Canvas` for line drawing with the given `width` and `height` with default color.
    fn new(width: usize, height: usize) -> Self {
        let buffer = vec![RGBPixel::default(); width * height];
        Self { width, height, buffer }
    }

    /// Vertical center of the `Canvas`.
    fn center_v(&self) -> f32 {
        self.height as f32 / 2.
    }

    /// Horizontal center of the `Canvas`.
    fn center_h(&self) -> f32 {
        self.width as f32 / 2.
    }

    /// Canvas underlying buffer for direct manipulation
    fn buffer_mut(&mut self) -> &mut [RGBPixel] {
        &mut self.buffer
    }

    /// Gives bounds for horizontal straight line on `y` with `stroke_size`.
    fn h_line_bounds(&self, y: f32, stroke_size: usize) -> (f32, f32) {
        let start_y = max((y - stroke_size as f32 / 2.) as i32, 0) as f32;
        let end_y = min((y + stroke_size as f32 / 2.) as i32, self.height as i32) as f32;

        (start_y, end_y)
    }

    /// Gives bounds for vertical straight line on `y` with `stroke_size`.
    fn v_line_bounds(&self, x: f32, stroke_size: usize) -> (f32, f32) {
        let start_x = max((x - stroke_size as f32 / 2.) as i32, 0) as f32;
        let end_x = min((x + stroke_size as f32 / 2.) as i32, self.width as i32) as f32;

        (start_x, end_x)
    }

    /// Draws a horizontal straight line from (`x`, `y`) of `size` with the given `stroke_size`.
    fn draw_h_line(&mut self, x: f32, y: f32, size: f32, stroke_size: usize) {
        let (start_y, end_y) = self.h_line_bounds(y, stroke_size);
        self.draw_rect(x, start_y as f32, size, (end_y - start_y) as f32, COLOR_FILL);
    }

    /// Draws a vertical straight line from (`x`, `y`) of `size` with the given `stroke_size`.
    fn draw_v_line(&mut self, x: f32, y: f32, size: f32, stroke_size: usize) {
        let (start_x, end_x) = self.v_line_bounds(x, stroke_size);
        self.draw_rect(start_x as f32, y, (end_x - start_x) as f32, size, COLOR_FILL);
    }

    /// Draws a rect from the (`x`, `y`) of the given `width` and `height` using `color`.
    fn draw_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: RGBPixel) {
        let start_x = x as usize;
        let end_x = min((x + width) as usize, self.width);
        let start_y = y as usize;
        let end_y = min((y + height) as usize, self.height);
        for y in start_y..end_y {
            let y = y * self.width;
            self.buffer[start_x + y..end_x + y].fill(color);
        }
    }

    /// Naive arbitrary line drawing from (`from_x`, `from_y`) to (`to_x`, `to_y`).
    fn draw_line(&mut self, from_x: f32, from_y: f32, to_x: f32, to_y: f32) {
        let d_x = to_x - from_x;
        let d_y = to_y - from_y;
        for x in from_x as usize..=to_x as usize {
            let y = from_y + d_y * (x as f32 - from_x) / d_x;
            let y = y.clamp(0., self.height as f32 - 1.);
            let index = min(x + y as usize * self.width, self.buffer.len() - 1);
            self.buffer[index] = COLOR_FILL;
        }
    }

    /// Draws an arc from `(0, self.center_v())` to `(self.center_h(), 0)`.
    ///
    /// You can mirror Arc in whichever direction you'd like.
    fn draw_arc_centered(&mut self, stroke_size: usize) {
        let v_line_bounds = self.v_line_bounds(self.center_h(), stroke_size);
        let v_line_bounds = (v_line_bounds.0 as usize + 1, v_line_bounds.1 as usize);
        let h_line_bounds = self.h_line_bounds(self.center_v(), stroke_size);
        let h_line_bounds = (h_line_bounds.0 as usize + 1, h_line_bounds.1 as usize);

        for (to_x, from_y) in (v_line_bounds.0..=v_line_bounds.1)
            .into_iter()
            .zip((h_line_bounds.0..=h_line_bounds.1).into_iter())
        {
            let d1 = to_x as f32;
            let d2 = from_y as f32;

            let mut y = from_y as f32;
            while y >= 0. {
                let x = f32::sqrt(d2 * d2 - y * y) * d1 / d2;
                let x_r = min(x as usize, v_line_bounds.1 as usize - 1);
                let y_r = min(y as usize, h_line_bounds.1 as usize - 1);
                let index = min(x_r + y_r as usize * self.width, self.buffer.len() - 1);
                self.buffer[index] = COLOR_FILL;
                y -= 0.1;
            }
        }
    }

    /// Fills the `Canvas` with the given `Color`.
    fn fill(&mut self, color: RGBPixel) {
        self.buffer.fill(color);
    }

    /// Consumes `Canvas` and returns its underlying storage as raw byte vector.
    fn into_raw_buffer(self) -> Vec<u8> {
        // SAFETY This is safe since we use `repr(packed)` on `Pixel` struct for underlying storage
        // of the `Canvas` buffer which consists of three u8 values.
        unsafe {
            let capacity = self.buffer.capacity();
            let len = self.buffer.len() * 3;
            let buf = self.buffer.as_ptr() as *mut u8;
            std::mem::forget(self.buffer);
            Vec::from_raw_parts(buf, len, capacity)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crossfont::Metrics;

    #[test]
    fn builtin_line_drawing_glyphs_coverage() {
        // Dummy metrics values to test builtin glyphs coverage.
        let metrics = Metrics {
            average_advance: 6.,
            line_height: 16.,
            descent: 4.,
            underline_position: 2.,
            underline_thickness: 2.,
            strikeout_position: 2.,
            strikeout_thickness: 2.,
        };

        // Test coverage of box drawing characters.
        for character in '\u{2500}'..='\u{259f}' {
            assert!(builtin_glyph(character, &metrics).is_some());
        }
    }
}
