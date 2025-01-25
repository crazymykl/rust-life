use std::cmp::max;

use crate::board::Board;
use ::image::ImageBuffer;
use piston_window::*;

const LIVE_COLOR: [u8; 4] = [255, 255, 255, 255];
const DEAD_COLOR: [u8; 4] = [0, 0, 0, 255];

pub fn run(
    brd: &mut Board,
    scale: f64,
    ups: u64,
    init_running: bool,
    generation_limit: Option<usize>,
    exit_on_finish: bool,
) {
    let scale_dimension = |x: f64| -> usize { (x / scale).floor() as usize };

    let mut window: PistonWindow = WindowSettings::new(
        "Life",
        [brd.cols() as f64 * scale, brd.rows() as f64 * scale],
    )
    .exit_on_esc(true)
    .graphics_api(OpenGL::V3_2)
    .build()
    .unwrap();
    window.set_ups(ups);
    let mut running = init_running;
    let mut cursor = [0.0, 0.0];
    let mut texture_context = TextureContext {
        factory: window.factory.clone(),
        encoder: window.factory.create_command_buffer().into(),
    };
    let mut texture = Texture::from_image(
        &mut texture_context,
        &ImageBuffer::new(brd.cols() as u32, brd.rows() as u32),
        &TextureSettings::new().mag(Filter::Nearest),
    )
    .unwrap();

    while let Some(e) = window.next() {
        e.mouse_cursor(|xy| {
            cursor = xy;
        });

        if let Some(btn) = e.press_args() {
            match btn {
                Button::Mouse(MouseButton::Left) => {
                    let (x, y) = (scale_dimension(cursor[1]), scale_dimension(cursor[0]));
                    *brd = brd.toggle(x, y);
                }
                Button::Mouse(MouseButton::Right) | Button::Keyboard(Key::Space) => {
                    running = !running;
                }
                Button::Keyboard(Key::C) => *brd = brd.clear(),
                Button::Keyboard(Key::R) => *brd = brd.random(),
                Button::Keyboard(Key::S) => *brd = brd.next_generation(),
                _ => {}
            };
        }

        if e.render_args().is_some() {
            let cells = brd
                .iter()
                .flat_map(|&val| if val { LIVE_COLOR } else { DEAD_COLOR })
                .collect();

            texture
                .update(
                    &mut texture_context,
                    &ImageBuffer::from_raw(brd.cols() as u32, brd.rows() as u32, cells).unwrap(),
                )
                .unwrap();
            window.draw_2d(&e, |c, g, d| {
                image(&texture, c.transform.scale(scale, scale), g);
                texture_context.encoder.flush(d);
            });
        }

        if e.update_args().is_some() && running {
            if Some(brd.generation()) == generation_limit {
                if exit_on_finish {
                    window.set_should_close(true);
                } else {
                    running = false;
                }
            } else {
                *brd = brd.next_generation();
            }
        }

        if let Some(r) = e.resize_args() {
            let (old_cols, old_rows) = (brd.cols(), brd.rows());
            let (cols, rows) = (
                max(old_cols, scale_dimension(r.window_size[0])),
                max(old_rows, scale_dimension(r.window_size[1])),
            );
            if cols != old_cols || rows != old_rows {
                *brd = brd.pad(0, (cols - old_cols) as isize, (rows - old_rows) as isize, 0);
                texture = Texture::from_image(
                    &mut texture_context,
                    &ImageBuffer::new(cols as u32, rows as u32),
                    &TextureSettings::new().mag(Filter::Nearest),
                )
                .unwrap();
            }
        }
    }
}
