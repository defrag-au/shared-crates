//! wgpu backend — forward-raster a [`DrawList`].
//!
//! **Antipattern:** GPU → CPU readback → GPU upload to show a pane. The
//! document is already a forward projection. Interactive viewers must
//! [`Gpu::from_device`] on the window's device and [`Gpu::present_list`]
//! into a sampled texture. [`Gpu::rasterize`] (CPU bytes) is for tests.

use std::collections::HashMap;

use scenegraph::{
    DrawList, Error, Fonts, Geometry, Images, Mat4, Materials, RgbaImage, Scene, Shade,
    SubmitCensus, Vec3, WorldLamp, WorldMesh, MAX_DIR_LIGHTS, MAX_POINT_LIGHTS,
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
    nrm: [f32; 3],
    /// 1 = Lambert, 0 = Unlit. Document selects the model; this is the program.
    lit: f32,
    /// 1 = sample atlas as MSDF (labels). 0 = albedo.
    sdf: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuDirLight {
    dir: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuPointLight {
    pos_range: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    dir_count: u32,
    point_count: u32,
    _pad: [u32; 2],
    dirs: [GpuDirLight; 4],
    points: [GpuPointLight; 4],
}

struct Batch {
    key: Option<String>,
    first: u32,
    count: u32,
    sdf: bool,
}

struct SubmitCache {
    geo_gen: u64,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    ubuf: wgpu::Buffer,
    bind0: wgpu::BindGroup,
    tex_binds: Vec<wgpu::BindGroup>,
    batches: Vec<Batch>,
    stats: PresentStats,
}

struct Gathered {
    verts: Vec<GpuVertex>,
    /// u32: one scene's meshes, quads and glyphs share this buffer, and
    /// a few models pass 65,535 vertices between them.
    indices: Vec<u32>,
    batches: Vec<Batch>,
    stats: PresentStats,
}

struct CachedTex {
    view: wgpu::TextureView,
    w: u32,
    h: u32,
    bytes: usize,
    _tex: wgpu::Texture,
}

struct Targets {
    width: u32,
    height: u32,
    color: wgpu::Texture,
    /// `Rgba8Unorm` view of [`Self::color`] for egui native textures.
    egui_view: wgpu::TextureView,
    msaa: Option<wgpu::Texture>,
    depth: wgpu::Texture,
    read: Option<wgpu::Buffer>,
    bytes_per_row: u32,
}

/// What the GPU actually submitted. Dump-shaped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PresentStats {
    pub quads: u32,
    pub meshes: u32,
    pub labels: u32,
    pub verts: u32,
    pub indices: u32,
    pub batches: u32,
    /// Vertex/index buffers reused; only the camera uniform was written.
    pub cached: bool,
}

impl PresentStats {
    pub fn dump(&self) -> String {
        format!(
            "submit\nquads={} meshes={} labels={} verts={} idx={} batches={} geo={}\n",
            self.quads,
            self.meshes,
            self.labels,
            self.verts,
            self.indices,
            self.batches,
            if self.cached { "cached" } else { "rebuild" },
        )
    }
}

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    uniform_layout: wgpu::BindGroupLayout,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    font_sampler: wgpu::Sampler,
    white_view: wgpu::TextureView,
    _white: wgpu::Texture,
    textures: HashMap<String, CachedTex>,
    font_textures: HashMap<String, CachedTex>,
    font_rev: HashMap<String, usize>,
    targets: Option<Targets>,
    sample_count: u32,
    submit: Option<SubmitCache>,
}

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

impl Gpu {
    pub fn new() -> Result<Self, Error> {
        pollster::block_on(Self::new_async())
    }

