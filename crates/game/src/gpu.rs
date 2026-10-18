//! Draws a frame's [`DrawList`] through the GPU at the window's own
//! resolution: the view scaled to fit at its own shape, letterboxed in
//! black, with smoothly filtered textures drawn from their mipmaps and
//! pixel art kept sharp, quads on whole pixels, and edges smoothed by multisampling. Quads with
//! normal maps are lit in the shader just as the software canvas lights
//! them. Frames are presented in step with the display's refresh when
//! vsync is on.

use std::collections::HashMap;
use std::sync::Arc;

use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

use crate::canvas::Color;
use crate::display::{letterbox, pixel_fit};
use crate::report;
use crate::surface::{
    DrawList, Drawn, LIGHT_HEIGHT, Lighting, MAX_LIGHTS, Quad, Texture, triangulate,
};

/// Every vertex is in view units, and samples its texture within
/// `bounds` so filtering doesn't reach into neighbouring frames. Colours
/// are premultiplied by alpha; `flash` replaces a texel's colour with the
/// vertex colour, keeping its alpha times the vertex's. `lit` says whether to light the
/// texel by its normal (`w`), and how the quad is flipped (`xyz`, see
/// [`crate::surface::normal`]); [`Lighting::at`] is the same sum.
const SHADER: &str = "
struct View {
    size: vec2<f32>,
    // 1 when the target encodes to sRGB itself, so colours, which are
    // sRGB already, are decoded first.
    linear_out: f32,
    unused: f32,
}

struct Light {
    // x, y and radius.
    place: vec4<f32>,
    color: vec4<f32>,
}

struct Lighting {
    ambient: vec4<f32>,
    // w is how far in front point lights are.
    key_direction: vec4<f32>,
    key: vec4<f32>,
    // x is how many lights there are.
    count: vec4<f32>,
    lights: array<Light, 16>,
}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> lighting: Lighting;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var sampling: sampler;
@group(1) @binding(2) var normals: texture_2d<f32>;

struct In {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) bounds: vec4<f32>,
    @location(4) flash: f32,
    @location(5) lit: vec4<f32>,
}

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) bounds: vec4<f32>,
    @location(3) flash: f32,
    @location(4) at: vec2<f32>,
    @location(5) lit: vec4<f32>,
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
    out.at = in.position;
    out.lit = in.lit;
    return out;
}

fn light(n: vec3<f32>, at: vec2<f32>) -> vec3<f32> {
    var sum = lighting.ambient.rgb
        + lighting.key.rgb * max(dot(n, lighting.key_direction.xyz), 0.0);
    let count = u32(lighting.count.x);
    for (var i = 0u; i < count; i++) {
        let l = lighting.lights[i];
        let d = vec3<f32>(l.place.xy - at, lighting.key_direction.w);
        let reach = clamp(1.0 - length(d.xy) / l.place.z, 0.0, 1.0);
        sum += l.color.rgb * reach * reach * max(dot(n, normalize(d)), 0.0);
    }
    return sum;
}

// A texel of a normal map, premultiplied, in view axes, flipped.
fn normal(encoded: vec4<f32>, flip: vec3<f32>) -> vec3<f32> {
    var n = encoded.rgb / encoded.a * 2.0 - 1.0;
    n.y = -n.y;
    n.x = select(n.x, -n.x, flip.x > 0.5);
    n.y = select(n.y, -n.y, flip.y > 0.5);
    return select(n, n.yxz, flip.z > 0.5);
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let uv = clamp(in.uv, in.bounds.xy, in.bounds.zw);
    var texel = textureSample(image, sampling, uv);
    let encoded = textureSample(normals, sampling, uv);
    if in.lit.w > 0.5 && encoded.a > 0.0 {
        let n = normal(encoded, in.lit.xyz);
        if dot(n, n) > 0.0 {
            texel = vec4<f32>(texel.rgb * light(normalize(n), in.at), texel.a);
        }
    }
    let flashed = vec4<f32>(in.color.rgb * texel.a, texel.a) * in.color.a;
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
    view: wgpu::TextureView,
    pixel_art: bool,
    /// The last frame it was drawn in.
    used: u64,
}

/// What a batch draws with: a texture by [`Texture::id`], and its normal
/// map's, or `None` for solid shapes.
type Batch = Option<(u64, Option<u64>)>;

