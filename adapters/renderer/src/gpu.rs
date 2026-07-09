//! The shared `wgpu` draw path (ADR 0020 §2; issue #8 Phase 1).
//!
//! GPU code — **not** run in the gate (no GPU there, I9); it is exercised only
//! through the headless-PNG capture used by `/verify` and by the on-screen
//! workbench. The pure geometry, camera, light, and colour it draws are tested
//! separately in their own modules. This module builds the render pipeline and
//! vertex/uniform buffers once, then records a Lambert-shaded pass into any
//! colour+depth target — shared verbatim by the windowed and headless adapters.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use providence_config::{RenderParams, WaterParams};

use crate::camera::Camera;
use crate::light;
use crate::mesh::Mesh;
use crate::water::WaterPlane;

/// Depth buffer format for hidden-surface removal, shared by both targets.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The vertex buffer layout: three `vec3<f32>` attributes (position, normal,
/// colour) matching [`GpuVertex`] and the shader's `vs_main` locations.
const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];

/// The water plane's vertex layout: a `vec3<f32>` position plus a `f32`
/// water-column depth (ADR 0023, Phase 2 refinement) — the surface's colour and
/// shimmer come from the uniform, cued per-fragment off the interpolated depth.
const WATER_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32];

/// The flat-shaded terrain shader: transform by the view/projection matrix,
/// then Lambert diffuse (single directional light) plus ambient fill. Colours
/// are linear RGB; an sRGB render target does the encode on write.
const SHADER: &str = r"
struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    shading: vec4<f32>,
    // Hover highlight (issue #12): xyz = hovered vertex world position,
    // w = soft-disc radius (0 disables). rgb = glow tint, w = peak intensity.
    highlight_center: vec4<f32>,
    highlight_tint: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.color = color;
    out.world = position;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let l = normalize(u.light_dir.xyz);
    let diffuse = max(dot(n, l), 0.0) * u.shading.y;
    let shade = min(u.shading.x + diffuse, 1.0);
    var color = in.color * shade;

    // Hover highlight (issue #12): a soft pool of light on the surface under the
    // cursor. Measured by horizontal (xz) distance from the hovered vertex and
    // eased by a smoothstep to nothing at the disc rim — mirrors the canonical
    // `highlight::glow_falloff`. Additive, so it reads as light rather than paint.
    let radius = u.highlight_center.w;
    let intensity = u.highlight_tint.w;
    if radius > 0.0 && intensity > 0.0 {
        let center_xz = vec2<f32>(u.highlight_center.x, u.highlight_center.z);
        let d = distance(in.world.xz, center_xz);
        let s = clamp(1.0 - d / radius, 0.0, 1.0);
        let glow = s * s * (3.0 - 2.0 * s) * intensity;
        color = color + u.highlight_tint.rgb * glow;
    }
    return vec4<f32>(color, 1.0);
}
";