    pub async fn new_async() -> Result<Self, Error> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| Error::Gpu(e.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| Error::Gpu(e.to_string()))?;
        Self::from_device(&adapter, device, queue)
    }

    /// Share the window's device. Interactive panes must use this — not a
    /// second adapter plus CPU readback.
    pub fn from_device(
        adapter: &wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
    ) -> Result<Self, Error> {
        let sample_count = if adapter
            .get_texture_format_features(COLOR_FORMAT)
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4)
        {
            4
        } else {
            1
        };

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sg"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sg-u"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sg-t"),
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
            label: Some("sg-pl"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sg-rp"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                // wgpu 30: `VertexState.buffers` is `&[Option<VertexBufferLayout>]`.
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Float32x3, 4 => Float32, 5 => Float32],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let font_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            anisotropy_clamp: 1,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let white = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sg-white"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &white,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let white_view = white.create_view(&Default::default());
        Ok(Self {
            device,
            queue,
            pipeline,
            uniform_layout,
            tex_layout,
            sampler,
            font_sampler,
            white_view,
            _white: white,
            textures: HashMap::new(),
            font_textures: HashMap::new(),
            font_rev: HashMap::new(),
            targets: None,
            sample_count,
            submit: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rasterize(
        &mut self,
        scene: &Scene,
        images: &Images,
        geom: &Geometry,
        materials: &Materials,
        fonts: &mut Fonts,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, Error> {
        let list = scene.draw_list_with(geom, materials, fonts)?;
        self.rasterize_list(&list, images, fonts, width, height)
    }

    pub fn rasterize_list(
        &mut self,
        list: &DrawList,
        images: &Images,
        fonts: &Fonts,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, Error> {
        self.present_list(list, images, fonts, width, height)?;
        self.read_color(width, height)
    }

    /// Forward raster on this device. No CPU readback. The colour target is
    /// sampled via [`Self::egui_view`]. Rebuilds GPU buffers every call
    /// (tests; frustum-culls). Interactive panes use [`Self::present_list_geo`].
    pub fn present_list(
        &mut self,
        list: &DrawList,
        images: &Images,
        fonts: &Fonts,
        width: u32,
        height: u32,
    ) -> Result<PresentStats, Error> {
        self.present(list, images, fonts, width, height, None)
    }

    /// Like [`Self::present_list`], but world verts live in one GPU upload per
    /// `geo_gen`. Same gen + new camera writes the uniform buffer only.
    pub fn present_list_geo(
        &mut self,
        list: &DrawList,
        images: &Images,
        fonts: &Fonts,
        width: u32,
        height: u32,
        geo_gen: u64,
    ) -> Result<PresentStats, Error> {
        self.present(list, images, fonts, width, height, Some(geo_gen))
    }

    fn present(
        &mut self,
        list: &DrawList,
        images: &Images,
        fonts: &Fonts,
        width: u32,
        height: u32,
        geo_gen: Option<u64>,
    ) -> Result<PresentStats, Error> {
        let aspect = width as f32 / height.max(1) as f32;
        let vp = list.view_proj(aspect);
        let uniforms = pack_lights(list, vp);
        self.ensure_targets(width, height);

        if let Some(gen) = geo_gen {
            if let Some(mut cache) = self.submit.take() {
                if cache.geo_gen == gen {
                    self.queue
                        .write_buffer(&cache.ubuf, 0, bytemuck::bytes_of(&uniforms));
                    let dirty = self.refresh_batch_textures(&cache.batches, images, fonts);
                    if dirty {
                        cache.tex_binds = self.make_tex_binds(&cache.batches);
                    }
                    cache.stats.cached = true;
                    self.encode_draw(&cache);
                    let stats = cache.stats.clone();
                    self.submit = Some(cache);
                    return Ok(stats);
                }
            }
        }

        let cull = geo_gen.is_none();
        let mut gathered = gather(list, cull, vp);
        if gathered.verts.is_empty() {
            self.submit = None;
            self.submit_clear();
            return Ok(gathered.stats);
        }
        gathered.stats.batches = gathered.batches.len() as u32;
        gathered.stats.cached = false;

        let vbuf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sg-v"),
                contents: bytemuck::cast_slice(&gathered.verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let ibuf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sg-i"),
                contents: bytemuck::cast_slice(&gathered.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let ubuf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sg-u"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let bind0 = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sg-bg0"),
            layout: &self.uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ubuf.as_entire_binding(),
            }],
        });
        self.refresh_batch_textures(&gathered.batches, images, fonts);
        let tex_binds = self.make_tex_binds(&gathered.batches);
        let cache = SubmitCache {
            geo_gen: geo_gen.unwrap_or(0),
            vbuf,
            ibuf,
            ubuf,
            bind0,
            tex_binds,
            batches: gathered.batches,
            stats: gathered.stats.clone(),
        };
        self.encode_draw(&cache);
        let stats = cache.stats.clone();
        self.submit = Some(cache);
        Ok(stats)
    }

    fn refresh_batch_textures(
        &mut self,
        batches: &[Batch],
        images: &Images,
        fonts: &Fonts,
    ) -> bool {
        let mut dirty = false;
        for batch in batches {
            if batch.sdf {
                if let Some(k) = batch.key.as_deref() {
                    dirty |= self.ensure_font_tex(k, fonts);
                }
            } else {
                dirty |= self.ensure_tex(batch.key.as_deref(), images);
            }
        }
        dirty
    }

    fn make_tex_binds(&self, batches: &[Batch]) -> Vec<wgpu::BindGroup> {
        let mut tex_binds = Vec::with_capacity(batches.len());
        for batch in batches {
            let view = if batch.sdf {
                batch
                    .key
                    .as_deref()
                    .and_then(|k| self.font_textures.get(k))
                    .map(|c| &c.view)
                    .unwrap_or(&self.white_view)
            } else {
                batch
                    .key
                    .as_deref()
                    .and_then(|k| self.textures.get(k))
                    .map(|c| &c.view)
                    .unwrap_or(&self.white_view)
            };
            tex_binds.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sg-bg1"),
                layout: &self.tex_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(if batch.sdf {
                            &self.font_sampler
                        } else {
                            &self.sampler
                        }),
                    },
                ],
            }));
        }
        tex_binds
    }

    fn encode_draw(&mut self, cache: &SubmitCache) {
        let color_view = self
            .targets
            .as_ref()
            .unwrap()
            .color
            .create_view(&Default::default());
        let depth_view = self
            .targets
            .as_ref()
            .unwrap()
            .depth
            .create_view(&Default::default());
        let msaa_view = self
            .targets
            .as_ref()
            .unwrap()
            .msaa
            .as_ref()
            .map(|t| t.create_view(&Default::default()));

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sg-e"),
            });
        {
            let (pass_view, resolve_target, store) = match &msaa_view {
                Some(msaa) => (msaa, Some(&color_view), wgpu::StoreOp::Discard),
                None => (&color_view, None, wgpu::StoreOp::Store),
            };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sg-p"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: pass_view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
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
            pass.set_bind_group(0, &cache.bind0, &[]);
            pass.set_vertex_buffer(0, cache.vbuf.slice(..));
            pass.set_index_buffer(cache.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            for (i, batch) in cache.batches.iter().enumerate() {
                pass.set_bind_group(1, &cache.tex_binds[i], &[]);
                pass.draw_indexed(batch.first..batch.first + batch.count, 0, 0..1);
            }
        }
        self.queue.submit(Some(enc.finish()));
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// `Rgba8Unorm` view of the last [`Self::present_list`] target (egui native).
    pub fn egui_view(&self) -> Option<&wgpu::TextureView> {
        self.targets.as_ref().map(|t| &t.egui_view)
    }

    fn submit_clear(&mut self) {
        let color_view = self
            .targets
            .as_ref()
            .unwrap()
            .color
            .create_view(&Default::default());
        let depth_view = self
            .targets
            .as_ref()
            .unwrap()
            .depth
            .create_view(&Default::default());
        let msaa_view = self
            .targets
            .as_ref()
            .unwrap()
            .msaa
            .as_ref()
            .map(|t| t.create_view(&Default::default()));
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sg-clear"),
            });
        {
            let (pass_view, resolve_target, store) = match &msaa_view {
                Some(msaa) => (msaa, Some(&color_view), wgpu::StoreOp::Discard),
                None => (&color_view, None, wgpu::StoreOp::Store),
            };
            let _pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sg-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: pass_view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
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
        }
        self.queue.submit(Some(enc.finish()));
    }

    fn read_color(&mut self, width: u32, height: u32) -> Result<Vec<u8>, Error> {
        self.ensure_read_buffer(width, height);
        let bytes_per_row = self.targets.as_ref().expect("targets").bytes_per_row;
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sg-read"),
            });
        {
            let targets = self.targets.as_ref().expect("targets");
            let read = targets.read.as_ref().expect("read");
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &targets.color,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: read,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bytes_per_row),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.queue.submit(Some(enc.finish()));
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        {
            let targets = self.targets.as_ref().expect("targets");
            let read = targets.read.as_ref().expect("read");
            let slice = read.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| Error::Gpu(e.to_string()))?;
            // wgpu 30: `get_mapped_range` returns a Result.
            let data = slice
                .get_mapped_range()
                .map_err(|e| Error::Gpu(e.to_string()))?;
            for row in 0..height {
                let start = (row * bytes_per_row) as usize;
                rgba.extend_from_slice(&data[start..start + (width * 4) as usize]);
            }
        }
        self.targets
            .as_ref()
            .expect("targets")
            .read
            .as_ref()
            .expect("read")
            .unmap();
        Ok(rgba)
    }

    fn ensure_font_tex(&mut self, key: &str, fonts: &Fonts) -> bool {
        let rev = fonts.glyph_count(key).unwrap_or(0);
        if self.font_rev.get(key).copied() == Some(rev) && self.font_textures.contains_key(key) {
            return false;
        }
        let Some(img) = fonts.atlas(key) else {
            self.font_textures.remove(key);
            self.font_rev.remove(key);
            return true;
        };
        let cached = upload_rgba_nomip(&self.device, &self.queue, &img);
        self.font_textures.insert(key.to_string(), cached);
        self.font_rev.insert(key.to_string(), rev);
        true
    }

    fn ensure_tex(&mut self, key: Option<&str>, images: &Images) -> bool {
        let Some(k) = key else {
            return false;
        };
        let Some(img) = images.get(k) else {
            return self.textures.remove(k).is_some();
        };
        if self
            .textures
            .get(k)
            .is_some_and(|c| c.w == img.width && c.h == img.height && c.bytes == img.pixels.len())
        {
            return false;
        }
        let cached = upload_rgba(&self.device, &self.queue, img);
        self.textures.insert(k.to_string(), cached);
        true
    }

    fn ensure_targets(&mut self, width: u32, height: u32) {
        if self
            .targets
            .as_ref()
            .is_some_and(|t| t.width == width && t.height == height)
        {
            return;
        }
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sg-c"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });
        let egui_view = color.create_view(&wgpu::TextureViewDescriptor {
            label: Some("sg-egui"),
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        });
        let msaa = (self.sample_count > 1).then(|| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sg-msaa"),
                size,
                mip_level_count: 1,
                sample_count: self.sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: COLOR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sg-d"),
            size,
            mip_level_count: 1,
            sample_count: self.sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let bytes_per_row = (width * 4).div_ceil(256) * 256;
        self.targets = Some(Targets {
            width,
            height,
            color,
            egui_view,
            msaa,
            depth,
            read: None,
            bytes_per_row,
        });
    }

    fn ensure_read_buffer(&mut self, width: u32, height: u32) {
        let bytes_per_row = (width * 4).div_ceil(256) * 256;
        let size = bytes_per_row as u64 * height as u64;
        let need = self
            .targets
            .as_ref()
            .is_none_or(|t| t.read.as_ref().is_none_or(|b| b.size() != size));
        if !need {
            return;
        }
        let read = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sg-r"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        if let Some(t) = self.targets.as_mut() {
            t.read = Some(read);
            t.bytes_per_row = bytes_per_row;
        }
    }
}

