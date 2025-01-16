use crate::board;
use crate::Args;
use ::image::{ImageBuffer, Rgba};
use piston_window::*;

const LIVE_COLOR: [u8; 4] = [255, 255, 255, 255];
const DEAD_COLOR: [u8; 4] = [0, 0, 0, 255];

pub fn main(args: Args) {
    let (rows, cols) = (args.rows, args.cols);
    let scale_dimension = |x: u32| -> usize { (f64::from(x) / args.scale).floor() as usize };
    let mut brd = board::Board::new(rows as usize, cols as usize).random();

    let mut window: PistonWindow = WindowSettings::new(
        "Life",
        [f64::from(cols) * args.scale, f64::from(rows) * args.scale],
    )
    .exit_on_esc(true)
    .graphics_api(OpenGL::V3_2)
    .build()
    .unwrap();
    let mut running = true;
    let mut cursor = [0, 0];
    let mut canvas = ImageBuffer::new(cols, rows);
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
                    let (x, y) = (scale_dimension(cursor[1]), scale_dimension(cursor[0]));
                    brd = brd.toggle(x, y);
                }
                Button::Mouse(MouseButton::Right) | Button::Keyboard(Key::Space) => {
                    running = !running
                }
                Button::Keyboard(Key::C) => brd = brd.clear(),
                Button::Keyboard(Key::R) => brd = brd.random(),
                Button::Keyboard(Key::S) => brd = brd.next_generation(),
                _ => {}
            };
        }

        if e.render_args().is_some() {
            for (x, y, val) in brd.cells() {
                let color = if val { LIVE_COLOR } else { DEAD_COLOR };
                canvas.put_pixel(x as u32, y as u32, Rgba(color));
            }
            texture.update(&mut texture_context, &canvas).unwrap();
            window.draw_2d(&e, |c, g, d| {
                clear([0.0; 4], g);
                image(&texture, c.transform.scale(args.scale, args.scale), g);
                texture_context.encoder.flush(d);
            });
        }

        if e.update_args().is_some() && running {
            brd = brd.next_generation();
        }
    }
}