/// The water-surface shader (ADR 0023, Phase 2 + depth-cue refinement): transform
/// the flat plane, then colour it by the **depth of water beneath the fragment**.
/// The interpolated water-column depth (in height steps) drives a shallow→deep
/// ramp: the surface lerps from `color` (shallow rgb + opacity) to `deep` (deep
/// rgb + opacity) over the first `depth.x` steps, so a dug-out seabed reads as a
/// darker, more opaque patch of *deeper water* rather than a see-through crater —
/// the surface height itself never moves (the Director's ruling). A gentle
/// time-driven shimmer (`ripple` = amplitude, speed, scale, time) modulates the
/// brightness so the sea reads as *alive*. Alpha-blended over the terrain and
/// depth-tested so land above the waterline occludes it — the coastline for free.
/// Wall-clock time enters only here, at the edge (I3). Linear RGB; the sRGB
/// target encodes on write.
const WATER_SHADER: &str = r"
struct Water {
    view_proj: mat4x4<f32>,
    color: vec4<f32>,
    deep: vec4<f32>,
    depth: vec4<f32>,
    ripple: vec4<f32>,
};
@group(0) @binding(0) var<uniform> w: Water;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) column: f32,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) column: f32) -> VsOut {
    var out: VsOut;
    out.clip = w.view_proj * vec4<f32>(position, 1.0);
    out.world = position;
    out.column = column;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let amplitude = w.ripple.x;
    let speed = w.ripple.y;
    let scale = w.ripple.z;
    let time = w.ripple.w;
    // Depth cue: how far toward 'fully deep' this fragment's water column is.
    // depth.x (depth_full) is validated > 0, so the divide is safe; smoothstep
    // eases the shallow→deep transition rather than ramping it linearly.
    let t = smoothstep(0.0, 1.0, clamp(in.column / w.depth.x, 0.0, 1.0));
    let rgb = mix(w.color.rgb, w.deep.rgb, t);
    let alpha = mix(w.color.a, w.deep.a, t);
    // Two crossing travelling waves → a soft diagonal shimmer, in [-2, 2].
    let wave = sin(in.world.x * scale + time * speed)
             + sin(in.world.z * scale - time * speed * 0.8);
    let shimmer = 1.0 + wave * 0.5 * amplitude;
    return vec4<f32>(rgb * shimmer, alpha);
}
";

/// A GPU vertex, matching [`crate::mesh::Vertex`] with a `#[repr(C)]`,
/// `bytemuck`-castable layout for upload. Position, normal, colour — 9 floats.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 3],
}

/// The per-frame uniform block mirrored by `Uniforms` in the shader. `vec4`
/// padding keeps the std140 layout the GPU expects.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    shading: [f32; 4],
    /// Hover highlight (issue #12): `[x, y, z, radius]` — the hovered vertex
    /// world position and the soft-disc radius (`0` disables the glow).
    highlight_center: [f32; 4],
    /// Hover highlight tint: `[r, g, b, intensity]` — the glow colour and its
    /// peak added brightness.
    highlight_tint: [f32; 4],
}

/// A water-plane GPU vertex: a world-space position plus the water-column depth
/// at that grid point, in height steps (ADR 0023, Phase 2 refinement). Colour and
/// shimmer come from the uniform, cued off the depth. Matches
/// [`WATER_VERTEX_ATTRIBUTES`].
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuWaterVertex {
    position: [f32; 3],
    depth: f32,
}

/// The per-frame water uniform block mirrored by `Water` in [`WATER_SHADER`].
/// `color`/`deep` are the shallow/deep rgb + opacity (`a`) the surface ramps
/// between over the first `depth.x` steps of water column (ADR 0023, Phase 2
/// refinement); `ripple` is amplitude, speed, scale, time — the wall-clock time
/// supplied at the edge (I3). `depth.yzw` pad the std140 vec4.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct WaterUniforms {
    view_proj: [[f32; 4]; 4],
    color: [f32; 4],
    deep: [f32; 4],
    depth: [f32; 4],
    ripple: [f32; 4],
}

/// The prepared terrain draw: pipeline, buffers, and the presentation state
/// needed to update the camera each frame. Built once from a [`Mesh`] and the
/// [`RenderParams`]; drawn into whatever colour/depth views the target provides.
/// When a [`WaterPlane`] is supplied it also carries a translucent [`WaterPass`]
/// drawn over the terrain (ADR 0023, Phase 2).
pub struct TerrainScene {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    camera: Camera,
    background: wgpu::Color,
    ambient: f32,
    diffuse: f32,
    light_dir: [f32; 3],
    /// The alpha-blended water pass, present when the scene was built with a
    /// water plane (ADR 0023, Phase 2). Absent for a terrain-only capture (e.g.
    /// a mid-animation still built via `present_mesh` without a plane).
    water: Option<WaterPass>,
    /// The wall-clock time (seconds) the water shimmer is drawn at, supplied at
    /// the edge (I3). `0` unless the caller advances it each frame.
    time: f32,
    /// The hovered vertex's world position when the cursor is over the terrain
    /// (issue #12): the centre of the soft highlight glow, or `None` when the
    /// cursor points at empty space (the glow is then off). Adapter-local view
    /// state, like the camera — it never crosses the boundary (ADR 0020 §3).
    highlight_center: Option<[f32; 3]>,
    /// The highlight glow tint (`render.highlight.rgb`), linear RGB.
    highlight_rgb: [f32; 3],
    /// The highlight glow's soft-disc radius (`render.highlight.radius`).
    highlight_radius: f32,
    /// The highlight glow's peak added brightness (`render.highlight.intensity`).
    highlight_intensity: f32,
}