/// Gribb–Hartmann planes from a column-major view-proj. Sphere vs frustum
/// so present skips what the camera cannot see; hit still uses the full list.
fn sphere_in_frustum(vp: Mat4, p: Vec3, radius: f32) -> bool {
    let planes = [
        (
            vp[0][3] + vp[0][0],
            vp[1][3] + vp[1][0],
            vp[2][3] + vp[2][0],
            vp[3][3] + vp[3][0],
        ),
        (
            vp[0][3] - vp[0][0],
            vp[1][3] - vp[1][0],
            vp[2][3] - vp[2][0],
            vp[3][3] - vp[3][0],
        ),
        (
            vp[0][3] + vp[0][1],
            vp[1][3] + vp[1][1],
            vp[2][3] + vp[2][1],
            vp[3][3] + vp[3][1],
        ),
        (
            vp[0][3] - vp[0][1],
            vp[1][3] - vp[1][1],
            vp[2][3] - vp[2][1],
            vp[3][3] - vp[3][1],
        ),
        (
            vp[0][3] + vp[0][2],
            vp[1][3] + vp[1][2],
            vp[2][3] + vp[2][2],
            vp[3][3] + vp[3][2],
        ),
        (
            vp[0][3] - vp[0][2],
            vp[1][3] - vp[1][2],
            vp[2][3] - vp[2][2],
            vp[3][3] - vp[3][2],
        ),
    ];
    for (a, b, c, d) in planes {
        let len = (a * a + b * b + c * c).sqrt().max(1e-8);
        if (a * p.x + b * p.y + c * p.z + d) / len < -radius {
            return false;
        }
    }
    true
}