/// Draws lists into textures of one format.
struct Painter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    samples: u32,
    view: wgpu::Buffer,
    lighting: wgpu::Buffer,
    view_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    smooth: wgpu::Sampler,
    sharp: wgpu::Sampler,
    /// One white texel, for solid shapes.
    white: wgpu::BindGroup,
    /// One texel, for textures without a normal map.
    flat: wgpu::TextureView,
    /// By [`Texture::id`].
    textures: HashMap<u64, Loaded>,
    /// Textures paired with their normal maps, by what a batch draws with.
    groups: HashMap<(u64, Option<u64>), wgpu::BindGroup>,
    /// Textures too large for this GPU, already warned about.
    too_large: Vec<u64>,
    vertices: wgpu::Buffer,
    /// The multisampled target, and its size.
    multisampled: Option<(wgpu::TextureView, (u32, u32))>,
    /// Draws pixel art into [`Painter::low`]: one sample, and colours
    /// written as they are.
    low_pipeline: wgpu::RenderPipeline,
    /// The view's size for drawing into [`Painter::low`].
    low_uniform: wgpu::Buffer,
    low_view_group: wgpu::BindGroup,
    /// Pixel art is drawn into this, a pixel to a view unit, then scaled
    /// up from it: the target, what draws it, and its size.
    low: Option<(wgpu::TextureView, wgpu::BindGroup, (u32, u32))>,
    frame: u64,
}

/// Floats per vertex: position, uv, colour, bounds, flash and lit.
const FLOATS: usize = 17;

