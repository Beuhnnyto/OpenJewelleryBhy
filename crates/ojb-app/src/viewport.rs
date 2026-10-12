//! GPU viewport: renders the scene off-screen with wgpu (Vulkan / DX12 /
//! Metal / GL, chosen by the platform) at 4× MSAA and hands the resolved
//! texture to egui. Also owns the orbit camera and CPU picking.

use crate::scene::{Scene, SceneNode, Vertex, MAT_UNLIT};
use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use glam::{Mat4, Vec3};
use ojb_core::document::NodeId;
use std::sync::Arc;
use wgpu::util::DeviceExt;

const SAMPLES: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const SHADER: &str = r#"
struct Globals { view_proj: mat4x4<f32>, eye: vec4<f32>, light: vec4<f32> };
@group(0) @binding(0) var<uniform> g: Globals;
struct Draw { tint: vec4<f32>, params: vec4<f32> };
@group(1) @binding(0) var<uniform> d: Draw;

struct VIn { @location(0) pos: vec3<f32>, @location(1) nrm: vec3<f32>, @location(2) col: vec4<f32> };
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) col: vec4<f32>,
};

@vertex fn vs(v: VIn) -> VOut {
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.world = v.pos;
    o.nrm = v.nrm;
    o.col = v.col;
    return o;
}

fn sky(r: vec3<f32>) -> vec3<f32> {
    // studio environment: bright softbox above, darker floor, a side strip
    let up = clamp(r.z * 0.5 + 0.5, 0.0, 1.0);
    let strip = smoothstep(0.75, 0.95, abs(r.x));
    return mix(vec3<f32>(0.12, 0.12, 0.14), vec3<f32>(1.0, 1.0, 1.0), up * up) + vec3<f32>(0.5) * strip;
}

@fragment fn fs(i: VOut, @builtin(front_facing) ff: bool) -> @location(0) vec4<f32> {
    let base = mix(i.col.rgb, d.tint.rgb, d.tint.a);
    var c = base;
    if (i.col.a < 1.5) {
        var n = normalize(i.nrm);
        if (!ff) { n = -n; }
        let v = normalize(g.eye.xyz - i.world);
        let l = normalize(g.light.xyz);
        let h = normalize(l + v);
        let ndv = max(dot(n, v), 0.0);
        let r = reflect(-v, n);
        if (i.col.a < 0.5) {
            // polished metal: tinted environment reflection + highlight
            let fres = base + (vec3<f32>(1.0) - base) * pow(1.0 - ndv, 5.0);
            let spec = pow(max(dot(n, h), 0.0), 90.0);
            c = fres * sky(r) * 0.85 + base * 0.12 * max(dot(n, l), 0.0) + vec3<f32>(spec) * 0.6;
        } else {
            // gem: facet-dependent sparkle
            let spark = pow(max(dot(n, h), 0.0), 30.0);
            let facet = 0.5 + 0.5 * sin(dot(n, vec3<f32>(17.0, 11.0, 7.0)));
            c = base * (0.25 + 0.45 * facet + 0.3 * sky(r).x) + vec3<f32>(spark) * 1.2;
        }
    }
    c = c / (c + vec3<f32>(0.6)) * 1.6; // gentle tone map
    return vec4<f32>(pow(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2)), d.params.x);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    light: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawUniform {
    tint: [f32; 4],
    params: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub fov: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { target: Vec3::new(0.0, 0.0, 4.0), yaw: 35f32.to_radians(), pitch: 22f32.to_radians(), dist: 60.0, fov: 30f32.to_radians() }
    }
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        let d = Vec3::new(self.yaw.sin() * self.pitch.cos(), -self.yaw.cos() * self.pitch.cos(), self.pitch.sin());
        self.target + d * self.dist
    }
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let up = if self.pitch.abs() > 1.5 { Vec3::Y } else { Vec3::Z };
        let near = (self.dist * 0.01).max(0.01);
        Mat4::perspective_rh(self.fov, aspect, near, self.dist * 20.0) * Mat4::look_at_rh(self.eye(), self.target, up)
    }
    pub fn fit(&mut self, b: &ojb_core::mesh::Aabb) {
        if b.is_empty() {
            return;
        }
        self.target = b.center().as_vec3();
        let r = (b.size().length() as f32 * 0.5).max(1.0);
        self.dist = r / (self.fov * 0.5).sin() * 1.1;
    }
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(-1.55, 1.55);
    }
    pub fn pan(&mut self, dx: f32, dy: f32, height_px: f32) {
        let eye = self.eye();
        let fwd = (self.target - eye).normalize();
        let up0 = if self.pitch.abs() > 1.5 { Vec3::Y } else { Vec3::Z };
        let right = fwd.cross(up0).normalize();
        let up = right.cross(fwd);
        let scale = 2.0 * self.dist * (self.fov * 0.5).tan() / height_px.max(1.0);
        self.target += (-right * dx + up * dy) * scale;
    }
    pub fn zoom(&mut self, steps: f32) {
        self.dist = (self.dist * (1.0 - steps * 0.0015).clamp(0.5, 1.5)).clamp(0.5, 2000.0);
    }
    /// World-space ray through a point given in normalised device coords.
    pub fn ray(&self, ndc: [f32; 2], aspect: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(aspect).inverse();
        let a = inv.project_point3(Vec3::new(ndc[0], ndc[1], 0.0));
        let b = inv.project_point3(Vec3::new(ndc[0], ndc[1], 1.0));
        (a, (b - a).normalize())
    }
}