fn shade_lit(shade: Shade) -> f32 {
    match shade {
        Shade::Lambert => 1.0,
        Shade::Unlit => 0.0,
    }
}

fn pack_lights(list: &DrawList, vp: Mat4) -> Uniforms {
    let mut u = Uniforms {
        view_proj: vp,
        dir_count: 0,
        point_count: 0,
        _pad: [0; 2],
        dirs: [GpuDirLight {
            dir: [0.0; 4],
            color: [0.0; 4],
        }; 4],
        points: [GpuPointLight {
            pos_range: [0.0; 4],
            color: [0.0; 4],
        }; 4],
    };
    for l in &list.lights {
        let rgb = [
            srgb_u8_to_lin(l.color[0]) * l.intensity,
            srgb_u8_to_lin(l.color[1]) * l.intensity,
            srgb_u8_to_lin(l.color[2]) * l.intensity,
            0.0,
        ];
        match l.lamp {
            WorldLamp::Dir { dir } => {
                let i = u.dir_count as usize;
                if i < MAX_DIR_LIGHTS {
                    u.dirs[i] = GpuDirLight {
                        dir: [dir.x, dir.y, dir.z, 0.0],
                        color: rgb,
                    };
                    u.dir_count += 1;
                }
            }
            WorldLamp::Point { pos, range } => {
                let i = u.point_count as usize;
                if i < MAX_POINT_LIGHTS {
                    u.points[i] = GpuPointLight {
                        pos_range: [pos.x, pos.y, pos.z, range],
                        color: rgb,
                    };
                    u.point_count += 1;
                }
            }
        }
    }
    u
}

