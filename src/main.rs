#![cfg_attr(all(test, feature = "unstable"), feature(test))]

extern crate threadpool;
extern crate rand;
extern crate num_cpus;

extern crate piston_window;
extern crate image as im;

use piston_window::*;

#[cfg(all(test, feature = "unstable"))]
mod benchmarks;

mod board;

const SCALE: f64 = 2.0;
const X_SZ: u32 = 1280;
const Y_SZ: u32 = 800;

fn scale_dimension(x: u32) -> usize {
    (x as f64 / SCALE).floor() as usize
}

#[cfg(not(test))]
fn main() {
    let (rows, cols) = (scale_dimension(X_SZ), scale_dimension(Y_SZ));
    let mut brd = board::Board::new(rows, cols).random();
    let ref mut worker_pool = board::WorkerPool::new_with_default_size();

    let window: PistonWindow = WindowSettings::new("Life", [X_SZ, Y_SZ])
        .exit_on_esc(true)
        .opengl(OpenGL::V3_2)
        .build()
        .unwrap();
    let mut running = true;
    let mut cursor = [0, 0];
    let mut canvas = im::ImageBuffer::new(rows as u32, cols as u32);
    let mut texture = Texture::from_image(
        &mut *window.factory.borrow_mut(),
        &canvas,
        &TextureSettings::new()
    ).unwrap();

    for e in window {
        e.mouse_cursor(|x, y| {
            cursor = [x as u32, y as u32];
        });

        if let Some(btn) = e.press_args() {
            match btn {
                Button::Mouse(MouseButton::Left) => {
                    let (x, y) = (scale_dimension(cursor[0]) - 1
                                , scale_dimension(cursor[1]) - 1);
                    brd = brd.toggle(x, y);
                },
                Button::Mouse(MouseButton::Right)
                | Button::Keyboard(Key::Space)   => running = !running,
                Button::Keyboard(Key::C)         => brd = brd.clear(),
                Button::Keyboard(Key::R)         => brd = brd.random(),
                Button::Keyboard(Key::S)         => brd = brd.parallel_next_generation(worker_pool),
                _                                => {}
            };
        }

        if let Some(_) = e.render_args() {
            for (x, y, val) in brd.cells() {
                let color = if val { [255, 255, 255, 255] } else { [0, 0, 0, 255] };
                canvas.put_pixel(y as u32, x as u32, im::Rgba(color));
            }
            texture.update(&mut e.encoder.borrow_mut(), &canvas).unwrap();
            e.draw_2d(|c, g| {
                clear([0.0; 4], g);
                image(&texture, c.transform.scale(SCALE, SCALE), g);
            });
        }

        if let Some(_) = e.update_args() {
            if running { brd = brd.parallel_next_generation(worker_pool); }
        }
    }
}