struct Targets {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth: wgpu::TextureView,
    tex_id: egui::TextureId,
}

struct Draw {
    buf: wgpu::Buffer,
    bg: wgpu::BindGroup,
}

struct GpuNode {
    src: Arc<SceneNode>,
    metal: Option<(wgpu::Buffer, u32)>,
    gems: Option<(wgpu::Buffer, u32)>,
    cutters: Option<(wgpu::Buffer, u32)>,
    lines: Option<(wgpu::Buffer, u32)>,
    draw: Draw,
}

pub struct Gpu {
    opaque: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    draw_bgl: wgpu::BindGroupLayout,
    plain: Draw,
    cutter: Draw,
    overlay: Draw,
    grid: Option<(wgpu::Buffer, u32)>,
    nodes: Vec<GpuNode>,
    targets: Option<Targets>,
}

#[derive(Clone, Copy, Default)]
pub struct DrawOptions {
    pub selected: Option<NodeId>,
    pub hovered: Option<NodeId>,
    pub show_grid: bool,
    pub show_cutters: bool,
    pub show_curves: bool,
}

impl Gpu {
    pub fn new(rs: &egui_wgpu::RenderState) -> Gpu {
        let device = &rs.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ojb shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let globals_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("globals"), entries: &[uniform_entry] });
        let draw_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("draw"), entries: &[uniform_entry] });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ojb layout"),
            bind_group_layouts: &[Some(&globals_bgl), Some(&draw_bgl)],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];
        let vlayout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attrs,
        };
        let make = |topology, blend: bool, depth_write: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ojb pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(vlayout.clone())],
                },
                primitive: wgpu::PrimitiveState { topology, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: if blend { Some(wgpu::BlendState::ALPHA_BLENDING) } else { None },
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let opaque = make(wgpu::PrimitiveTopology::TriangleList, false, true);
        let blend = make(wgpu::PrimitiveTopology::TriangleList, true, false);
        let lines = make(wgpu::PrimitiveTopology::LineList, true, true);
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let mk_draw = |tint: [f32; 4], alpha: f32| make_draw(device, &draw_bgl, tint, alpha);
        Gpu {
            plain: mk_draw([0.0; 4], 1.0),
            cutter: mk_draw([0.9, 0.2, 0.15, 1.0], 0.28),
            overlay: mk_draw([0.0, 0.85, 0.95, 0.7], 0.35),
            opaque,
            blend,
            lines,
            globals,
            globals_bg,
            draw_bgl,
            grid: None,
            nodes: Vec::new(),
            targets: None,
        }
    }

    /// Uploads a new scene, reusing GPU buffers of unchanged nodes.
    pub fn upload(&mut self, rs: &egui_wgpu::RenderState, scene: &Scene) {
        let device = &rs.device;
        let mut old: Vec<GpuNode> = std::mem::take(&mut self.nodes);
        for sn in &scene.nodes {
            if let Some(pos) = old.iter().position(|g| Arc::ptr_eq(&g.src, sn)) {
                self.nodes.push(old.swap_remove(pos));
                continue;
            }
            let buf = |v: &Vec<Vertex>| -> Option<(wgpu::Buffer, u32)> {
                if v.is_empty() {
                    return None;
                }
                Some((
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("node"),
                        contents: bytemuck::cast_slice(v),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    v.len() as u32,
                ))
            };
            self.nodes.push(GpuNode {
                metal: buf(&sn.metal),
                gems: buf(&sn.gems),
                cutters: buf(&sn.cutters),
                lines: buf(&sn.lines),
                draw: make_draw(device, &self.draw_bgl, [0.0; 4], 1.0),
                src: sn.clone(),
            });
        }
        // grid under the model, 1 mm cells, 5 mm accents (as in MatrixGold)
        let b = scene.bbox;
        let (c, z, half) = if b.is_empty() {
            (Vec3::ZERO, 0.0, 15.0)
        } else {
            let s = b.size();
            (b.center().as_vec3(), b.min.z as f32 - 0.5, ((s.x.max(s.y) * 0.75) as f32).ceil().max(10.0))
        };
        let mut v = Vec::new();
        let n = half as i32;
        for i in -n..=n {
            let k = if i == 0 { 0.45 } else if i % 5 == 0 { 0.72 } else { 0.84 };
            let col = [k, k, k + 0.02, MAT_UNLIT];
            let (x, y) = (c.x.round() + i as f32, c.y.round() + i as f32);
            for p in [[x, c.y - half, z], [x, c.y + half, z], [c.x - half, y, z], [c.x + half, y, z]] {
                v.push(Vertex { pos: p, nrm: [0.0; 3], col });
            }
        }
        self.grid = Some((
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("grid"),
                contents: bytemuck::cast_slice(&v),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            v.len() as u32,
        ));
    }

    fn ensure_targets(&mut self, rs: &egui_wgpu::RenderState, size: [u32; 2]) {
        if self.targets.as_ref().map(|t| t.size) == Some(size) {
            return;
        }
        let device = &rs.device;
        let tex = |format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("viewport"),
                    size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let msaa = tex(COLOR_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolve = tex(COLOR_FORMAT, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
        let depth = tex(DEPTH_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let mut renderer = rs.renderer.write();
        let tex_id = match self.targets.take() {
            Some(t) => {
                renderer.update_egui_texture_from_wgpu_texture(device, &resolve, wgpu::FilterMode::Linear, t.tex_id);
                t.tex_id
            }
            None => renderer.register_native_texture(device, &resolve, wgpu::FilterMode::Linear),
        };
        self.targets = Some(Targets { size, msaa, resolve, depth, tex_id });
    }

    /// Renders and returns the egui texture to display.
    pub fn render(&mut self, rs: &egui_wgpu::RenderState, size: [u32; 2], cam: &Camera, opt: DrawOptions) -> egui::TextureId {
        let size = [size[0].clamp(16, 8192), size[1].clamp(16, 8192)];
        self.ensure_targets(rs, size);
        let q = &rs.queue;
        let aspect = size[0] as f32 / size[1] as f32;
        let eye = cam.eye();
        let light = (eye - cam.target).normalize() + Vec3::new(-0.3, 0.2, 0.9);
        let g = Globals {
            view_proj: cam.view_proj(aspect).to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, 1.0],
            light: [light.x, light.y, light.z, 0.0],
        };
        q.write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));
        for n in &self.nodes {
            let id = n.src.id;
            let tint = if Some(id) == opt.selected {
                [0.2, 0.75, 1.0, 0.16]
            } else if Some(id) == opt.hovered {
                [0.2, 0.75, 1.0, 0.1]
            } else {
                [0.0; 4]
            };
            q.write_buffer(&n.draw.buf, 0, bytemuck::bytes_of(&DrawUniform { tint, params: [1.0, 0.0, 0.0, 0.0] }));
        }
        let t = self.targets.as_ref().unwrap();
        let mut enc = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("viewport") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa,
                    depth_slice: None,
                    resolve_target: Some(&t.resolve),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.93, g: 0.94, b: 0.955, a: 1.0 }), store: wgpu::StoreOp::Discard },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            let draw = |pass: &mut wgpu::RenderPass<'_>, b: &Option<(wgpu::Buffer, u32)>| {
                if let Some((buf, n)) = b {
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..*n, 0..1);
                }
            };
            // opaque final model
            pass.set_pipeline(&self.opaque);
            for n in self.nodes.iter().filter(|n| n.src.in_final) {
                pass.set_bind_group(1, &n.draw.bg, &[]);
                draw(&mut pass, &n.metal);
                draw(&mut pass, &n.gems);
            }
            // grid + curves
            pass.set_pipeline(&self.lines);
            pass.set_bind_group(1, &self.plain.bg, &[]);
            if opt.show_grid {
                draw(&mut pass, &self.grid);
            }
            for n in &self.nodes {
                if opt.show_curves || Some(n.src.id) == opt.selected {
                    draw(&mut pass, &n.lines);
                }
            }
            // translucent: selected hidden node + cutters
            pass.set_pipeline(&self.blend);
            for n in &self.nodes {
                let sel = Some(n.src.id) == opt.selected || Some(n.src.id) == opt.hovered;
                if sel && !n.src.in_final {
                    pass.set_bind_group(1, &self.overlay.bg, &[]);
                    draw(&mut pass, &n.metal);
                    draw(&mut pass, &n.gems);
                }
                if opt.show_cutters || sel {
                    pass.set_bind_group(1, &self.cutter.bg, &[]);
                    draw(&mut pass, &n.cutters);
                }
            }
        }
        q.submit(Some(enc.finish()));
        t.tex_id
    }

    /// Picks the node under a ray (closest hit) and the hit point.
    pub fn pick(&self, origin: Vec3, dir: Vec3) -> Option<(NodeId, Vec3)> {
        let mut best: Option<(f32, NodeId)> = None;
        for n in self.nodes.iter().filter(|n| n.src.in_final) {
            if let Some(b) = n.src.bbox {
                if !ray_box(origin, dir, b.min.as_vec3() - 0.5, b.max.as_vec3() + 0.5) {
                    continue;
                }
            }
            for t in hit(&n.src.metal, origin, dir) {
                if best.map_or(true, |b| t.0 < b.0) {
                    best = Some((t.0, n.src.id));
                }
            }
            for t in hit(&n.src.gems, origin, dir) {
                if best.map_or(true, |b| t.0 < b.0) {
                    let owner = n.src.gem_owner.iter().find(|(r, _)| r.contains(&t.1)).map(|x| x.1).unwrap_or(n.src.id);
                    best = Some((t.0, owner));
                }
            }
        }
        best.map(|b| (b.1, origin + dir * b.0))
    }
}

