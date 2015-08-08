#![cfg_attr(all(test, feature = "unstable"), feature(test))]

extern crate threadpool;
extern crate rand;
extern crate num_cpus;

extern crate piston_window;

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

    let opengl = OpenGL::V3_2;
    let window: PistonWindow = WindowSettings::new("Life", [X_SZ, Y_SZ])
        .exit_on_esc(true)
        .opengl(opengl)
        .into();
    let mut running = true;
    let mut cursor = [0, 0];

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
            e.draw_2d(|c, g| {
                clear([0.0; 4], g);
                for (x, y, val) in brd.cells() {
                    if !val { continue; }
                    rectangle(
                        [1.0; 4],
                        [y as f64 * SCALE, x as f64 * SCALE, SCALE, SCALE],
                        c.transform, g
                    );
                }
            });
        }

        if let Some(_) = e.update_args() {
            if running { brd = brd.parallel_next_generation(worker_pool); }
        }
    }
}
