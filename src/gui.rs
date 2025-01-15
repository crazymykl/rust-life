extern crate image;
extern crate piston_window;

use self::piston_window::*;
use self::image::{ImageBuffer, Rgba};
use crate::board;

const SCALE: f64 = 2.0;
const X_SZ: u32 = 1280;
const Y_SZ: u32 = 800;
const LIVE_COLOR: [u8; 4] = [255, 255, 255, 255];
const DEAD_COLOR: [u8; 4] = [0, 0, 0, 255];

fn scale_dimension(x: u32) -> usize {
    (f64::from(x) / SCALE).floor() as usize
}

pub fn main() {
    let (rows, cols) = (scale_dimension(X_SZ), scale_dimension(Y_SZ));
    let mut brd = board::Board::new(rows, cols).random();

    let mut window: PistonWindow = WindowSettings::new("Life", [X_SZ, Y_SZ])
        .exit_on_esc(true)
        .graphics_api(OpenGL::V3_2)
        .build()
        .unwrap();
    let mut running = true;
    let mut cursor = [0, 0];
    let mut canvas = ImageBuffer::new(rows as u32, cols as u32);
    let mut texture_context = TextureContext {
        factory: window.factory.clone(),
        encoder: window.factory.create_command_buffer().into(),
    };
    let mut texture =
        Texture::from_image(&mut texture_context, &canvas, &TextureSettings::new()).unwrap();

    while let Some(e) = window.next() {
        e.mouse_cursor(|[x, y]| {
            cursor = [x as u32, y as u32];
        });

        if let Some(btn) = e.press_args() {
            match btn {
                Button::Mouse(MouseButton::Left) => {
                    let (x, y) = (
                        scale_dimension(cursor[0]) - 1,
                        scale_dimension(cursor[1]) - 1,
                    );
                    brd = brd.toggle(x, y);
                }
                Button::Mouse(MouseButton::Right) | Button::Keyboard(Key::Space) => {
                    running = !running
                }
                Button::Keyboard(Key::C) => brd = brd.clear(),
                Button::Keyboard(Key::R) => brd = brd.random(),
                Button::Keyboard(Key::S) => brd = brd.parallel_next_generation(),
                _ => {}
            };
        }

        if e.render_args().is_some() {
            for (x, y, val) in brd.cells() {
                let color = if val {
                    LIVE_COLOR
                } else {
                    DEAD_COLOR
                };
                canvas.put_pixel(y as u32, x as u32, Rgba(color));
            }
            texture.update(&mut texture_context, &canvas).unwrap();
            window.draw_2d(&e, |c, g, d| {
                clear([0.0; 4], g);
                image(&texture, c.transform.scale(SCALE, SCALE), g);
                texture_context.encoder.flush(d);
            });
        }

        if e.update_args().is_some() && running {
            brd = brd.parallel_next_generation();
        }
    }
}
