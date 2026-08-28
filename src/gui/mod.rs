//! GUI front-end, built directly on `winit` (events) and `wgpu` (rendering).
//!
//! The window/surface plumbing lives in `App`, an `ApplicationHandler`. The
//! actual game state and all event *handling* live in the headless, GPU-free
//! [`State`], which can be constructed and driven without an `EventLoop` —
//! that's what the unit tests below exercise.

use std::cell::RefCell;
use std::cmp::max;
use std::num::ParseFloatError;
use std::sync::Arc;

use crate::lifeboard::LifeBoard;
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, BufferUsages, CompositeAlphaMode, Device, Extent3d,
    PresentMode, Queue, RenderPipeline, Surface, SurfaceConfiguration, Texture, TextureUsages,
    TextureViewDimension,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes};

const LIVE_COLOR: [u8; 4] = [255, 255, 255, 255];
const DEAD_COLOR: [u8; 4] = [0, 0, 0, 255];

const MIN_SCALE: f64 = 0.1;
const MAX_SCALE: f64 = 100.0;

/// Headless game state: the board and every piece of per-input state, plus the
/// (optional) GPU-side cell texture used to render it. No window and no event
/// loop are involved, so this can be built and driven from unit tests.
pub struct State<B: LifeBoard> {
    pub brd: B,
    pub cursor: [f64; 2],
    pub scale: f64,
    pub running: bool,
    pub generation_limit: Option<usize>,
    pub exit_on_finish: bool,
    pub should_close: bool,
    gpu: GpuState,
}

struct GpuState {
    cell_texture: Option<Texture>,
    // Kept alive alongside the bind group that references it.
    cell_sampler: Option<wgpu::Sampler>,
    bind_group: Option<BindGroup>,
}

/// The per-frame renderer, built once against a window's surface.
struct Wgpu {
    device: Device,
    queue: Queue,
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
    rect_buffer: Buffer,
}

/// The `ApplicationHandler`: owns the window and renderer, paces the
/// simulation, and forwards `winit` window events into the headless [`State`].
/// The window and renderer are built in [`App::resumed`], per winit's
/// lifecycle guidance (this is also the path used when targeting the web).
struct App<B: LifeBoard> {
    state: Arc<RefCell<State<B>>>,
    window: Option<Arc<Window>>,
    gpu: Option<Wgpu>,
    tick: std::time::Duration,
    last_step: std::time::Instant,
    window_size: (u32, u32),
}

impl<B: LifeBoard> State<B> {
    fn new(
        brd: B,
        scale: f64,
        running: bool,
        generation_limit: Option<usize>,
        exit_on_finish: bool,
    ) -> Self {
        State {
            brd,
            cursor: [0.0, 0.0],
            scale,
            running,
            generation_limit,
            exit_on_finish,
            should_close: false,
            gpu: GpuState {
                cell_texture: None,
                cell_sampler: None,
                bind_group: None,
            },
        }
    }

    /// The initial cell texture: an empty `cols` × `rows` RGBA texture, so the
    /// first frame can be drawn before the board's contents are uploaded.
    fn init_cell_texture(&mut self, gpu: &Wgpu) {
        self.update_cell_texture(gpu);
    }

