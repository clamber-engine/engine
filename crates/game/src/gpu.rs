//! Draws a frame's [`DrawList`] through the GPU at the window's own
//! resolution: the view scaled to fit at its own shape, letterboxed in
//! black, with smoothly filtered textures drawn from their mipmaps and
//! pixel art kept sharp, and edges smoothed by multisampling. Frames are
//! presented in step with the display's refresh when vsync is on.

use std::collections::HashMap;
use std::sync::Arc;

use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

use crate::canvas::Color;
use crate::display::letterbox;
use crate::report;
use crate::surface::{DrawList, Drawn, Quad, Texture, triangulate};

/// Every vertex is in view units, and samples its texture within
/// `bounds` so filtering doesn't reach into neighbouring frames. Colours
/// are premultiplied by alpha; `flash` replaces a texel's colour with the
/// vertex colour, keeping its alpha.
const SHADER: &str = "
struct View {
    size: vec2<f32>,
    // 1 when the target encodes to sRGB itself, so colours, which are
    // sRGB already, are decoded first.
    linear_out: f32,
    unused: f32,
}

@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var sampling: sampler;

struct In {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) bounds: vec4<f32>,
    @location(4) flash: f32,
}

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) bounds: vec4<f32>,
    @location(3) flash: f32,
}

@vertex
fn vertex(in: In) -> Out {
    let p = in.position / view.size;
    var out: Out;
    out.position = vec4<f32>(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    out.bounds = in.bounds;
    out.flash = in.flash;
    return out;
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let texel = textureSample(image, sampling, clamp(in.uv, in.bounds.xy, in.bounds.zw));
    let flashed = vec4<f32>(in.color.rgb * texel.a, texel.a);
    let out = mix(texel * in.color, flashed, in.flash);
    if view.linear_out > 0.5 && out.a > 0.0 {
        return vec4<f32>(decode(out.rgb / out.a) * out.a, out.a);
    }
    return out;
}
";

/// Samples per pixel, where the GPU can.
const SAMPLES: u32 = 4;

/// Frames a texture can go undrawn before it's dropped from the GPU.
const UNUSED_FRAMES: u64 = 600;

pub struct Gpu {
    window: Arc<Window>,
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    painter: Painter,
}

impl Gpu {
    /// Sets up drawing into `window`, with vsync or without.
    pub fn new(
        window: Arc<Window>,
        display: OwnedDisplayHandle,
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
        // Colours are sRGB already; draw them as they are if the window
        // can take them that way.
        let plain = config.format.remove_srgb_suffix();
        if surface.get_capabilities(&adapter).formats.contains(&plain) {
            config.format = plain;
        }
        config.present_mode = present_mode(vsync);
        surface.configure(&device, &config);
        let painter = Painter::new(&adapter, device, queue, config.format);
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

    /// Draws `list`. With vsync, waits for the display to be ready for
    /// another frame first. Draws nothing while the window has no area or
    /// can't be seen.
    pub fn present(&mut self, list: &DrawList) -> Result<(), String> {
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
        self.painter.paint(list, &frame.texture);
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

/// A texture on the GPU, ready to draw with.
struct Loaded {
    bind_group: wgpu::BindGroup,
    /// The last frame it was drawn in.
    used: u64,
}

/// Draws lists into textures of one format.
struct Painter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    samples: u32,
    view: wgpu::Buffer,
    view_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    smooth: wgpu::Sampler,
    sharp: wgpu::Sampler,
    /// One white texel, for solid shapes.
    white: wgpu::BindGroup,
    /// By [`Texture::id`].
    textures: HashMap<u64, Loaded>,
    /// Textures too large for this GPU, already warned about.
    too_large: Vec<u64>,
    vertices: wgpu::Buffer,
    /// The multisampled target, and its size.
    multisampled: Option<(wgpu::TextureView, (u32, u32))>,
    frame: u64,
}

/// Floats per vertex: position, uv, colour, bounds and flash.
const FLOATS: usize = 13;

impl Painter {
    fn new(
        adapter: &wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Painter {
        let samples = match adapter
            .get_texture_format_features(format)
            .flags
            .sample_count_supported(SAMPLES)
        {
            true => SAMPLES,
            false => 1,
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("draw"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("view"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
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
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("draw"),
            bind_group_layouts: &[Some(&view_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4, 4 => Float32
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("draw"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (FLOATS * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let view = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("view"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("view"),
            layout: &view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view.as_entire_binding(),
            }],
        });
        let sampler = |label, filter, mipmap_filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                mag_filter: filter,
                min_filter: filter,
                mipmap_filter,
                ..Default::default()
            })
        };
        let smooth = sampler(
            "smooth",
            wgpu::FilterMode::Linear,
            wgpu::MipmapFilterMode::Linear,
        );
        let sharp = sampler(
            "sharp",
            wgpu::FilterMode::Nearest,
            wgpu::MipmapFilterMode::Nearest,
        );
        let vertices = vertex_buffer(&device, 1 << 16);
        let mut texel = clamber_sprite::Pixels::new(1, 1);
        texel.data[0] = [255; 4];
        let white = upload(
            &device,
            &queue,
            &texture_layout,
            &sharp,
            &Texture::new(texel, true),
        )
        .expect("one texel fits");
        Painter {
            white,
            device,
            queue,
            pipeline,
            format,
            samples,
            view,
            view_group,
            texture_layout,
            smooth,
            sharp,
            textures: HashMap::new(),
            too_large: Vec::new(),
            vertices,
            multisampled: None,
            frame: 0,
        }
    }

    /// Whether `texture` is on the GPU now, putting it there if need be.
    fn load(&mut self, texture: &Texture) -> bool {
        let frame = self.frame;
        if let Some(loaded) = self.textures.get_mut(&texture.id()) {
            loaded.used = frame;
            return true;
        }
        if self.too_large.contains(&texture.id()) {
            return false;
        }
        let sampler = if texture.pixel_art {
            &self.sharp
        } else {
            &self.smooth
        };
        match upload(
            &self.device,
            &self.queue,
            &self.texture_layout,
            sampler,
            texture,
        ) {
            Some(bind_group) => {
                let loaded = Loaded {
                    bind_group,
                    used: frame,
                };
                self.textures.insert(texture.id(), loaded);
                true
            }
            None => {
                report::warn(format!(
                    "a {} by {} image is too large for this GPU, so it isn't drawn",
                    texture.width(),
                    texture.height()
                ));
                self.too_large.push(texture.id());
                false
            }
        }
    }

    /// Draws `list` into `target`, letterboxed in black.
    fn paint(&mut self, list: &DrawList, target: &wgpu::Texture) {
        self.frame += 1;
        let mut mesh = Mesh::default();
        let view = (list.view.0 as f32, list.view.1 as f32);
        mesh.rect([0.0, 0.0, view.0, view.1], list.background, 255);
        for item in &list.items {
            match item {
                Drawn::Rect {
                    edges,
                    color,
                    alpha,
                } => mesh.rect(*edges, *color, *alpha),
                Drawn::Line { a, b, color } => mesh.line(*a, *b, *color),
                Drawn::Polygon {
                    points,
                    color,
                    alpha,
                } => mesh.polygon(points, *color, *alpha),
                Drawn::Quad(quad) => {
                    if self.load(&quad.texture) {
                        mesh.quad(quad);
                    }
                }
            }
        }
        let frame = self.frame;
        self.textures.retain(|_, t| t.used + UNUSED_FRAMES >= frame);

        let bytes: Vec<u8> = mesh.floats.iter().flat_map(|f| f.to_le_bytes()).collect();
        if bytes.len() as u64 > self.vertices.size() {
            let size = (bytes.len() as u64).next_power_of_two();
            self.vertices = vertex_buffer(&self.device, size);
        }
        self.queue.write_buffer(&self.vertices, 0, &bytes);
        let linear_out = if self.format.is_srgb() { 1.0f32 } else { 0.0 };
        let uniform: Vec<u8> = [view.0, view.1, linear_out, 0.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        self.queue.write_buffer(&self.view, 0, &uniform);

        let size = (target.width(), target.height());
        if self.samples > 1 && self.multisampled.as_ref().is_none_or(|m| m.1 != size) {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("multisampled"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.samples,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.multisampled = Some((texture.create_view(&Default::default()), size));
        }
        let target_view = target.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.format),
            ..Default::default()
        });
        let (view_into, resolve_target) = match &self.multisampled {
            Some((multisampled, _)) if self.samples > 1 => (multisampled, Some(&target_view)),
            _ => (&target_view, None),
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: view_into,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let (x, y, w, h) = letterbox(list.view, size);
            pass.set_viewport(x, y, w, h, 0.0, 1.0);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.view_group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            for (texture, range) in &mesh.batches {
                let group = match texture {
                    Some(id) => &self.textures[id].bind_group,
                    None => &self.white,
                };
                pass.set_bind_group(1, group, &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
    }
}

/// Puts `texture` on the GPU, premultiplied by alpha, drawn with
/// `sampler`, or `None` if it's too large for it.
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    texture: &Texture,
) -> Option<wgpu::BindGroup> {
    let max = device.limits().max_texture_dimension_2d;
    if texture.width().max(texture.height()) > max {
        return None;
    }
    let size = wgpu::Extent3d {
        width: texture.width().max(1),
        height: texture.height().max(1),
        depth_or_array_layers: 1,
    };
    let gpu = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: texture.levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, image) in texture.levels.iter().enumerate() {
        let bytes: Vec<u8> = image
            .data
            .iter()
            .flat_map(|&[r, g, b, a]| {
                let p = |c: u8| ((c as u32 * a as u32 + 127) / 255) as u8;
                [p(r), p(g), p(b), a]
            })
            .collect();
        if bytes.is_empty() {
            continue;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
    }
    Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&gpu.create_view(&Default::default())),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    }))
}

fn vertex_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vertices"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Triangles to draw, in runs that share a texture.
#[derive(Default)]
struct Mesh {
    floats: Vec<f32>,
    /// Vertex ranges by texture, `None` for solid shapes.
    batches: Vec<(Option<u64>, std::ops::Range<u32>)>,
}

/// Where a vertex samples from, and the bounds samples stay within.
type Sample = ((f32, f32), [f32; 4]);

const SOLID: Sample = ((0.5, 0.5), [0.0, 0.0, 1.0, 1.0]);

impl Mesh {
    fn vertex(
        &mut self,
        texture: Option<u64>,
        at: (f32, f32),
        sample: Sample,
        color: [f32; 4],
        flash: f32,
    ) {
        let n = (self.floats.len() / FLOATS) as u32;
        match self.batches.last_mut() {
            Some((t, range)) if *t == texture => range.end = n + 1,
            _ => self.batches.push((texture, n..n + 1)),
        }
        let (uv, bounds) = sample;
        self.floats.extend([at.0, at.1, uv.0, uv.1]);
        self.floats.extend(color);
        self.floats.extend(bounds);
        self.floats.push(flash);
    }

    fn solid(&mut self, points: [(f32, f32); 3], color: Color, alpha: u8) {
        let color = premultiplied(color, alpha);
        for p in points {
            self.vertex(None, p, SOLID, color, 0.0);
        }
    }

    fn rect(&mut self, [l, t, r, b]: [f32; 4], color: Color, alpha: u8) {
        self.solid([(l, t), (r, t), (l, b)], color, alpha);
        self.solid([(r, t), (r, b), (l, b)], color, alpha);
    }

    /// A unit wide band along the line through the unit squares' centres.
    fn line(&mut self, a: (f32, f32), b: (f32, f32), color: Color) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = (dx * dx + dy * dy).sqrt();
        let (ux, uy) = if length > 0.0 {
            (dx / length * 0.5, dy / length * 0.5)
        } else {
            (0.5, 0.0)
        };
        let (nx, ny) = (-uy, ux);
        let (a, b) = (
            (a.0 + 0.5 - ux, a.1 + 0.5 - uy),
            (b.0 + 0.5 + ux, b.1 + 0.5 + uy),
        );
        let corners = [
            (a.0 + nx, a.1 + ny),
            (b.0 + nx, b.1 + ny),
            (b.0 - nx, b.1 - ny),
            (a.0 - nx, a.1 - ny),
        ];
        self.solid([corners[0], corners[1], corners[3]], color, 255);
        self.solid([corners[1], corners[2], corners[3]], color, 255);
    }

    fn polygon(&mut self, points: &[(f32, f32)], color: Color, alpha: u8) {
        for [a, b, c] in triangulate(points) {
            self.solid([points[a], points[b], points[c]], color, alpha);
        }
    }

    fn quad(&mut self, quad: &Quad) {
        let texture = &quad.texture;
        let size = (texture.width() as f32, texture.height() as f32);
        let src = quad.src;
        let (x, y, w, h) = (src.x as f32, src.y as f32, src.w as f32, src.h as f32);
        let bounds = if texture.pixel_art {
            // Just short of the far edges, which belong to the next texels.
            let shy = 1.0 / 64.0;
            [
                x / size.0,
                y / size.1,
                (x + w - shy) / size.0,
                (y + h - shy) / size.1,
            ]
        } else {
            [
                (x + 0.5) / size.0,
                (y + 0.5) / size.1,
                (x + w - 0.5).max(x + 0.5) / size.0,
                (y + h - 0.5).max(y + 0.5) / size.1,
            ]
        };
        let (color, flash) = match quad.flash {
            Some([r, g, b, _]) => ([r, g, b, 255].map(|c| c as f32 / 255.0), 1.0),
            None => ([1.0; 4], 0.0),
        };
        let drawn = quad.flip.size((w, h));
        let [l, t, r, b] = quad.dst;
        let corner = |cx: f32, cy: f32| {
            let (sx, sy) = quad.flip.source((cx * drawn.0, cy * drawn.1), (w, h));
            let uv = ((x + sx) / size.0, (y + sy) / size.1);
            ((l + (r - l) * cx, t + (b - t) * cy), (uv, bounds))
        };
        let id = Some(texture.id());
        for (cx, cy) in [
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
            (1.0, 0.0),
            (1.0, 1.0),
            (0.0, 1.0),
        ] {
            let (at, sample) = corner(cx, cy);
            self.vertex(id, at, sample, color, flash);
        }
    }
}

fn premultiplied(color: Color, alpha: u8) -> [f32; 4] {
    let a = alpha as f32 / 255.0;
    let c = |shift: u32| ((color >> shift) & 0xff) as f32 / 255.0 * a;
    [c(16), c(8), c(0), a]
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
    use clamber_sprite::{Pixels, Rect};
    use wgpu::naga;

    use super::*;
    use crate::canvas::rgb;
    use crate::surface::{Flip, Surface};

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

    /// `list` painted into a `width` by `height` texture, as 0x00RRGGBB,
    /// or `None` without a GPU (or a software one) to do it.
    fn paint(list: &DrawList, width: u32, height: u32) -> Option<Vec<u32>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let (adapter, device, queue) = open_device(&instance, None)
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
        let mut painter = Painter::new(&adapter, device.clone(), queue.clone(), format);
        painter.paint(list, &target);

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

    const RED: u32 = rgb(255, 0, 0);
    const BLUE: u32 = rgb(0, 0, 255);

    #[test]
    fn paints_the_view_scaled_and_letterboxed() {
        let mut list = DrawList::new((2, 1));
        list.clear(BLUE);
        list.fill_rect(0, 0, 1, 1, RED);
        let Some(out) = paint(&list, 6, 5) else {
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
        let mut list = DrawList::new((2, 2));
        list.clear(BLUE);
        list.fill_rect(0, 0, 1, 2, RED);
        let out = paint(&list, 5, 5).unwrap();
        for row in out.chunks(5) {
            assert_eq!(row[..2], [RED, RED]);
            assert_eq!(row[3..], [BLUE, BLUE]);
            let (r, g, b) = (row[2] >> 16, row[2] >> 8 & 0xff, row[2] & 0xff);
            assert!((96..=160).contains(&r) && g == 0 && (96..=160).contains(&b));
        }
    }

    #[test]
    fn draws_pixel_art_sharp_and_the_rest_smooth() {
        let mut image = Pixels::new(2, 1);
        image.data = vec![[255, 0, 0, 255], [0, 0, 255, 255]];
        let quad = |pixel_art| Quad {
            texture: Texture::new(image.clone(), pixel_art),
            src: Rect {
                x: 0,
                y: 0,
                w: 2,
                h: 1,
            },
            dst: [0.0, 0.0, 2.0, 1.0],
            flip: Flip {
                x: true,
                ..Flip::default()
            },
            flash: None,
        };
        let mut list = DrawList::new((2, 1));
        list.quad(&quad(true));
        let Some(out) = paint(&list, 8, 4) else {
            return;
        };
        // Mirrored, four pixels to a texel.
        assert!(out[..4].iter().all(|&c| c == BLUE));
        assert!(out[4..8].iter().all(|&c| c == RED));

        let mut list = DrawList::new((2, 1));
        list.quad(&quad(false));
        let out = paint(&list, 8, 4).unwrap();
        // Blue fading to red across the middle.
        assert_eq!(out[0], BLUE);
        assert_eq!(out[7], RED);
        let purple = out[3];
        assert!(purple >> 16 > 0x20 && purple & 0xff > 0x20, "{purple:06x}");
    }
}
