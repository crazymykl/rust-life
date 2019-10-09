extern crate piston_window;
extern crate image as im;

use self::piston_window::*;
use board;

// woh
// const SCALE: f64 = 1.0;
// const X_SZ: u32 = 1920;
// const Y_SZ: u32 = 1080;

const SCALE: u32 = 10;
const X_SZ: u32 = 800;
const Y_SZ: u32 = 600;

pub fn main() {
    let (rows, cols) = (X_SZ / SCALE, Y_SZ / SCALE);
    let mut brd = board::Board::new(rows as usize, cols as usize).random();
    // let ref mut worker_pool = board::WorkerPool::new_with_default_size();

    let mut window: PistonWindow = WindowSettings::new("Life", [X_SZ, Y_SZ])
        .exit_on_esc(true)
        .opengl(OpenGL::V3_2)
        .fullscreen(true)
        .build()
        .unwrap();
    let mut running = true;
    let mut cursor = [0, 0];
    let mut canvas = im::ImageBuffer::new(X_SZ as u32, X_SZ as u32);
    let mut texture = Texture::from_image(
        &mut window.factory,
        &canvas,
        &TextureSettings::new()
    ).unwrap();

    while let Some(e) = window.next() {
        e.mouse_cursor(|x, y| {
            cursor = [x as u32, y as u32];
        });

        if let Some(btn) = e.press_args() {
            match btn {
                Button::Mouse(MouseButton::Left) => {
                    // let (x, y) = (scale_dimension(cursor[0]) - 1,
                    //               scale_dimension(cursor[1]) - 1);
                    // brd = brd.toggle(x, y);
                },
                Button::Mouse(MouseButton::Right)
                | Button::Keyboard(Key::Space)   => running = !running,
                Button::Keyboard(Key::C)         => brd = brd.clear(),
                Button::Keyboard(Key::R)         => brd = brd.random(),
                Button::Keyboard(Key::S)         => brd = brd.parallel_next_generation(),
                _                                => {}
            };
        }

        if e.render_args().is_some() {
            for (x, y, val) in brd.cells() {
                let color = if val { [255, 255, 255, 255] } else { [0, 0, 0, 255] };
                for i in (0..SCALE) {
                    let sx = x as u32 * SCALE + i;
                    let sy = y as u32 * SCALE + i;
                    canvas.put_pixel(sy, sx, im::Rgba(color));
                }
            }
            texture.update(&mut window.encoder, &canvas).unwrap();
            window.draw_2d(&e, |c, g| {
                clear([0.0; 4], g);
                image(&texture, c.transform, g);
            });
        }

        if e.update_args().is_some() && running {
            brd = brd.parallel_next_generation();
        }
    }
}