fn gather(list: &DrawList, cull: bool, vp: Mat4) -> Gathered {
    let mut verts = Vec::new();
    let mut indices = Vec::<u32>::new();
    let mut batches = Vec::new();
    let mut stats = PresentStats::default();
    for q in &list.quads {
        if cull {
            let radius = q.axis_x.length() + q.axis_y.length();
            if !sphere_in_frustum(vp, q.centre, radius) {
                continue;
            }
        }
        stats.quads += 1;
        stats.verts += 4;
        stats.indices += 6;
        let first = indices.len() as u32;
        let base = verts.len() as u32;
        let c = q.colour_f();
        let n = [q.normal.x, q.normal.y, q.normal.z];
        let lit = shade_lit(q.shade);
        let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        for (p, uv) in q.corners().into_iter().zip(uvs) {
            verts.push(GpuVertex {
                pos: [p.x, p.y, p.z],
                uv,
                color: c,
                nrm: n,
                lit,
                sdf: 0.0,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        push_batch(&mut batches, q.image.clone(), first, 6, false);
    }
    for mesh in &list.meshes {
        if cull {
            let radius = mesh.verts.iter().fold(0.0_f32, |r, (p, _, _, _)| {
                r.max((*p - mesh.centre).length())
            });
            if !sphere_in_frustum(vp, mesh.centre, radius) {
                continue;
            }
        }
        stats.meshes += 1;
        stats.verts += mesh.verts.len() as u32;
        stats.indices += mesh.indices.len() as u32;
        let first = indices.len() as u32;
        let base = verts.len() as u32;
        let c = mesh.colour_f();
        let lit = shade_lit(mesh.shade);
        for (p, n, uv, rgb) in &mesh.verts {
            verts.push(GpuVertex {
                pos: [p.x, p.y, p.z],
                uv: *uv,
                color: [c[0] * rgb[0], c[1] * rgb[1], c[2] * rgb[2], c[3]],
                nrm: [n.x, n.y, n.z],
                lit,
                sdf: 0.0,
            });
        }
        for i in &mesh.indices {
            indices.push(base + u32::from(*i));
        }
        push_mesh_batches(&mut batches, mesh, first, mesh.indices.len() as u32);
    }
    for lab in &list.labels {
        if cull {
            let radius = lab.axis_x.length() + lab.axis_y.length();
            if !sphere_in_frustum(vp, lab.centre, radius) {
                continue;
            }
        }
        stats.labels += 1;
        let c = lab.colour_f();
        let n = [lab.normal.x, lab.normal.y, lab.normal.z];
        let lit = shade_lit(lab.shade);
        for g in &lab.glyphs {
            let first = indices.len() as u32;
            let base = verts.len() as u32;
            for i in 0..4 {
                let p = g.corners[i];
                verts.push(GpuVertex {
                    pos: [p.x, p.y, p.z],
                    uv: g.uv[i],
                    color: c,
                    nrm: n,
                    lit,
                    sdf: 1.0,
                });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            push_batch(&mut batches, Some(g.atlas.clone()), first, 6, true);
            stats.verts += 4;
            stats.indices += 6;
        }
    }
    Gathered {
        verts,
        indices,
        batches,
        stats,
    }
}

/// What [`Gpu::present_list`] submits for `list` (no frustum cull), as
/// triangles per texture key. Pure: no device. Compare with
/// `mq::plan(..).census()`.
pub fn census(list: &DrawList) -> SubmitCensus {
    let g = gather(list, false, list.view_proj(1.0));
    let mut c = SubmitCensus::new();
    for b in &g.batches {
        c.add(b.key.as_deref(), b.sdf, b.count);
    }
    c.quads = g.stats.quads;
    c.meshes = g.stats.meshes;
    c.labels = g.stats.labels;
    c
}

fn push_batch(batches: &mut Vec<Batch>, key: Option<String>, first: u32, count: u32, sdf: bool) {
    if count == 0 {
        return;
    }
    if let Some(last) = batches.last_mut() {
        if last.key == key && last.sdf == sdf && last.first + last.count == first {
            last.count += count;
            return;
        }
    }
    batches.push(Batch {
        key,
        first,
        count,
        sdf,
    });
}

fn push_mesh_batches(batches: &mut Vec<Batch>, mesh: &WorldMesh, first: u32, total: u32) {
    let rim = mesh.rim_index.filter(|&i| i > 0 && i < total);
    let back = mesh.back_index.filter(|&i| i > 0 && i < total);
    match (rim, back) {
        (Some(r), Some(b)) if r <= b => {
            push_batch(batches, mesh.image.clone(), first, r, false);
            push_batch(batches, None, first + r, b - r, false);
            // Omitted mb= is no back print — do not reuse m= (that path is
            // X-flipped and ghosts through alpha holes).
            if mesh.image_back.is_some() {
                push_batch(
                    batches,
                    mesh.image_back.clone(),
                    first + b,
                    total - b,
                    false,
                );
            }
        }
        (Some(r), None) => {
            push_batch(batches, mesh.image.clone(), first, r, false);
            push_batch(batches, None, first + r, total - r, false);
        }
        (None, Some(b)) if mesh.image_back.is_some() => {
            push_batch(batches, mesh.image.clone(), first, b, false);
            push_batch(
                batches,
                mesh.image_back.clone(),
                first + b,
                total - b,
                false,
            );
        }
        _ => push_batch(batches, mesh.image.clone(), first, total, false),
    }
}

fn mip_levels(w: u32, h: u32) -> u32 {
    let mut n = 1;
    let mut mw = w;
    let mut mh = h;
    while mw > 1 || mh > 1 {
        n += 1;
        mw = (mw / 2).max(1);
        mh = (mh / 2).max(1);
    }
    n
}

fn srgb_u8_to_lin(c: u8) -> f32 {
    let x = c as f32 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

fn lin_to_srgb_u8(x: f32) -> u8 {
    let x = x.clamp(0.0, 1.0);
    let y = if x <= 0.0031308 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    };
    (y * 255.0).round() as u8
}

fn downsample_rgba(w: u32, h: u32, src: &[u8]) -> (u32, u32, Vec<u8>) {
    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let mut dst = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let x0 = x * 2;
            let y0 = y * 2;
            let x1 = (x0 + 1).min(w - 1);
            let y1 = (y0 + 1).min(h - 1);
            let mut acc = [0.0f32; 4];
            for (sx, sy) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
                let i = ((sy * w + sx) * 4) as usize;
                acc[0] += srgb_u8_to_lin(src[i]);
                acc[1] += srgb_u8_to_lin(src[i + 1]);
                acc[2] += srgb_u8_to_lin(src[i + 2]);
                acc[3] += src[i + 3] as f32;
            }
            let di = ((y * nw + x) * 4) as usize;
            dst[di] = lin_to_srgb_u8(acc[0] * 0.25);
            dst[di + 1] = lin_to_srgb_u8(acc[1] * 0.25);
            dst[di + 2] = lin_to_srgb_u8(acc[2] * 0.25);
            dst[di + 3] = (acc[3] * 0.25).round().clamp(0.0, 255.0) as u8;
        }
    }
    (nw, nh, dst)
}

fn write_mip(queue: &wgpu::Queue, tex: &wgpu::Texture, mip: u32, w: u32, h: u32, pixels: &[u8]) {
    let unpadded = w * 4;
    let padded = unpadded.div_ceil(256) * 256;
    let layout = wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(padded),
        rows_per_image: Some(h),
    };
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    let dest = wgpu::TexelCopyTextureInfo {
        texture: tex,
        mip_level: mip,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
    };
    if padded == unpadded {
        queue.write_texture(dest, pixels, layout, size);
    } else {
        let mut buf = vec![0u8; padded as usize * h as usize];
        for y in 0..h as usize {
            let src = y * unpadded as usize;
            let dst = y * padded as usize;
            buf[dst..dst + unpadded as usize]
                .copy_from_slice(&pixels[src..src + unpadded as usize]);
        }
        queue.write_texture(dest, &buf, layout, size);
    }
}