    /// (Re)create the cell texture for the board's current size, plus the bind
    /// group that samples it, then upload the current cell colors.
    fn update_cell_texture(&mut self, gpu: &Wgpu) {
        let (cols, rows) = (self.brd.cols(), self.brd.rows());
        let texture = Self::new_cell_texture(&gpu.device, cols, rows);
        let view = texture.create_view(&Default::default());
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cell sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cell bind group"),
            layout: &gpu.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu.rect_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });
        self.gpu.cell_texture = Some(texture);
        self.gpu.cell_sampler = Some(sampler);
        self.gpu.bind_group = Some(bind_group);
        self.upload_cells(gpu);
    }

    /// Rebuild the cell texture if the board changed size (it does when a
    /// window resize pads the board), so it matches the board dimensions.
    fn resize_texture_if_needed(&mut self, gpu: &Wgpu) {
        let Some(current) = self.gpu.cell_texture.as_ref() else {
            return;
        };
        let (cols, rows) = (self.brd.cols(), self.brd.rows());
        if current.size().width != cols as u32 || current.size().height != rows as u32 {
            self.update_cell_texture(gpu);
        }
    }

    fn new_cell_texture(device: &Device, cols: usize, rows: usize) -> Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cell texture"),
            size: Extent3d {
                width: cols as u32,
                height: rows as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    fn upload_cells(&self, gpu: &Wgpu) {
        let Some(texture) = self.gpu.cell_texture.as_ref() else {
            return;
        };
        let (cols, rows) = (self.brd.cols(), self.brd.rows());
        let cells: Vec<[u8; 4]> = self
            .brd
            .iter()
            .map(|live| if live { LIVE_COLOR } else { DEAD_COLOR })
            .collect();
        gpu.queue.write_texture(
            texture.as_image_copy(),
            bytemuck::cast_slice(&cells),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(cols as u32 * 4),
                rows_per_image: Some(rows as u32),
            },
            texture.size(),
        );
    }

    fn scaled_cursor(&self) -> (usize, usize) {
        (
            (self.cursor[1] / self.scale).floor() as usize,
            (self.cursor[0] / self.scale).floor() as usize,
        )
    }

    fn cursor_moved(&mut self, physical: PhysicalPosition<f64>) {
        self.cursor = [physical.x, physical.y];
    }

    fn left_click(&mut self) {
        let (x, y) = self.scaled_cursor();
        self.brd = self.brd.toggle(x, y);
    }

    fn right_click(&mut self) {
        self.running = !self.running;
    }

    fn key_press(&mut self, key: &Key) {
        match key {
            Key::Named(NamedKey::Escape) => self.close_requested(),
            Key::Named(NamedKey::Space) => self.running = !self.running,
            Key::Character(ch) => match ch.as_str() {
                "c" | "C" => self.brd = self.brd.clear(),
                "r" | "R" => self.brd = self.brd.random(),
                "s" | "S" => self.brd.step(),
                _ => {}
            },
            _ => {}
        }
    }

    fn resized(&mut self, width: u32, height: u32) {
        let scale = self.scale;
        let (old_cols, old_rows) = (self.brd.cols(), self.brd.rows());
        let (cols, rows) = (
            max(old_cols, (width as f64 / scale).floor() as usize),
            max(old_rows, (height as f64 / scale).floor() as usize),
        );
        if cols != old_cols || rows != old_rows {
            self.brd = self
                .brd
                .pad(0, (cols - old_cols) as isize, (rows - old_rows) as isize, 0);
        }
    }

    fn update(&mut self) {
        if !self.running {
            return;
        }
        if Some(self.brd.generation()) == self.generation_limit {
            if self.exit_on_finish {
                self.close_requested();
            } else {
                self.running = false;
            }
        } else {
            self.brd.step();
        }
    }

    fn close_requested(&mut self) {
        self.should_close = true;
    }

    /// Draw one frame: upload the current cells, then draw the cell texture
    /// stretched over the window and present.
    fn draw(&mut self, gpu: &Wgpu) {
        let (width, height) = (gpu.surface_config.width, gpu.surface_config.height);
        if width == 0 || height == 0 {
            return;
        }
        // Rebuild the cell texture (and its bind group) first if the board
        // changed size; then take the bind group for this frame.
        self.resize_texture_if_needed(gpu);
        let Some(bind_group) = self.gpu.bind_group.as_ref() else {
            return;
        };
        self.upload_cells(gpu);
        let frame = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&gpu.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
        frame.present();
    }
}

fn create_wgpu(window: Arc<Window>) -> Result<Wgpu, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    // A second handle to the same window keeps the `Surface` alive for as
    // long as the window does, and lets it be `'static`.
    let surface = instance
        .create_surface(window.clone())
        .map_err(|e| format!("failed to create a wgpu surface: {e:?}"))?;
    let adapter =
        futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| format!("failed to request a wgpu adapter: {e:?}"))?;
    let (device, queue) = futures::executor::block_on(adapter.request_device(&Default::default()))
        .map_err(|e| format!("failed to request a wgpu device: {e:?}"))?;

    let caps = surface.get_capabilities(&adapter);
    let alpha_mode = if caps
        .alpha_modes
        .contains(&CompositeAlphaMode::PostMultiplied)
    {
        CompositeAlphaMode::PostMultiplied
    } else if caps
        .alpha_modes
        .contains(&CompositeAlphaMode::PreMultiplied)
    {
        CompositeAlphaMode::PreMultiplied
    } else {
        CompositeAlphaMode::Opaque
    };
    let size = window.inner_size();
    let surface_config = SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        width: size.width,
        height: size.height,
        present_mode: PresentMode::Fifo,
        alpha_mode,
        view_formats: vec![wgpu::TextureFormat::Bgra8UnormSrgb],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &surface_config);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cell shader"),
        source: wgpu::ShaderSource::Wgsl(FRAGMENT_WGSL.into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cell layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cell pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cell pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let rect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rect buffer"),
        size: 16,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    Ok(Wgpu {
        device,
        queue,
        surface,
        surface_config,
        pipeline,
        bind_group_layout,
        rect_buffer,
    })
}

