use std::{cmp::max, num::ParseFloatError};

use crate::board::Board;
use ::image::ImageBuffer;
use piston_window::*;

#[cfg(feature = "test_mainthread")]
pub mod test_helper;

const LIVE_COLOR: [u8; 4] = [255, 255, 255, 255];
const DEAD_COLOR: [u8; 4] = [0, 0, 0, 255];

struct GameState {
    brd: Board,
    scale: f64,
    window: PistonWindow,
    cursor: [f64; 2],
    texture_context: G2dTextureContext,
    texture: G2dTexture,
    running: bool,
    generation_limit: Option<usize>,
    exit_on_finish: bool,
}

impl GameState {
    fn new(
        brd: Board,
        scale: f64,
        ups: u64,
        running: bool,
        generation_limit: Option<usize>,
        exit_on_finish: bool,
    ) -> Self {
        let mut window: PistonWindow = WindowSettings::new(
            "Life",
            [brd.cols() as f64 * scale, brd.rows() as f64 * scale],
        )
        .exit_on_esc(true)
        .graphics_api(OpenGL::V3_2)
        .build()
        .unwrap();
        window.set_ups(ups);
        let mut texture_context = window.create_texture_context();
        let texture = Self::make_texture(&mut texture_context, brd.cols(), brd.rows());

        GameState {
            brd,
            scale,
            window,
            cursor: [0.0, 0.0],
            texture_context,
            texture,
            running,
            generation_limit,
            exit_on_finish,
        }
    }

    fn make_texture(
        texture_context: &mut G2dTextureContext,
        cols: usize,
        rows: usize,
    ) -> G2dTexture {
        Texture::from_image(
            texture_context,
            &ImageBuffer::new(cols as u32, rows as u32),
            &TextureSettings::new().mag(Filter::Nearest),
        )
        .unwrap()
    }

    fn run(&mut self) {
        while let Some(e) = self.window.next() {
            self.handle_event(e);
        }
    }

    fn handle_event(&mut self, e: Event) {
        e.mouse_cursor(|xy| {
            self.cursor = xy;
        });

        if let Some(btn) = e.press_args() {
            match btn {
                Button::Mouse(MouseButton::Left) => {
                    let (x, y) = self.scaled_cursor();
                    self.brd = self.brd.toggle(x, y);
                }
                Button::Mouse(MouseButton::Right) | Button::Keyboard(Key::Space) => {
                    self.running = !self.running;
                }
                Button::Keyboard(Key::C) => self.brd = self.brd.clear(),
                Button::Keyboard(Key::Q) => self.window.set_should_close(true),
                Button::Keyboard(Key::R) => self.brd = self.brd.random(),
                Button::Keyboard(Key::S) => self.brd = self.brd.next_generation(),
                _ => {}
            };
        }

        if e.render_args().is_some() {
            let cells = self
                .brd
                .iter()
                .flat_map(|&val| if val { LIVE_COLOR } else { DEAD_COLOR })
                .collect();

            self.texture
                .update(
                    &mut self.texture_context,
                    &ImageBuffer::from_raw(self.brd.cols() as u32, self.brd.rows() as u32, cells)
                        .unwrap(),
                )
                .unwrap();
            self.window.draw_2d(&e, |c, g, d| {
                image(&self.texture, c.transform.scale(self.scale, self.scale), g);
                self.texture_context.encoder.flush(d);
            });
        }

        if e.update_args().is_some() && self.running {
            if Some(self.brd.generation()) == self.generation_limit {
                if self.exit_on_finish {
                    self.window.set_should_close(true);
                } else {
                    self.running = false;
                }
            } else {
                self.brd = self.brd.next_generation();
            }
        }

        if let Some(r) = e.resize_args() {
            let (old_cols, old_rows) = (self.brd.cols(), self.brd.rows());
            let (cols, rows) = (
                max(old_cols, self.scale_dimension(r.window_size[0])),
                max(old_rows, self.scale_dimension(r.window_size[1])),
            );
            if cols != old_cols || rows != old_rows {
                self.brd =
                    self.brd
                        .pad(0, (cols - old_cols) as isize, (rows - old_rows) as isize, 0);
                self.texture = Self::make_texture(&mut self.texture_context, cols, rows);
            }
        }
    }

    fn scale_dimension(&self, x: f64) -> usize {
        (x / self.scale).floor() as usize
    }

    fn scaled_cursor(&self) -> (usize, usize) {
        (
            self.scale_dimension(self.cursor[1]),
            self.scale_dimension(self.cursor[0]),
        )
    }
}

pub fn run(
    brd: Board,
    scale: f64,
    ups: u64,
    init_running: bool,
    generation_limit: Option<usize>,
    exit_on_finish: bool,
) {
    GameState::new(
        brd,
        scale,
        ups,
        init_running,
        generation_limit,
        exit_on_finish,
    )
    .run();
}

const MIN_SCALE: f64 = 0.1;
const MAX_SCALE: f64 = 100.0;

pub(crate) fn valid_scale(s: &str) -> Result<f64, String> {
    match s.parse().map_err(|e: ParseFloatError| e.to_string())? {
        n @ MIN_SCALE..=MAX_SCALE => Ok(n),
        _ => Err(format!(
            "Scale must be between {MIN_SCALE} and {MAX_SCALE} (inclusive)"
        )),
    }
}

#[test]
fn test_valid_scale() {
    assert_eq!(
        valid_scale("0"),
        Err(format!(
            "Scale must be between {MIN_SCALE} and {MAX_SCALE} (inclusive)"
        ))
    );
    assert_eq!(valid_scale("1"), Ok(1.0));
    assert_eq!(
        valid_scale("9999"),
        Err(format!(
            "Scale must be between {MIN_SCALE} and {MAX_SCALE} (inclusive)"
        ))
    );
    assert_eq!(
        valid_scale("puppies"),
        Err(format!("invalid float literal"))
    );
}
