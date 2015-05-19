#![feature(test)]

extern crate threadpool;
extern crate rand;
extern crate num_cpus;

extern crate piston;
extern crate opengl_graphics;
extern crate graphics;
extern crate glutin_window;

use opengl_graphics::{GlGraphics, OpenGL};
use glutin_window::GlutinWindow as Window;
use piston::window::{AdvancedWindow, WindowSettings};
use piston::input::{Key, Button};
use piston::input::mouse::MouseButton;
use piston::event::*;

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

    let opengl = OpenGL::_3_2;
    let window = Window::new(
        opengl,
        WindowSettings::new("Life", [X_SZ, Y_SZ])
        .exit_on_esc(true)
    );
    let ref mut gl = GlGraphics::new(opengl);
    let rect = graphics::Rectangle::new([1.0; 4]);
    let mut running = true;
    let mut cursor = [0.0, 0.0];

    for e in window.events() {
        e.mouse_cursor(|x, y| {
            cursor = [x, y];
        });

        if let Some(btn) = e.press_args() {
            match btn {
                Button::Mouse(MouseButton::Left) => {
                    let (x, y) = ((cursor[0] / SCALE).floor() as usize - 1
                                , (cursor[1] / SCALE).floor() as usize - 1);
                    brd = brd.toggle(x, y);
                },
                Button::Keyboard(Key::C)         => brd = brd.clear(),
                Button::Keyboard(Key::R)         => brd = brd.random(),
                Button::Keyboard(Key::S)         => brd = brd.parallel_next_generation(worker_pool),
                Button::Keyboard(Key::Space)     => running = !running,
                _                                => {}
            };
        }

        if let Some(args) = e.render_args() {
            gl.draw(args.viewport(), |c, g| {
                graphics::clear([0.0; 4], g);
                for (x, y, val) in brd.cells() {
                    if !val { continue; }
                    rect.draw(
                        [y as f64 * SCALE, x as f64 * SCALE, SCALE, SCALE],
                        &c.draw_state, c.transform, g
                    );
                }
            });
        }

        if let Some(_) = e.update_args() {
            if running {
                brd = brd.parallel_next_generation(worker_pool);
            }
        }
    }
}
