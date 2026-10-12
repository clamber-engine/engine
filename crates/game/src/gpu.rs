//! Shows the canvas in the window through the GPU: scaled to fit at its
//! own shape, with sharp edges between its pixels but no uneven ones
//! when the scale isn't whole, and presented in step with the display's
//! refresh when vsync is on.

use std::sync::Arc;

use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

use crate::canvas::Canvas;
use crate::display::letterbox;

/// Draws a texture over the viewport. Each window pixel samples the
/// texture bilinearly, but only within its own share of the edges
/// between texels, so pixels stay crisp at any scale.
const SHADER: &str = "
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var bilinear: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vertex(@builtin(vertex_index) i: u32) -> Out {
    let uv = vec2<f32>(f32(i & 1u), f32(i >> 1u));
    var out: Out;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(image));
    let texel = in.uv * size;
    let scale = max(1.0 / abs(vec2<f32>(dpdx(texel.x), dpdy(texel.y))), vec2<f32>(1.0));
    let edge = 0.5 - 0.5 / scale;
    let from_centre = fract(texel) - 0.5;
    let blend = (from_centre - clamp(from_centre, -edge, edge)) * scale + 0.5;
    let uv = (floor(texel) + blend) / size;
    return vec4<f32>(textureSample(image, bilinear, uv).rgb, 1.0);
}
";

pub struct Gpu {
    window: Arc<Window>,
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    painter: Painter,
}

impl Gpu {
    /// Sets up drawing a canvas of `canvas` pixels into `window`, with
    /// vsync or without.
    pub fn new(
        window: Arc<Window>,
        display: OwnedDisplayHandle,
        canvas: (u32, u32),
        vsync: bool,
    ) -> Result<Gpu, String> {
        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display)),
        );
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let (adapter, device, queue) = open_device(&instance, Some(&surface))?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("the GPU can't draw to this window")?;
        config.present_mode = present_mode(vsync);
        surface.configure(&device, &config);
        let painter = Painter::new(device, queue, canvas, config.format);
        Ok(Gpu {
            window,
            instance,
            surface,
            config,
            painter,
        })
    }

    pub fn vsync(&self) -> bool {
        self.config.present_mode == present_mode(true)
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        if vsync != self.vsync() {
            self.config.present_mode = present_mode(vsync);
            self.configure();
        }
    }

    fn configure(&self) {
        self.surface.configure(&self.painter.device, &self.config);
    }

    /// Shows `canvas`, letterboxed in black. With vsync, waits for the
    /// display to be ready for another frame first. Draws nothing while
    /// the window has no area or can't be seen.
    pub fn present(&mut self, canvas: &Canvas) -> Result<(), String> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        if (size.width, size.height) != (self.config.width, self.config.height) {
            self.config.width = size.width;
            self.config.height = size.height;
            self.configure();
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                // Shown as it is; set up again for the next frame.
                self.config.width = 0;
                frame
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .instance
                    .create_surface(self.window.clone())
                    .map_err(|e| e.to_string())?;
                self.configure();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("the GPU couldn't get a frame to draw".into());
            }
        };
        self.painter.paint(canvas, &frame.texture);
        self.painter.queue.present(frame);
        Ok(())
    }
}

/// An adapter, compatible with `surface` if there is one, and a device
/// on it.
fn open_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface>,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), String> {
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: surface,
        ..Default::default()
    }))
    .map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("clamber"),
        required_limits:
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .map_err(|e| e.to_string())?;
    Ok((adapter, device, queue))
}

/// Draws a canvas into textures of one format.
struct Painter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    image: wgpu::Texture,
    /// The canvas's pixels as the texture takes them, kept to reuse.
    bytes: Vec<u8>,
}