/// The prepared water draw (ADR 0023, Phase 2): its own pipeline, plane vertex
/// buffer, and uniform, plus the `render.water.*` colour and shimmer params it
/// uploads each frame. Recorded after the terrain pass, alpha-blended and
/// depth-tested so land above the waterline occludes it.
struct WaterPass {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    /// Shallow surface colour + opacity as `[r, g, b, a]` (linear RGB).
    color: [f32; 4],
    /// Deep surface colour + opacity as `[r, g, b, a]` — the depth cue ramps the
    /// surface toward this over `depth_full` steps of water column (ADR 0023,
    /// Phase 2 refinement).
    deep: [f32; 4],
    /// Water-column depth (in height steps) the deep cue saturates at
    /// (`render.water.depth_full`, validated > 0).
    depth_full: f32,
    /// Shimmer amplitude / speed / spatial scale (`render.water.ripple_*`).
    ripple_amplitude: f32,
    ripple_speed: f32,
    ripple_scale: f32,
}

impl TerrainScene {
    /// Build the pipeline and upload the mesh. `color_format` is the render
    /// target's texture format (the surface's for a window, `Rgba8UnormSrgb`
    /// for a capture). When `water` is `Some`, a translucent [`WaterPass`] is
    /// prepared over the terrain from `render.water.*` (ADR 0023, Phase 2).
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        params: &RenderParams,
        mesh: &Mesh,
        water: Option<&WaterPlane>,
    ) -> Self {
        let vertices: Vec<GpuVertex> = mesh
            .vertices
            .iter()
            .map(|v| GpuVertex {
                position: v.position,
                normal: v.normal,
                color: v.color,
            })
            .collect();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain-vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let vertex_count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain-bind-group-layout"),
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-bind-group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline = build_pipeline(device, color_format, &bind_group_layout);

        let background = params.background.rgb;
        let light_dir = light::direction(
            params.lighting.azimuth_degrees,
            params.lighting.elevation_degrees,
        );
        let water = water.map(|plane| WaterPass::new(device, color_format, &params.water, plane));
        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            vertex_buffer,
            vertex_count,
            camera: Camera::from_params(&params.camera),
            background: wgpu::Color {
                r: f64::from(background[0]),
                g: f64::from(background[1]),
                b: f64::from(background[2]),
                a: 1.0,
            },
            ambient: params.lighting.ambient,
            diffuse: params.lighting.diffuse,
            light_dir,
            water,
            time: 0.0,
            highlight_center: None,
            highlight_rgb: params.highlight.rgb,
            highlight_radius: params.highlight.radius,
            highlight_intensity: params.highlight.intensity,
        }
    }

    /// Advance the water shimmer to wall-clock `seconds` (ADR 0023, Phase 2). The
    /// window calls this each frame from an adapter-local clock so the sea moves;
    /// a headless capture leaves it at `0` for a deterministic still. The time
    /// only reaches the water shader — never the core (I3).
    pub fn set_time(&mut self, seconds: f32) {
        self.time = seconds;
    }

    /// Move the hover highlight to a hovered vertex's world position, or clear it
    /// (`None`) when the cursor leaves the terrain (issue #12). The window sets
    /// this as the cursor moves; the next [`update`](Self::update) uploads it into
    /// the terrain uniform. Adapter-local, like [`set_camera`](Self::set_camera)
    /// and [`set_time`](Self::set_time) — it never reaches the core (ADR 0020 §3).
    pub fn set_highlight(&mut self, center: Option<[f32; 3]>) {
        self.highlight_center = center;
    }

    /// Replace the view camera (issue #8 Phase 2). The window sets this from
    /// its [`OrbitController`](crate::camera::OrbitController) each time a drag
    /// or scroll moves the view; the next [`update`](Self::update) uploads the
    /// new view/projection matrix.
    pub fn set_camera(&mut self, camera: Camera) {
        self.camera = camera;
    }

    /// Replace the drawn geometry with a freshly built [`Mesh`] (issue #9
    /// Phase 2). After a shaping command mutates the height field, the renderer
    /// rebuilds the mesh from the new snapshot and re-uploads it here; the next
    /// [`draw`](Self::draw) shows the changed land. The grid dimensions never
    /// change under shaping, so this is a same-size vertex-buffer swap. Called
    /// only on a shaping click — user-paced, not per-frame — so recreating the
    /// buffer is cheap enough.
    pub fn set_mesh(&mut self, device: &wgpu::Device, mesh: &Mesh) {
        let vertices: Vec<GpuVertex> = mesh
            .vertices
            .iter()
            .map(|v| GpuVertex {
                position: v.position,
                normal: v.normal,
                color: v.color,
            })
            .collect();
        self.vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain-vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.vertex_count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
    }

    /// Re-upload the water plane after a shaping edit moved the seabed (ADR 0023,
    /// Phase 2 refinement). The surface height is unchanged, but the water-column
    /// depth beneath it shifts when land is raised or dug, so the depth cue must
    /// re-derive against the fresh heights — the water counterpart to
    /// [`set_mesh`](Self::set_mesh). A no-op when the scene carries no water pass
    /// (a terrain-only capture). User-paced, so rebuilding the buffer is cheap.
    pub fn set_water(&mut self, device: &wgpu::Device, plane: &WaterPlane) {
        if let Some(water) = &mut self.water {
            water.set_plane(device, plane);
        }
    }

    /// Recompute and upload the uniforms for a viewport of the given pixel
    /// size. Called on resize and before each draw so the projection tracks the
    /// surface's aspect ratio (and, in Phase 2, the live camera). Also refreshes
    /// the water uniform (same view/projection, plus the shimmer at the current
    /// [`set_time`](Self::set_time)) when a water pass is present.
    pub fn update(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        let aspect = aspect_ratio(width, height);
        let view_proj = self.camera.view_projection(aspect);
        // Pack the hover highlight: a hovered vertex places the glow at its world
        // position with the configured radius/tint; no hover leaves the intensity
        // at 0 so the shader draws no glow (issue #12).
        let (highlight_center, highlight_tint) = match self.highlight_center {
            Some([x, y, z]) => (
                [x, y, z, self.highlight_radius],
                [
                    self.highlight_rgb[0],
                    self.highlight_rgb[1],
                    self.highlight_rgb[2],
                    self.highlight_intensity,
                ],
            ),
            None => ([0.0; 4], [0.0; 4]),
        };
        let uniforms = Uniforms {
            view_proj,
            light_dir: [self.light_dir[0], self.light_dir[1], self.light_dir[2], 0.0],
            shading: [self.ambient, self.diffuse, 0.0, 0.0],
            highlight_center,
            highlight_tint,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        if let Some(water) = &self.water {
            let uniforms = WaterUniforms {
                view_proj,
                color: water.color,
                deep: water.deep,
                depth: [water.depth_full, 0.0, 0.0, 0.0],
                ripple: [
                    water.ripple_amplitude,
                    water.ripple_speed,
                    water.ripple_scale,
                    self.time,
                ],
            };
            queue.write_buffer(&water.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        }
    }

    /// Record the terrain pass into `color_view` (cleared to the background)
    /// with hidden-surface removal against `depth_view`, then — when a water pass
    /// is present — the translucent water plane alpha-blended over it (loading,
    /// not clearing, so land drawn above the waterline occludes the sea; ADR 0023
    /// Phase 2).
    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.background),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..self.vertex_count, 0..1);
        }

        if let Some(water) = &self.water {
            water.draw(encoder, color_view, depth_view);
        }
    }
}