fn upload_rgba_nomip(device: &wgpu::Device, queue: &wgpu::Queue, img: &RgbaImage) -> CachedTex {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sg-font"),
        size: wgpu::Extent3d {
            width: img.width,
            height: img.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    write_mip(queue, &tex, 0, img.width, img.height, &img.pixels);
    let view = tex.create_view(&Default::default());
    CachedTex {
        view,
        w: img.width,
        h: img.height,
        bytes: img.pixels.len(),
        _tex: tex,
    }
}

fn upload_rgba(device: &wgpu::Device, queue: &wgpu::Queue, img: &RgbaImage) -> CachedTex {
    let levels = mip_levels(img.width, img.height);
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sg-img"),
        size: wgpu::Extent3d {
            width: img.width,
            height: img.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut w = img.width;
    let mut h = img.height;
    let mut px = img.pixels.clone();
    write_mip(queue, &tex, 0, w, h, &px);
    for mip in 1..levels {
        let (nw, nh, next) = downsample_rgba(w, h, &px);
        write_mip(queue, &tex, mip, nw, nh, &next);
        w = nw;
        h = nh;
        px = next;
    }
    let view = tex.create_view(&Default::default());
    CachedTex {
        view,
        w: img.width,
        h: img.height,
        bytes: img.pixels.len(),
        _tex: tex,
    }
}

/// Backend program. The document selects [`Shade`]; it does not contain WGSL.
const SHADER: &str = r#"
struct DirLight {
    dir: vec4<f32>,
    color: vec4<f32>,
};
struct PointLight {
    pos_range: vec4<f32>,
    color: vec4<f32>,
};
struct Uniforms {
    view_proj: mat4x4<f32>,
    dir_count: u32,
    point_count: u32,
    _pad: vec2<u32>,
    dirs: array<DirLight, 4>,
    points: array<PointLight, 4>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var albedo: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

fn srgb_to_lin(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

fn median3(a: f32, b: f32, c: f32) -> f32 {
    return max(min(a, b), min(max(a, b), c));
}

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) nrm: vec3<f32>,
    @location(4) lit: f32,
    @location(5) sdf: f32,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) world: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) lit: f32,
    @location(5) sdf: f32,
};