/// Bytes of the lighting uniform: four vectors, then each light's two.
const LIGHTING_BYTES: u64 = (4 + 2 * MAX_LIGHTS as u64) * 16;

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
            entries: &[0, 1].map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
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
            0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4, 4 => Float32,
            5 => Float32x4
        ];
        let pipeline_for = |format, samples| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            })
        };
        let pipeline = pipeline_for(format, samples);
        let low_pipeline = pipeline_for(format.remove_srgb_suffix(), 1);
        let view = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("view"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let lighting = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lighting"),
            size: LIGHTING_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let low_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("low view"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_group_for = |view: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("view"),
                layout: &view_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: view.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: lighting.as_entire_binding(),
                    },
                ],
            })
        };
        let view_group = view_group_for(&view);
        let low_view_group = view_group_for(&low_uniform);
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
        let white = upload(&device, &queue, &Texture::new(texel.clone(), true));
        let white = white.expect("one texel fits");
        let flat = upload(&device, &queue, &Texture::new(texel, true));
        let flat = flat.expect("one texel fits");
        let white = bind(&device, &texture_layout, &white, &flat, &sharp);
        Painter {
            white,
            flat,
            lighting,
            groups: HashMap::new(),
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
            low_pipeline,
            low_uniform,
            low_view_group,
            low: None,
            frame: 0,
        }
    }

    /// Makes sure [`Painter::low`] is `size` pixels.
    fn low_target(&mut self, size: (u32, u32)) {
        if self.low.as_ref().is_some_and(|low| low.2 == size) {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("low"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format.remove_srgb_suffix(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let group = bind(
            &self.device,
            &self.texture_layout,
            &view,
            &self.flat,
            &self.sharp,
        );
        self.low = Some((view, group, size));
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
        match upload(&self.device, &self.queue, texture) {
            Some(view) => {
                let loaded = Loaded {
                    view,
                    pixel_art: texture.pixel_art,
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

    /// Makes sure there's a bind group for `batch`, whose textures are
    /// loaded.
    fn group(&mut self, (texture, normals): (u64, Option<u64>)) {
        if self.groups.contains_key(&(texture, normals)) {
            return;
        }
        let image = &self.textures[&texture];
        let normals_view = normals.map_or(&self.flat, |n| &self.textures[&n].view);
        let sampler = if image.pixel_art {
            &self.sharp
        } else {
            &self.smooth
        };
        let group = bind(
            &self.device,
            &self.texture_layout,
            &image.view,
            normals_view,
            sampler,
        );
        self.groups.insert((texture, normals), group);
    }

    /// Draws `list` into `target`, letterboxed in black.
    fn paint(&mut self, list: &DrawList, target: &wgpu::Texture) {
        self.frame += 1;
        let size = (target.width(), target.height());
        let view = (list.view.0 as f32, list.view.1 as f32);
        let mut mesh = Mesh {
            pixels: match list.pixel_art {
                true => 1.0,
                false => letterbox(list.view, size).2 / view.0,
            },
            ..Mesh::default()
        };
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
                        let normals = quad.normals.as_ref().filter(|n| self.load(n));
                        let normals = normals.map(|n| n.id());
                        self.group((quad.texture.id(), normals));
                        mesh.quad(quad, normals);
                    }
                }
            }
        }
        let frame = self.frame;
        self.textures.retain(|_, t| t.used + UNUSED_FRAMES >= frame);
        let textures = &self.textures;
        self.groups.retain(|(texture, normals), _| {
            textures.contains_key(texture) && normals.is_none_or(|n| textures.contains_key(&n))
        });

        // Pixel art is drawn into the low target, then that is drawn into
        // the window with these vertices, after the rest.
        let scaled_up = (mesh.floats.len() / FLOATS) as u32;
        if list.pixel_art {
            mesh.floats.extend(scale_up(view));
            self.low_target(list.view);
        }

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
        let uniform: Vec<u8> = [view.0, view.1, 0.0, 0.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        self.queue.write_buffer(&self.low_uniform, 0, &uniform);
        self.queue
            .write_buffer(&self.lighting, 0, &lighting_bytes(&list.lighting));

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
        if let Some((low, _, _)) = self.low.as_ref().filter(|_| list.pixel_art) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("draw low"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: low,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.low_pipeline);
            pass.set_bind_group(0, &self.low_view_group, &[]);
            self.draw_batches(&mut pass, &mesh);
        }
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.view_group, &[]);
            match &self.low {
                Some((_, low, _)) if list.pixel_art => {
                    let (x, y, w, h) = pixel_fit(list.view, size);
                    pass.set_viewport(x, y, w, h, 0.0, 1.0);
                    pass.set_vertex_buffer(0, self.vertices.slice(..));
                    pass.set_bind_group(1, low, &[]);
                    pass.draw(scaled_up..scaled_up + 6, 0..1);
                }
                _ => {
                    // On whole pixels, as the quads' edges are.
                    let (x, y, w, h) = letterbox(list.view, size);
                    pass.set_viewport(x.floor(), y.floor(), w, h, 0.0, 1.0);
                    self.draw_batches(&mut pass, &mesh);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Draws `mesh`'s batches, whose vertices are in [`Painter::vertices`].
    fn draw_batches(&self, pass: &mut wgpu::RenderPass, mesh: &Mesh) {
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        for (batch, range) in &mesh.batches {
            let group = match batch {
                Some(key) => &self.groups[key],
                None => &self.white,
            };
            pass.set_bind_group(1, group, &[]);
            pass.draw(range.clone(), 0..1);
        }
    }
}

/// The lighting uniform's bytes, as the shader lays it out.
fn lighting_bytes(lighting: &Lighting) -> Vec<u8> {
    let rgb = |c: [f32; 3]| [c[0], c[1], c[2], 0.0];
    let [x, y, z] = lighting.key_direction;
    let count = lighting.lights.len().min(MAX_LIGHTS);
    let mut floats = Vec::with_capacity(LIGHTING_BYTES as usize / 4);
    floats.extend(rgb(lighting.ambient));
    floats.extend([x, y, z, LIGHT_HEIGHT]);
    floats.extend(rgb(lighting.key));
    floats.extend([count as f32, 0.0, 0.0, 0.0]);
    for l in &lighting.lights[..count] {
        floats.extend([l.at.0, l.at.1, l.radius, 0.0]);
        floats.extend(rgb(l.color));
    }
    floats.resize(LIGHTING_BYTES as usize / 4, 0.0);
    floats.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// A bind group drawing `image`, lit by `normals`, sampled with
/// `sampler`.
fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    image: &wgpu::TextureView,
    normals: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(image),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(normals),
            },
        ],
    })
}

/// Puts `texture` on the GPU, premultiplied by alpha, or `None` if it's
/// too large for it.
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &Texture,
) -> Option<wgpu::TextureView> {
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
    Some(gpu.create_view(&Default::default()))
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
    /// Vertex ranges by what they draw with.
    batches: Vec<(Batch, std::ops::Range<u32>)>,
    /// Pixels drawn into per view unit, to put quads' edges on whole
    /// pixels, or 0 to leave them where they are.
    pixels: f32,
}

