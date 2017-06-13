extern crate piston_window;
extern crate image as im;

use self::piston_window::*;
use board;

const SCALE: f64 = 1.0;
const X_SZ: u32 = 1280;
const Y_SZ: u32 = 800;

fn scale_dimension(x: u32) -> usize {
    (x as f64 / SCALE).floor() as usize
}

pub fn main() {
    let (rows, cols) = (scale_dimension(X_SZ), scale_dimension(Y_SZ));
    let mut board = board::Board::new(rows, cols).random();
    // let ref mut worker_pool = board::WorkerPool::new_with_default_size();

    let mut window: PistonWindow = WindowSettings::new("Life", [X_SZ, Y_SZ])
        .exit_on_esc(true)
        .opengl(OpenGL::V3_2)
        .build()
        .unwrap();
    let mut running = true;
    let mut cursor = [0, 0];
    let mut canvas = im::ImageBuffer::new(rows as u32, cols as u32);
    let mut texture = Texture::from_image(
        &mut window.factory,
        &canvas,
        &TextureSettings::new()
    ).unwrap();

    while let Some(event) = window.next() {
        event.mouse_cursor(|x, y| {
            cursor = [x as u32, y as u32];
        });

        if let Some(button) = event.press_args() {
            match button {
                Button::Mouse(MouseButton::Left) => {
                    let (x, y) = (scale_dimension(cursor[0]) - 1,
                                  scale_dimension(cursor[1]) - 1);
                    board = board.toggle(x, y);
                },
                Button::Keyboard(Key::Space) => running = !running,
                Button::Keyboard(Key::C) => board = board.clear(),
                Button::Keyboard(Key::R) => board = board.random(),
                Button::Keyboard(Key::S) => board = board.parallel_next_generation(),
                _ => {}
            };
        }

        if let Some(_) = event.render_args() {
            for (x, y, val) in board.cells() {
                let color = if val { [255, 255, 255, 255] } else { [0, 0, 0, 255] };
                canvas.put_pixel(y as u32, x as u32, im::Rgba(color));
            }
            texture.update(&mut window.encoder, &canvas).unwrap();
            window.draw_2d(&event, |c, g| {
                clear([0.0; 4], g);
                image(&texture, c.transform.scale(SCALE, SCALE), g);
            });
        }

        if let Some(_) = event.update_args() {
            if running { board = board.parallel_next_generation(); }
        }
    }
}