@vertex
fn vs(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    o.color = vec4<f32>(srgb_to_lin(v.color.rgb), v.color.a);
    o.nrm = v.nrm;
    o.world = v.pos;
    o.uv = v.uv;
    o.lit = v.lit;
    o.sdf = v.sdf;
    return o;
}

@fragment
fn fs(v: VsOut) -> @location(0) vec4<f32> {
    if (v.sdf > 0.5) {
        let s = textureSample(albedo, samp, v.uv);
        let sd = median3(s.r, s.g, s.b) - 0.5;
        let w = max(fwidth(sd), 1e-4);
        let fill = clamp(sd / w + 0.5, 0.0, 1.0);
        if (fill < 0.01) {
            discard;
        }
        var rgb = v.color.rgb;
        if (v.lit >= 0.5) {
            let n = normalize(v.nrm);
            var acc = vec3<f32>(0.18, 0.18, 0.18);
            for (var i = 0u; i < u.dir_count; i++) {
                let dirl = u.dirs[i];
                let l = normalize(-dirl.dir.xyz);
                acc += dirl.color.rgb * max(dot(n, l), 0.0);
            }
            for (var i = 0u; i < u.point_count; i++) {
                let p = u.points[i];
                let to_l = p.pos_range.xyz - v.world;
                let dist = length(to_l);
                let ldir = to_l / max(dist, 1e-4);
                let att = clamp(1.0 - dist / max(p.pos_range.w, 1e-4), 0.0, 1.0);
                acc += p.color.rgb * max(dot(n, ldir), 0.0) * att * att;
            }
            rgb = rgb * acc;
        }
        return vec4<f32>(rgb, v.color.a * fill);
    }
    let base = textureSample(albedo, samp, v.uv) * v.color;
    if (base.a < 0.031) {
        discard;
    }
    if (v.lit < 0.5) {
        return base;
    }
    let n = normalize(v.nrm);
    var acc = vec3<f32>(0.18, 0.18, 0.18);
    for (var i = 0u; i < u.dir_count; i++) {
        let d = u.dirs[i];
        let l = normalize(-d.dir.xyz);
        acc += d.color.rgb * max(dot(n, l), 0.0);
    }
    for (var i = 0u; i < u.point_count; i++) {
        let p = u.points[i];
        let to_l = p.pos_range.xyz - v.world;
        let dist = length(to_l);
        let ldir = to_l / max(dist, 1e-4);
        let att = clamp(1.0 - dist / max(p.pos_range.w, 1e-4), 0.0, 1.0);
        acc += p.color.rgb * max(dot(n, ldir), 0.0) * att * att;
    }
    return vec4<f32>(base.rgb * acc, base.a);
}
"#;