/// Where a vertex samples from, and the bounds samples stay within.
type Sample = ((f32, f32), [f32; 4]);

/// A quad's two triangles, as corners of a unit square.
const CORNERS: [(f32, f32); 6] = [
    (0.0, 0.0),
    (1.0, 0.0),
    (0.0, 1.0),
    (1.0, 0.0),
    (1.0, 1.0),
    (0.0, 1.0),
];

const SOLID: Sample = ((0.5, 0.5), [0.0, 0.0, 1.0, 1.0]);

impl Mesh {
    fn vertex(
        &mut self,
        batch: Batch,
        at: (f32, f32),
        sample: Sample,
        color: [f32; 4],
        flash: f32,
        lit: [f32; 4],
    ) {
        let n = (self.floats.len() / FLOATS) as u32;
        match self.batches.last_mut() {
            Some((b, range)) if *b == batch => range.end = n + 1,
            _ => self.batches.push((batch, n..n + 1)),
        }
        let (uv, bounds) = sample;
        self.floats.extend([at.0, at.1, uv.0, uv.1]);
        self.floats.extend(color);
        self.floats.extend(bounds);
        self.floats.push(flash);
        self.floats.extend(lit);
    }

    fn solid(&mut self, points: [(f32, f32); 3], color: Color, alpha: u8) {
        let color = premultiplied(color, alpha);
        for p in points {
            self.vertex(None, p, SOLID, color, 0.0, [0.0; 4]);
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

    /// Draws `quad`, lit by the normal map with id `normals` if it has
    /// one on the GPU.
    fn quad(&mut self, quad: &Quad, normals: Option<u64>) {
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
        // Premultiplied, with its opacity in alpha for a flash.
        let opacity = quad.opacity as f32 / 255.0;
        let (color, flash) = match quad.flash {
            Some([r, g, b, _]) => ([r, g, b].map(|c| c as f32 / 255.0), 1.0),
            None => ([opacity; 3], 0.0),
        };
        let color = [color[0], color[1], color[2], opacity];
        let drawn = quad.flip.size((w, h));
        // Every quad's edges on whole pixels, so layers and sprites
        // scrolling between them don't shift against each other or
        // change shape from frame to frame (#190).
        let pixels = self.pixels;
        let [l, t, r, b] = match pixels > 0.0 {
            true => quad.dst.map(|v| (v * pixels).round() / pixels),
            false => quad.dst,
        };
        let corner = |cx: f32, cy: f32| {
            let (sx, sy) = quad.flip.source((cx * drawn.0, cy * drawn.1), (w, h));
            let uv = ((x + sx) / size.0, (y + sy) / size.1);
            ((l + (r - l) * cx, t + (b - t) * cy), (uv, bounds))
        };
        let batch = Some((texture.id(), normals));
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        let lit = [
            on(quad.flip.x),
            on(quad.flip.y),
            on(quad.flip.diagonal),
            on(normals.is_some() && quad.flash.is_none()),
        ];
        for (cx, cy) in CORNERS {
            let (at, sample) = corner(cx, cy);
            self.vertex(batch, at, sample, color, flash, lit);
        }
    }
}

/// The vertices of a texture covering a `view`, sampled sharp: the low
/// target drawn into the window.
fn scale_up(view: (f32, f32)) -> Vec<f32> {
    // Just short of the far edges, as for any pixel art.
    let shy = 1.0 / 64.0;
    let bounds = [0.0, 0.0, 1.0 - shy / view.0, 1.0 - shy / view.1];
    let mut mesh = Mesh::default();
    for (cx, cy) in CORNERS {
        let at = (cx * view.0, cy * view.1);
        mesh.vertex(None, at, ((cx, cy), bounds), [1.0; 4], 0.0, [0.0; 4]);
    }
    mesh.floats
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
    fn puts_quads_on_whole_pixels() {
        let quad = Quad {
            texture: Texture::new(Pixels::new(8, 8), false),
            src: Rect {
                x: 0,
                y: 0,
                w: 8,
                h: 8,
            },
            dst: [1.13, 2.29, 9.13, 10.29],
            flip: Flip::default(),
            flash: None,
            opacity: 255,
            normals: None,
        };
        // Corners as left, top, right and bottom.
        let edges = |pixels: f32| {
            let mut mesh = Mesh {
                pixels,
                ..Mesh::default()
            };
            mesh.quad(&quad, None);
            let at: Vec<(f32, f32)> = mesh.floats.chunks(FLOATS).map(|v| (v[0], v[1])).collect();
            let x = at.iter().map(|p| p.0);
            let y = at.iter().map(|p| p.1);
            [
                x.clone().fold(f32::MAX, f32::min),
                y.clone().fold(f32::MAX, f32::min),
                x.fold(f32::MIN, f32::max),
                y.fold(f32::MIN, f32::max),
            ]
        };
        let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5);
        // Five pixels to a unit, as a 384-unit view is shown at 1920.
        assert!(near(edges(5.0), [1.2, 2.2, 9.2, 10.2]), "{:?}", edges(5.0));
        // Pixel art: whole units.
        assert!(near(edges(1.0), [1.0, 2.0, 9.0, 10.0]));
        assert!(near(edges(0.0), quad.dst));
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
            opacity: 255,
            normals: None,
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

        // A pixel-art view is drawn a pixel to a unit, so even the smooth
        // texture is a texel to a pixel, then scaled up 3 times (not 3.5)
        // and centred.
        let mut list = DrawList::new((2, 1));
        list.pixel_art = true;
        list.quad(&quad(false));
        let out = paint(&list, 7, 5).unwrap();
        #[rustfmt::skip]
        assert_eq!(out, [
            0, 0, 0, 0, 0, 0, 0,
            BLUE, BLUE, BLUE, RED, RED, RED, 0,
            BLUE, BLUE, BLUE, RED, RED, RED, 0,
            BLUE, BLUE, BLUE, RED, RED, RED, 0,
            0, 0, 0, 0, 0, 0, 0,
        ]);
    }

    #[test]
    fn lights_quads_as_the_canvas_does() {
        let fill = |c: [u8; 4]| {
            let mut image = Pixels::new(4, 2);
            image.data.fill(c);
            image
        };
        // Facing up and towards the viewer, stored y up.
        let normals = fill([128, 218, 218, 255]);
        let lighting = Lighting {
            ambient: [0.25, 0.25, 0.25],
            key_direction: [0.0, -1.0, 0.0],
            key: [0.5, 0.0, 0.0],
            lights: vec![crate::surface::PointLight {
                at: (1.0, 2.5),
                radius: 3.0,
                color: [0.0, 0.0, 30.0],
            }],
        };
        let mut list = DrawList::new((4, 2));
        list.light(&lighting);
        for flip_y in [false, true] {
            list.quad(&Quad {
                texture: Texture::new(fill([200, 200, 200, 255]), false),
                src: Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 2,
                },
                dst: [0.0, if flip_y { 1.0 } else { 0.0 }, 4.0, 2.0],
                flip: Flip {
                    y: flip_y,
                    ..Flip::default()
                },
                flash: None,
                opacity: 255,
                normals: Some(Texture::new(normals.clone(), false)),
            });
        }
        let Some(out) = paint(&list, 8, 4) else {
            return;
        };
        let mut canvas = crate::Canvas::scaled((4, 2), 2);
        canvas.light(&lighting);
        for item in &list.items {
            let Drawn::Quad(quad) = item else { continue };
            canvas.quad(quad);
        }
        for (i, (&gpu, &cpu)) in out.iter().zip(&canvas.pixels).enumerate() {
            let channels = |c: u32| [c >> 16, c >> 8 & 0xff, c & 0xff];
            let near = channels(gpu)
                .iter()
                .zip(channels(cpu))
                .all(|(&a, b)| a.abs_diff(b) <= 3);
            assert!(
                near,
                "pixel {i}: {gpu:06x} on the GPU, {cpu:06x} in software"
            );
        }
        // Lit red from above on the top half; turned over, the bottom
        // half faces down and gets none, but the light below it is blue.
        let top = canvas.get(0, 0).unwrap();
        let bottom = canvas.get(2, 3).unwrap();
        assert!(top >> 16 > bottom >> 16, "{top:06x} {bottom:06x}");
        assert!(bottom & 0xff > top & 0xff, "{top:06x} {bottom:06x}");
    }
}