impl<B: LifeBoard> App<B> {
    fn reconfigure_surface(&mut self, width: u32, height: u32) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        if width == 0 || height == 0 {
            return;
        }
        gpu.surface_config = SurfaceConfiguration {
            width,
            height,
            ..gpu.surface_config.clone()
        };
        gpu.surface.configure(&gpu.device, &gpu.surface_config);
    }

    /// Update the (board px, window px) uniform the vertex shader uses to keep
    /// the board anchored at the top-left at `scale` px per cell.
    fn update_rect(&mut self, window: (u32, u32)) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        let state = self.state.borrow();
        let scale = state.scale as f32;
        let board = (
            state.brd.cols() as f32 * scale,
            state.brd.rows() as f32 * scale,
        );
        drop(state);
        let data = [board.0, board.1, window.0 as f32, window.1 as f32];
        gpu.queue
            .write_buffer(&gpu.rect_buffer, 0, bytemuck::cast_slice(&data));
    }
}

/// The shader: a full-window "super triangle" that samples the cell texture
/// with nearest filtering (so cells stay crisp). The board's first row is the
/// texture's `v = 0`, which maps to the top of the window.
const FRAGMENT_WGSL: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// The cell board's size (xy) and the window's size (zw), in pixels. The board
// is anchored at the top-left and kept at exactly `scale` px per cell, so
// cursor (px) / scale always maps to the right cell. Pixels outside the board
// render dead.
@group(0) @binding(2)
var<uniform> rect: vec4<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    let vertices = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = vertices[vertex_index];
    var out: VsOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    // NDC -1..1 -> window pixel coords with the origin at the top-left
    // (NDC `y` points up, window `y` points down), then to the cell board's
    // uv space (its top-left is uv (0, 0)). Pixels beyond the board map to
    // uv >= 1 and the fragment stage clamps them to dead.
    let ndc = p * 0.5 + 0.5;
    let px = vec2<f32>(ndc.x, 1.0 - ndc.y) * rect.zw;
    out.uv = px / rect.xy;
    return out;
}

@group(0) @binding(0)
var cell: texture_2d<f32>;

@group(0) @binding(1)
var cell_sampler: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(cell, cell_sampler, in.uv);
}
"#;

impl<B: LifeBoard + 'static> ApplicationHandler for App<B> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let (width, height) = self.window_size;
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Life")
                        .with_inner_size(winit::dpi::PhysicalSize::new(width, height)),
                )
                .expect("failed to create the winit window"),
        );
        let gpu = create_wgpu(window.clone()).expect("failed to initialize wgpu");
        self.state.borrow_mut().init_cell_texture(&gpu);
        self.window = Some(window.clone());
        self.gpu = Some(gpu);
        self.update_rect((width, height));
        // Draw the first frame right away, instead of waiting for the first
        // `about_to_wait` pass.
        window.request_redraw();
    }

    /// A `RedrawRequested` only arrives when a redraw is *requested*, so an
    /// idle window would otherwise never animate. Ask for one every pass and
    /// pace the loop to `ups` (winit's "run on demand" pattern); the loop
    /// still wakes early for input, so clicks and keys stay responsive.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            std::time::Instant::now() + self.tick,
        ));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.state.borrow_mut().cursor_moved(position);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if state == winit::event::ElementState::Pressed {
                    match button {
                        winit::event::MouseButton::Left => self.state.borrow_mut().left_click(),
                        winit::event::MouseButton::Right => self.state.borrow_mut().right_click(),
                        _ => {}
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == winit::event::ElementState::Pressed {
                    self.state.borrow_mut().key_press(&event.logical_key);
                }
            }
            WindowEvent::Resized(size) => {
                self.reconfigure_surface(size.width, size.height);
                self.state.borrow_mut().resized(size.width, size.height);
                self.update_rect((size.width, size.height));
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                // Advance the simulation at most once per `tick`, so input
                // events that wake the loop early don't speed it up.
                if std::time::Instant::now() >= self.last_step {
                    self.state.borrow_mut().update();
                    self.last_step = std::time::Instant::now() + self.tick;
                    if self.state.borrow().should_close {
                        event_loop.exit();
                    }
                }
                // Only draw once the window and renderer exist (they are built
                // in `resumed`).
                if let Some(gpu) = self.gpu.as_ref() {
                    self.state.borrow_mut().draw(gpu);
                }
            }
            WindowEvent::CloseRequested => {
                self.state.borrow_mut().close_requested();
                event_loop.exit();
            }
            _ => {}
        }
    }
}

