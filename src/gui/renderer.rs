//! The `wgpu` side of the GUI: the per-window [`Renderer`], which owns the
//! cell texture the board is uploaded to and draws it into the window.
//!
//! Building a `Renderer` is two-phase because the adapter and device request
//! is asynchronous while the rest of the setup is not. [`Unattached::new_surface`]
//! builds the surface against the window; [`Unattached::attach_device`] awaits
//! the adapter and device and completes the surface configuration and pipeline.
//! Native drives the second phase inline with `futures::executor::block_on`
//! (see [`Renderer::for_window`]); the web runs it as a `spawn_local` task and
//! attaches the result in `App::proxy_wake_up`.

use std::sync::Arc;

use super::State;
use crate::lifeboard::LifeBoard;
use wgpu::SurfaceTarget;
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, BufferUsages, CompositeAlphaMode, Device, Extent3d,
    PresentMode, Queue, RenderPipeline, Surface, SurfaceConfiguration, Texture, TextureUsages,
    TextureViewDimension,
};
use winit::dpi::PhysicalSize;
use winit::window::Window;

/// The cell board's size (xy) and the window's size (zw), in pixels. The board
/// is anchored at the top-left and kept at exactly `scale` px per cell, so
/// cursor (px) / scale always maps to the right cell. Pixels outside the board
/// render dead.
const FRAGMENT_WGSL: &str = include_str!("board.wgsl");
const SURFACE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
const LIVE_COLOR: [u8; 4] = [255, 255, 255, 255];
const DEAD_COLOR: [u8; 4] = [0, 0, 0, 255];

/// The cell texture and the bind group that binds it (and its sampler) to the
/// pipeline. These are created and rebuilt together, so they are grouped into
/// a single `Option` to rule out a half-built or partially-freed set (wgpu's
/// refcounting keeps the sampler and texture alive via the bind group).
struct Cell {
    texture: Texture,
    bind_group: BindGroup,
}

/// The GPU-side state for the board: the (optional) cell texture and its
/// bindings, plus the reusable upload buffer.
#[derive(Default)]
struct GpuState {
    cell: Option<Cell>,
    /// Reusable RGBA upload buffer, sized for the largest board so far.
    cells: Vec<[u8; 4]>,
}

/// A renderer with its surface but not yet its adapter and device. It keeps
/// the instance that created the surface alongside it, so the second phase can
/// request a compatible adapter and complete the build.
pub(super) struct Unattached {
    instance: wgpu::Instance,
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
}

impl Unattached {
    /// Create the wgpu surface and the (adapter- and device-free) renderer
    /// against the window.
    ///
    /// One `instance` backs both the surface and the adapter: each `Instance`
    /// has its own object storage, and a surface is only resolvable by the
    /// instance that created it, so it is kept on the `Unattached` for the
    /// second phase. A second handle to the same window (cloned out of the
    /// `Arc`) is boxed into the `SurfaceTarget`, keeping the `Surface` alive
    /// for as long as the window does, and letting it be `'static`.
    ///
    /// The `?` only fires on a GPU failure, which a passing test can't force,
    /// so exclude it from coverage.
    #[cfg_attr(feature = "unstable", coverage(off))]
    pub(super) fn new_surface(
        window: &Arc<dyn Window>,
        size: PhysicalSize<u32>,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = Self::create_surface(&instance, window)?;

        let surface_config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: SURFACE_FORMAT,
            width: size.width,
            height: size.height,
            present_mode: PresentMode::Fifo,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![SURFACE_FORMAT],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };

        Ok(Unattached {
            instance,
            surface,
            surface_config,
        })
    }

    /// Create the wgpu surface for the window. The error arm only runs on a
    /// GPU failure, which a passing test can't force, so exclude it from
    /// coverage.
    #[cfg_attr(feature = "unstable", coverage(off))]
    fn create_surface(
        instance: &wgpu::Instance,
        window: &Arc<dyn Window>,
    ) -> Result<Surface<'static>, String> {
        instance
            .create_surface(SurfaceTarget::from(window.clone()))
            .map_err(|e| format!("failed to create a wgpu surface: {e:?}"))
    }

    /// Await an adapter and device compatible with the surface, then complete
    /// the surface configuration and build the pipeline and uniform buffer.
    ///
    /// This is the async heart of the build. Native drives it inline with
    /// `futures::executor::block_on` (safe: the `wgpu` requests resolve without
    /// an external runtime); the web runs it as a `spawn_local` task (a
    /// `block_on` there would deadlock, since the requests resolve on browser
    /// microtasks). The `?` only fires on a GPU failure, which a passing test
    /// can't force, so exclude it from coverage.
    #[cfg_attr(feature = "unstable", coverage(off))]
    pub(super) async fn attach_device(self) -> Result<Renderer, String> {
        let (device, queue) = Self::request_device(&self.instance, &self.surface).await?;
        self.surface.configure(&device, &self.surface_config);

        let (pipeline, bind_group_layout) = Renderer::create_pipeline(&device);
        let rect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect buffer"),
            size: 16,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Renderer {
            device,
            queue,
            surface: self.surface,
            surface_config: self.surface_config,
            pipeline,
            bind_group_layout,
            rect_buffer,
            gpu_state: GpuState::default(),
        })
    }

    /// Pick an adapter and device compatible with the surface. The error arms
    /// only run on a GPU failure, which a passing test can't force, so exclude
    /// them from coverage.
    #[cfg_attr(feature = "unstable", coverage(off))]
    async fn request_device(
        instance: &wgpu::Instance,
        surface: &Surface<'static>,
    ) -> Result<(Device, Queue), String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(surface),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("failed to request a wgpu adapter: {e:?}"))?;
        adapter
            .request_device(&Default::default())
            .await
            .map_err(|e| format!("failed to request a wgpu device: {e:?}"))
    }
}