#[cfg(feature = "egui")]
impl Gpu {
    #[allow(clippy::too_many_arguments)]
    pub fn rasterize_egui(
        &mut self,
        scene: &Scene,
        images: &Images,
        geom: &Geometry,
        materials: &Materials,
        fonts: &mut Fonts,
        width: u32,
        height: u32,
    ) -> Result<egui::ColorImage, Error> {
        let rgba = self.rasterize(scene, images, geom, materials, fonts, width, height)?;
        Ok(egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            &rgba,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_stats_dump() {
        let s = PresentStats {
            quads: 2,
            meshes: 1,
            labels: 0,
            verts: 12,
            indices: 18,
            batches: 2,
            cached: false,
        };
        assert_eq!(
            s.dump(),
            "submit\nquads=2 meshes=1 labels=0 verts=12 idx=18 batches=2 geo=rebuild\n"
        );
        let cached = PresentStats { cached: true, ..s };
        assert!(cached.dump().contains("geo=cached"), "{}", cached.dump());
    }

    #[test]
    fn room_raster_has_lit_pixels() {
        let Ok(mut gpu) = Gpu::new() else {
            eprintln!("skip: no wgpu adapter");
            return;
        };
        let scene =
            Scene::from_dump(include_str!("../../scenegraph/tests/golden/room.sg1")).unwrap();
        let mut geom = Geometry::new();
        geom.insert_tet("tet").unwrap();
        let rgba = gpu
            .rasterize(
                &scene,
                &Images::new(),
                &geom,
                &Materials::new(),
                &mut Fonts::new(),
                128,
                96,
            )
            .unwrap();
        assert_eq!(rgba.len(), 128 * 96 * 4);
        let lit = rgba
            .chunks(4)
            .filter(|p| p[0] > 40 || p[1] > 40 || p[2] > 40)
            .count();
        assert!(lit > 50, "expected crate/walls in frame, lit={lit}");
    }

    #[test]
    fn cards_sample_image_keys() {
        let Ok(mut gpu) = Gpu::new() else {
            eprintln!("skip: no wgpu adapter");
            return;
        };
        let scene =
            Scene::from_dump(include_str!("../../scenegraph/tests/golden/cards.sg1")).unwrap();
        let mut images = Images::new();
        images.insert_solid("front-a", [220, 40, 40]).unwrap();
        images.insert_solid("front-b", [40, 80, 220]).unwrap();
        let mats = Materials::from_dump(include_str!("../../scenegraph/tests/golden/surfaces.mt1"))
            .unwrap();
        let rgba = gpu
            .rasterize(
                &scene,
                &images,
                &Geometry::new(),
                &mats,
                &mut Fonts::new(),
                160,
                120,
            )
            .unwrap();
        let mut max_r = 0u8;
        let mut max_b = 0u8;
        let mut red = 0usize;
        let mut blue = 0usize;
        for p in rgba.chunks(4) {
            max_r = max_r.max(p[0]);
            max_b = max_b.max(p[2]);
            if p[0] > p[1] && p[0] > p[2] + 15 {
                red += 1;
            }
            if p[2] > p[1] && p[2] > p[0] + 15 {
                blue += 1;
            }
        }
        assert!(
            red > 10 && blue > 10,
            "front-a/b should read, red={red} blue={blue} max_r={max_r} max_b={max_b}"
        );
    }

    #[test]
    fn label_msdf_has_pixels() {
        let Ok(mut gpu) = Gpu::new() else {
            eprintln!("skip: no wgpu adapter");
            return;
        };
        let src = "sg1\nhi Label font=ibm size=0.5 \"Hi\"\ncam Camera eye=0,0,2 at=0,0,0\n";
        let scene = Scene::from_dump(src).unwrap();
        let mut fonts = Fonts::new();
        fonts
            .insert("ibm", scenegraph::NOTO_SANS_BOLD, Vec::new())
            .unwrap();
        let rgba = gpu
            .rasterize(
                &scene,
                &Images::new(),
                &Geometry::new(),
                &Materials::new(),
                &mut fonts,
                128,
                96,
            )
            .unwrap();
        let lit = rgba
            .chunks(4)
            .filter(|p| p[0] > 80 || p[1] > 80 || p[2] > 80)
            .count();
        assert!(lit > 20, "expected MSDF glyphs in frame, lit={lit}");
    }
}