impl Painter {
    fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        canvas: (u32, u32),
        target: wgpu::TextureFormat,
    ) -> Painter {
        // The canvas's 0x00RRGGBB pixels are B, G, R, unused in memory.
        let format = if target.is_srgb() {
            wgpu::TextureFormat::Bgra8UnormSrgb
        } else {
            wgpu::TextureFormat::Bgra8Unorm
        };
        let image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("canvas"),
            size: wgpu::Extent3d {
                width: canvas.0,
                height: canvas.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bilinear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("present"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("present"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(target.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &image.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Painter {
            device,
            queue,
            pipeline,
            bind_group,
            image,
            bytes: Vec::new(),
        }
    }

    /// Draws `canvas` into `target`, letterboxed in black.
    fn paint(&mut self, canvas: &Canvas, target: &wgpu::Texture) {
        self.bytes.clear();
        self.bytes
            .extend(canvas.pixels.iter().flat_map(|p| p.to_le_bytes()));
        self.queue.write_texture(
            self.image.as_image_copy(),
            &self.bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(canvas.width * 4),
                rows_per_image: None,
            },
            self.image.size(),
        );

        let view = target.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let (x, y, w, h) = letterbox(
                (canvas.width, canvas.height),
                (target.width(), target.height()),
            );
            pass.set_viewport(x, y, w, h, 0.0, 1.0);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }
}

fn present_mode(vsync: bool) -> wgpu::PresentMode {
    if vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    }
}

#[cfg(test)]
mod tests {
    use wgpu::naga;

    use super::*;
    use crate::canvas::rgb;

    #[test]
    fn the_shader_is_valid() {
        let module = naga::front::wgsl::parse_str(SHADER).expect("parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validates");
    }

    /// `canvas` painted into a `width` by `height` texture, as
    /// 0x00RRGGBB, or `None` without a GPU (or a software one) to do it.
    fn paint(canvas: &Canvas, width: u32, height: u32) -> Option<Vec<u32>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let (_, device, queue) = open_device(&instance, None)
            .map_err(|e| eprintln!("skipped, no GPU: {e}"))
            .ok()?;
        let format = wgpu::TextureFormat::Bgra8Unorm;
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut painter = Painter::new(
            device.clone(),
            queue.clone(),
            (canvas.width, canvas.height),
            format,
        );
        painter.paint(canvas, &target);

        let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
            },
            target.size(),
        );
        queue.submit([encoder.finish()]);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let bytes = buffer.slice(..).get_mapped_range().unwrap();
        Some(
            bytes
                .chunks(row as usize)
                .flat_map(|r| r[..width as usize * 4].chunks(4))
                .map(|p| rgb(p[2], p[1], p[0]))
                .collect(),
        )
    }

    #[test]
    fn paints_the_canvas_scaled_and_letterboxed() {
        const RED: u32 = rgb(255, 0, 0);
        const BLUE: u32 = rgb(0, 0, 255);
        let mut canvas = Canvas::new(2, 1);
        canvas.pixels = vec![RED, BLUE];
        let Some(out) = paint(&canvas, 6, 5) else {
            return;
        };
        // Scaled 3 times, with a row of black above and below.
        #[rustfmt::skip]
        assert_eq!(out, [
            0, 0, 0, 0, 0, 0,
            RED, RED, RED, BLUE, BLUE, BLUE,
            RED, RED, RED, BLUE, BLUE, BLUE,
            RED, RED, RED, BLUE, BLUE, BLUE,
            0, 0, 0, 0, 0, 0,
        ]);

        // At 2.5 times, the window pixel on the edge between them blends
        // them, and the rest are each one colour.
        let mut canvas = Canvas::new(2, 2);
        canvas.pixels = vec![RED, BLUE, RED, BLUE];
        let out = paint(&canvas, 5, 5).unwrap();
        for row in out.chunks(5) {
            assert_eq!(row[..2], [RED, RED]);
            assert_eq!(row[3..], [BLUE, BLUE]);
            let (r, g, b) = (row[2] >> 16, row[2] >> 8 & 0xff, row[2] & 0xff);
            assert!((120..=135).contains(&r) && g == 0 && (120..=135).contains(&b));
        }
    }
}