fn make_draw(device: &wgpu::Device, bgl: &wgpu::BindGroupLayout, tint: [f32; 4], alpha: f32) -> Draw {
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("draw"),
        contents: bytemuck::bytes_of(&DrawUniform { tint, params: [alpha, 0.0, 0.0, 0.0] }),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("draw"),
        layout: bgl,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
    });
    Draw { buf, bg }
}

fn ray_box(o: Vec3, d: Vec3, lo: Vec3, hi: Vec3) -> bool {
    let inv = d.recip();
    let t1 = (lo - o) * inv;
    let t2 = (hi - o) * inv;
    let tmin = t1.min(t2).max_element();
    let tmax = t1.max(t2).min_element();
    tmax >= tmin.max(0.0)
}

/// Möller–Trumbore over a non-indexed triangle list; yields (t, vertex index).
fn hit<'a>(v: &'a [Vertex], o: Vec3, d: Vec3) -> impl Iterator<Item = (f32, usize)> + 'a {
    v.chunks_exact(3).enumerate().filter_map(move |(i, t)| {
        let a = Vec3::from(t[0].pos);
        let e1 = Vec3::from(t[1].pos) - a;
        let e2 = Vec3::from(t[2].pos) - a;
        let p = d.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        let s = o - a;
        let u = s.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = s.cross(e1);
        let w = d.dot(q) * inv;
        if w < 0.0 || u + w > 1.0 {
            return None;
        }
        let t = e2.dot(q) * inv;
        (t > 0.0).then_some((t, i * 3))
    })
}