pub fn run<B: LifeBoard + 'static>(
    brd: B,
    scale: f64,
    ups: u64,
    init_running: bool,
    generation_limit: Option<usize>,
    exit_on_finish: bool,
) {
    let event_loop = EventLoop::new().expect("failed to create the winit event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let tick = std::time::Duration::from_millis(1000 / ups.max(1));

    let width = (brd.cols() as f64 * scale).max(1.0) as u32;
    let height = (brd.rows() as f64 * scale).max(1.0) as u32;

    let mut app = App {
        state: Arc::new(RefCell::new(State::new(
            brd,
            scale,
            init_running,
            generation_limit,
            exit_on_finish,
        ))),
        window: None,
        gpu: None,
        tick,
        last_step: std::time::Instant::now(),
        window_size: (width, height),
    };
    event_loop
        .run_app(&mut app)
        .expect("event loop terminated with an error");
}

pub(crate) fn valid_scale(s: &str) -> Result<f64, String> {
    match s.parse().map_err(|e: ParseFloatError| e.to_string())? {
        n @ MIN_SCALE..=MAX_SCALE => Ok(n),
        _ => Err(format!(
            "Scale must be between {MIN_SCALE} and {MAX_SCALE} (inclusive)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use std::str::FromStr;

    fn make_state() -> State<Board> {
        State::new(Board::new(3, 3), 4.0, true, Some(1), false)
    }

    #[test]
    fn cursor_move() {
        let mut s = make_state();
        assert_eq!(s.cursor, [0.0, 0.0]);
        s.cursor_moved(PhysicalPosition::new(9.0, 0.0));
        assert_eq!(s.cursor, [9.0, 0.0]);
    }

    #[test]
    fn click_cell_toggle() {
        let mut s = make_state();
        s.left_click();
        assert_eq!(s.brd.to_string(), "@..\n...\n...");
    }

    #[test]
    fn step_event() {
        let mut s = make_state();
        assert_eq!(s.brd.generation(), 0);
        s.key_press(&Key::Character("s".into()));
        assert_eq!(s.brd.generation(), 1);
    }

    #[test]
    fn clear_event() {
        let mut s = State::new(Board::from_str("@").unwrap(), 4.0, true, None, false);
        assert_eq!(s.brd.population(), 1);
        s.key_press(&Key::Character("c".into()));
        assert_eq!(s.brd.population(), 0);
    }

    #[test]
    fn randomize_event() {
        let mut s = State::new(Board::new(30, 30), 4.0, true, None, false);
        assert_eq!(s.brd.population(), 0);
        s.key_press(&Key::Character("r".into()));
        assert_ne!(s.brd.population(), 0);
    }

    #[test]
    fn quit_event() {
        let mut s = make_state();
        assert!(!s.should_close);
        s.key_press(&Key::Named(NamedKey::Escape));
        assert!(s.should_close);
    }

    #[test]
    fn unhandled_key() {
        let mut s = make_state();
        let (brd, running) = (s.brd.to_string(), s.running);
        s.key_press(&Key::Named(NamedKey::CapsLock));
        assert_eq!(s.brd.to_string(), brd);
        assert_eq!(s.running, running);
    }

    #[test]
    fn toggle_running() {
        let mut s = make_state();
        assert!(s.running);
        s.key_press(&Key::Named(NamedKey::Space));
        assert!(!s.running);
        s.right_click();
        assert!(s.running);
    }

    #[test]
    fn update_event() {
        let mut s = make_state();
        s.update();
        assert_eq!(s.brd.generation(), 1);
        assert!(s.running);
        s.update();
        assert_eq!(s.brd.generation(), 1);
        assert!(!s.running);
    }

    #[test]
    fn resize_event() {
        let mut s = make_state();
        assert_eq!(s.brd.cols() * s.brd.rows(), 9);
        s.resized(40, 100);
        assert_eq!(s.brd.cols() * s.brd.rows(), 250);
        s.resized(40, 40);
        // we don't truncate the board if the window shrinks
        assert_eq!(s.brd.cols() * s.brd.rows(), 250);
    }

    #[test]
    fn valid_scale_bounds() {
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
            Err("invalid float literal".to_string())
        );
    }

    /// Headless check that the WGSL parses (no GPU or adapter needed, so it
    /// also runs in CI). Catches shader regressions — e.g. an unknown
    /// identifier — without launching the GUI.
    #[test]
    fn shader_parses() {
        naga::front::wgsl::parse_str(FRAGMENT_WGSL).expect("the cell shader WGSL must parse");
    }
}