/// Upload a [`WaterPlane`] as a GPU vertex buffer (position + water-column
/// depth), returning it with its vertex count — shared by [`WaterPass::new`] and
/// [`WaterPass::set_plane`] so the initial build and the shaping rebuild agree.
fn water_vertex_buffer(device: &wgpu::Device, plane: &WaterPlane) -> (wgpu::Buffer, u32) {
    let vertices: Vec<GpuWaterVertex> = plane
        .vertices()
        .iter()
        .map(|v| GpuWaterVertex {
            position: v.position,
            depth: v.depth,
        })
        .collect();
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("water-vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let vertex_count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
    (vertex_buffer, vertex_count)
}

impl WaterPass {
    /// Build the water pipeline and upload the plane vertices (ADR 0023,
    /// Phase 2). The colour/opacity and shimmer params are kept to be written
    /// into the uniform each frame with the live view/projection and time.
    fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        params: &WaterParams,
        plane: &WaterPlane,
    ) -> Self {
        let (vertex_buffer, vertex_count) = water_vertex_buffer(device, plane);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water-uniforms"),
            size: std::mem::size_of::<WaterUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water-bind-group-layout"),
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water-bind-group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline = build_water_pipeline(device, color_format, &bind_group_layout);

        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            vertex_buffer,
            vertex_count,
            color: [params.rgb[0], params.rgb[1], params.rgb[2], params.opacity],
            deep: [
                params.deep_rgb[0],
                params.deep_rgb[1],
                params.deep_rgb[2],
                params.deep_opacity,
            ],
            depth_full: params.depth_full,
            ripple_amplitude: params.ripple_amplitude,
            ripple_speed: params.ripple_speed,
            ripple_scale: params.ripple_scale,
        }
    }

    /// Re-upload the plane vertices after a shaping edit changed the seabed depth
    /// (ADR 0023, Phase 2 refinement). The surface stays one flat height, but the
    /// water-column depth beneath it moves when the seabed is raised or dug, so
    /// the depth cue must track it — the water twin of
    /// [`TerrainScene::set_mesh`]. Same-size grid, so a plain vertex-buffer swap.
    fn set_plane(&mut self, device: &wgpu::Device, plane: &WaterPlane) {
        let (vertex_buffer, vertex_count) = water_vertex_buffer(device, plane);
        self.vertex_buffer = vertex_buffer;
        self.vertex_count = vertex_count;
    }

    /// Record the water pass, loading the colour/depth the terrain pass wrote so
    /// the sea blends over the seabed but is hidden behind taller land.
    fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("water-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

/// Build the render pipeline: compile the shader, lay out the single uniform
/// bind group, and configure the flat-shaded, depth-tested triangle pass.
fn build_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("terrain-shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("terrain-pipeline-layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("terrain-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &VERTEX_ATTRIBUTES,
            }],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Build the water render pipeline (ADR 0023, Phase 2): the shimmer shader, the
/// single uniform bind group, and an **alpha-blended, depth-tested but
/// non-depth-writing** triangle pass. Reading (not writing) depth with a `Less`
/// compare draws the translucent sheet over the seabed (floated just above it by
/// `render.water.surface_lift`) while land above the waterline still occludes it.
fn build_water_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("water-shader"),
        source: wgpu::ShaderSource::Wgsl(WATER_SHADER.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("water-pipeline-layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("water-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GpuWaterVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &WATER_VERTEX_ATTRIBUTES,
            }],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Create the shared depth texture view for a target of the given size.
pub fn depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain-depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Aspect ratio `width / height`, guarding a zero-height viewport.
fn aspect_ratio(width: u32, height: u32) -> f32 {
    if height == 0 {
        1.0
    } else {
        width as f32 / height as f32
    }
}
