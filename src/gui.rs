extern crate piston_window;
extern crate image as im;

use self::piston_window::*;
use board;

// // Like my AVR build.
// const UPS: u32 = 1;
// const SCALE: u32 = 10;
// const X_SZ: u32 = 80;
// const Y_SZ: u32 = 80;

// // Like my ARM build.
// const UPS: u32 = 15;
// const SCALE: u32 = 5;
// const X_SZ: u32 = 64*5;
// const Y_SZ: u32 = 64*5;

// // But now we have real hardware.
// const UPS: u32 = 10;
// const SCALE: u32 = 4;
// const X_SZ: u32 = 1920;
// const Y_SZ: u32 = 1080;

// This is a pretty big bang.
const UPS: u32 = 120;
const SCALE: u32 = 1;
const X_SZ: u32 = 1920;
const Y_SZ: u32 = 1080;

pub fn main() {
    let (rows, cols) = (X_SZ / SCALE, Y_SZ / SCALE);
    let mut brd = board::Board::new(rows as usize, cols as usize).random();
    let mut window: PistonWindow = WindowSettings::new("Life", [X_SZ, Y_SZ])
        .exit_on_esc(true)
        .graphics_api(OpenGL::V3_2)
        .fullscreen(true)
        .build()
        .unwrap();
    window.events = Events::new(EventSettings {
        max_fps: 60,
        ups: UPS as u64,
        ups_reset: 2,
        swap_buffers: true,
        lazy: false,
        bench_mode: false,
    });
    let mut canvas = im::ImageBuffer::new(X_SZ as u32, X_SZ as u32);
    let mut ctx = window.create_texture_context();
    let mut texture = Texture::from_image(
        &mut ctx,
        &canvas,
        &TextureSettings::new(),
    ).unwrap();

    let mut running = true;
    let mut cursor = [0, 0];
    while let Some(e) = window.next() {
        e.mouse_cursor(|[x, y]| {
            cursor = [x as u32, y as u32];
        });

        if let Some(btn) = e.press_args() {
            match btn {
                Button::Mouse(MouseButton::Left) => {
                    let (x, y) = (cursor[0] / SCALE,
                                  cursor[1] / SCALE);
                    brd = brd.toggle(x as usize, y as usize);
                },
                Button::Mouse(MouseButton::Right)
                | Button::Keyboard(Key::Space)   => running = !running,
                Button::Keyboard(Key::C)         => brd = brd.clear(),
                Button::Keyboard(Key::R)         => brd = brd.random(),
                Button::Keyboard(Key::S)         => brd = brd.parallel_next_generation(),
                _                                => {}
            };
        }

        if let Some(_) = e.render_args() {
            for (x, y, val) in brd.cells() {
                let color = if val {
                    [255, 255, 255, 255]
                } else {
                    [0, 0, 0, 255]
                };

                for i in 0..SCALE {
                    for j in 0..SCALE {
                        let sx = x as u32 * SCALE + i;
                        let sy = y as u32 * SCALE + j;
                        canvas.put_pixel(sy, sx, im::Rgba(color));
                    }
                }
            }

            texture.update(&mut ctx, &canvas).unwrap();
            window.draw_2d(&e, |c, g, d| {
                // Update texture before rendering.
                ctx.encoder.flush(d);

                clear([0.0; 4], g);
                image(&texture, c.transform, g);
            });
        }

        if let Some(_) = e.update_args() {
            if running {
                brd = brd.parallel_next_generation();
            }
        }
    }
}