/// The per-frame renderer, built once against a window's surface.
pub(super) struct Renderer {
    device: Device,
    queue: Queue,
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
    rect_buffer: Buffer,
    gpu_state: GpuState,
}

impl Renderer {
    /// Build a complete renderer against the window, driving the async device
    /// request to completion with `futures::executor::block_on`. This is
    /// native-only: on the web the request resolves on browser microtasks, so
    /// `block_on` (which parks the thread) would deadlock — the web instead
    /// runs `attach_device` as a `spawn_local` task.
    ///
    /// The `?` only fires on a GPU failure, which a passing test can't force,
    /// so exclude it from coverage.
    #[cfg_attr(feature = "unstable", coverage(off))]
    pub(super) fn for_window(window: &Arc<dyn Window>) -> Result<Renderer, String> {
        let unattached = Unattached::new_surface(window, window.surface_size())?;
        futures::executor::block_on(unattached.attach_device())
    }

    /// The surface's current configured size, in physical pixels.
    ///
    /// Used on the web to seed the first frame's rect uniform: the surface is
    /// configured at the window's size in `can_create_surfaces`, and
    /// `Window::surface_size` is not yet populated there.
    #[cfg(target_family = "wasm")]
    pub(super) fn surface_size(&self) -> (u32, u32) {
        (self.surface_config.width, self.surface_config.height)
    }

    /// Reconfigure the surface for a new window size. A zero size (which
    /// happens when the window is minimized) leaves the current configuration
    /// in place.
    pub(super) fn reconfigure(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config = SurfaceConfiguration {
            width,
            height,
            ..self.surface_config.clone()
        };
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Upload the (board px, window px) uniform the vertex shader uses to keep
    /// the board anchored at the top-left at `scale` px per cell.
    pub(super) fn update_rect(&mut self, board_px: (f32, f32), window_px: (u32, u32)) {
        let data = [
            board_px.0,
            board_px.1,
            window_px.0 as f32,
            window_px.1 as f32,
        ];
        self.queue
            .write_buffer(&self.rect_buffer, 0, bytemuck::cast_slice(&data));
    }

    /// Build the render pipeline and its bind group layout.
    fn create_pipeline(device: &Device) -> (RenderPipeline, BindGroupLayout) {
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
                    format: SURFACE_FORMAT,
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
        (pipeline, bind_group_layout)
    }

    /// Create a fresh cell texture for the board's current size, plus the
    /// sampler and bind group that bind it, then upload the current cells.
    pub(super) fn init_cell_texture<B: LifeBoard>(&mut self, state: &State<B>) {
        let (cols, rows) = (state.brd.cols(), state.brd.rows());
        let texture = Self::new_cell_texture(&self.device, cols, rows);
        let view = texture.create_view(&Default::default());
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cell sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cell bind group"),
            layout: &self.bind_group_layout,
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
                        buffer: &self.rect_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });
        self.gpu_state.cell = Some(Cell {
            texture,
            bind_group,
        });
        self.upload_cells(state);
    }

    /// Rebuild the cell texture if the board changed size (it does when a
    /// window resize pads the board), so it matches the board dimensions.
    fn resize_if_needed<B: LifeBoard>(&mut self, state: &State<B>) {
        let Some(cell) = self.gpu_state.cell.as_ref() else {
            return;
        };
        let (cols, rows) = (state.brd.cols(), state.brd.rows());
        let size = cell.texture.size();
        if size.width != cols as u32 || size.height != rows as u32 {
            self.init_cell_texture(state);
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
            format: SURFACE_FORMAT,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    /// Fill the reusable upload buffer with the board's cell colors.
    fn fill_cells<B: LifeBoard>(&mut self, state: &State<B>) {
        let cells = &mut self.gpu_state.cells;
        let len = state.brd.len();
        if cells.len() != len {
            cells.resize(len, Default::default());
        }
        for (i, live) in state.brd.iter().enumerate() {
            cells[i] = if live { LIVE_COLOR } else { DEAD_COLOR };
        }
    }

    /// Upload the board's cell colors to the cell texture.
    fn upload_cells<B: LifeBoard>(&mut self, state: &State<B>) {
        self.fill_cells(state);
        let Some(cell) = self.gpu_state.cell.as_ref() else {
            return;
        };
        let cells = &self.gpu_state.cells;
        let (cols, rows) = (state.brd.cols(), state.brd.rows());
        self.queue.write_texture(
            cell.texture.as_image_copy(),
            bytemuck::cast_slice(cells),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(cols as u32 * 4),
                rows_per_image: Some(rows as u32),
            },
            cell.texture.size(),
        );
    }

    /// Draw one frame: rebuild the cell texture if the board changed size,
    /// upload the current cells, then draw the cell texture anchored at the
    /// top-left and present.
    pub(super) fn draw<B: LifeBoard>(&mut self, state: &State<B>) {
        let (width, height) = (self.surface_config.width, self.surface_config.height);
        if width == 0 || height == 0 {
            return;
        }
        // Rebuild the cell texture (and its bind group) first if the board
        // changed size, then upload the cells; finally borrow the bind group
        // for this frame.
        self.resize_if_needed(state);
        self.upload_cells(state);
        let Some(cell) = self.gpu_state.cell.as_ref() else {
            return;
        };
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &cell.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }
}

#[cfg(test)]
mod tests {
    /// Headless check that the WGSL parses (no GPU or adapter needed, so it
    /// also runs in CI). Catches shader regressions — e.g. an unknown
    /// identifier — without launching the GUI.
    #[test]
    fn shader_parses() {
        naga::front::wgsl::parse_str(super::FRAGMENT_WGSL)
            .expect("the cell shader WGSL must parse");
    }
}
