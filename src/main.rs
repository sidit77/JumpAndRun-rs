use wgpu::util::DeviceExt;
use std::time::Duration;
use std::path::PathBuf;
use anyhow::*;
use imgui::Condition;
use imgui::im_str;
use glam::*;
use crate::framework::{run, Display, Game};
use wgpu::{BlendFactor, BlendOperation};
use ogmo3::{Level, Layer, Project};

mod framework;

struct Camera {
    position: Vec2,
    aspect: f32,
    scale: f32
}

impl Default for Camera {
    fn default() -> Self {
        Self{
            position: vec2(0.0,0.0),
            aspect: 1.0,
            scale: 350.0
        }
    }
}

impl Camera {

    fn new() -> Self {
        Default::default()
    }

    fn calc_aspect(&mut self, width: u32, height: u32){
        self.aspect = width as f32 / height as f32;
    }

    fn to_matrix(&self) -> Mat4 {
        Mat4::orthographic_rh(self.position.x - (self.scale * self.aspect),
                              self.position.x + (self.scale * self.aspect),
                              self.position.y - self.scale,
                              self.position.y + self.scale, 0.0, 100.0)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: glam::Vec2,
    tex_coords: glam::Vec2,
}

struct JumpAndRun {
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    camera: Camera,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    diffuse_bind_group: wgpu::BindGroup,
}

impl Game for JumpAndRun {

    fn init(display: &Display) -> Result<Self, Error> {
        display.window.set_title("Jump and Run");

        let vs_module = display.device.create_shader_module(&include_spirv_out!("shader.vert.spv"));
        let fs_module = display.device.create_shader_module(&include_spirv_out!("shader.frag.spv"));

        let mut camera = Camera::new();
        camera.scale = 1.0;
        camera.calc_aspect(display.sc_desc.width, display.sc_desc.height);

        let uniform_buffer = display.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Uniform Buffer"),
                contents: bytemuck::cast_slice(&[camera.to_matrix()]),
                usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            }
        );

        let uniform_bind_group_layout = display.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }
            ],
            label: Some("Uniform Bind Group Layout"),
        });

        let uniform_bind_group = display.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }
            ],
            label: Some("Uniform Bind Group"),
        });

        let base_path = PathBuf::from("./assets/");
        let project = Project::from_file(base_path.join("project.ogmo"))?;
        let level = Level::from_file(base_path.join("levels/level1.json"))?;

        let (tex_scale, diffuse_image) = project.tilesets.first().map(|ts| {
            let diffuse_image = image::open(base_path.join(&ts.path)).unwrap();
            (glam::vec2(ts.tile_width as f32 / diffuse_image.width() as f32, ts.tile_height as f32 / diffuse_image.height() as f32), diffuse_image)
        }).unwrap();

        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        match level.layers.first().unwrap() {
            Layer::TileCoords(layer) => {
                for tile in layer.unpack() {
                    if let Some(coords) = tile.grid_coords {
                        let pos_coord = glam::vec2(tile.grid_position.x as f32, -tile.grid_position.y as f32);
                        let uv_coord = glam::vec2(coords.x as f32, coords.y as f32);
                        let ci = vertices.len() as u16;

                        vertices.push(Vertex { position: glam::vec2(0.0, 0.0) + pos_coord, tex_coords: (glam::vec2(0.0, 1.0) + uv_coord) * tex_scale });
                        vertices.push(Vertex { position: glam::vec2(1.0, 0.0) + pos_coord, tex_coords: (glam::vec2(1.0, 1.0) + uv_coord) * tex_scale });
                        vertices.push(Vertex { position: glam::vec2(1.0, 1.0) + pos_coord, tex_coords: (glam::vec2(1.0, 0.0) + uv_coord) * tex_scale });
                        vertices.push(Vertex { position: glam::vec2(0.0, 1.0) + pos_coord, tex_coords: (glam::vec2(0.0, 0.0) + uv_coord) * tex_scale });

                        indices.push(0 + ci);
                        indices.push(1 + ci);
                        indices.push(2 + ci);
                        indices.push(0 + ci);
                        indices.push(2 + ci);
                        indices.push(3 + ci);

                    }
                }
            }
            _ => panic!("layer type not supported")
        }

        let diffuse_rgba = diffuse_image.as_rgba8().unwrap();

        use image::GenericImageView;
        let dimensions = diffuse_image.dimensions();

        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth: 1,
        };
        let diffuse_texture = display.device.create_texture(
            &wgpu::TextureDescriptor {
                // All textures are stored as 3D, we represent our 2D texture
                // by setting depth to 1.
                size: texture_size,
                mip_level_count: 1, // We'll talk about this a little later
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                // SAMPLED tells wgpu that we want to use this texture in shaders
                // COPY_DST means that we want to copy data to this texture
                usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
                label: Some("diffuse_texture"),
            }
        );

        display.queue.write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::TextureCopyView {
                texture: &diffuse_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            // The actual pixel data
            diffuse_rgba,
            // The layout of the texture
            wgpu::TextureDataLayout {
                offset: 0,
                bytes_per_row: 4 * dimensions.0,
                rows_per_image: dimensions.1,
            },
            texture_size,
        );

        let diffuse_texture_view = diffuse_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let diffuse_sampler = display.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_bind_group_layout = display.device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            comparison: false,
                            filtering: true,
                        },
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            }
        );

        let diffuse_bind_group = display.device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                layout: &texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                    }
                ],
                label: Some("diffuse_bind_group"),
            }
        );


        let render_pipeline_layout =
            display.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &texture_bind_group_layout,
                    &uniform_bind_group_layout
                ],
                push_constant_ranges: &[],
            });

        let render_pipeline = display.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: "main",
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::InputStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float2, 1 => Float2],
                    }
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: display.sc_desc.format,
                    color_blend: wgpu::BlendState {
                        src_factor: BlendFactor::SrcAlpha,
                        dst_factor: BlendFactor::OneMinusSrcAlpha,
                        operation: BlendOperation::Add,
                    },
                    alpha_blend: wgpu::BlendState {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                    write_mask: wgpu::ColorWrite::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: wgpu::CullMode::Back,
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        });


        let vertex_buffer = display.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsage::VERTEX,
        });

        let index_buffer = display.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsage::INDEX,
        });
        let num_indices = indices.len() as u32;



        Ok(Self {
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            camera,
            uniform_buffer,
            uniform_bind_group,
            diffuse_bind_group
        })
    }

    #[allow(unused_variables)]
    fn resize(&mut self, display: &Display, width: u32, height: u32) {
        self.camera.calc_aspect(width, height);
    }

    #[allow(unused_variables)]
    fn update(&mut self, display: &Display, dt: Duration) {

    }

    #[allow(unused_variables)]
    fn render(&mut self, display: &mut Display, encoder: &mut wgpu::CommandEncoder, frame: &wgpu::TextureView, ui: Option<&imgui::Ui>) {

        if let Some(ui) = ui {
            let window = imgui::Window::new(im_str!("Hello Imgui from WGPU!"));
            window
                .size([300.0, 100.0], Condition::FirstUseEver)
                .build(&ui, || {
                    ui.text(im_str!(
                        "FPS: {:.1}",
                        1.0 / ui.io().delta_time
                    ));
                    ui.separator();
                    imgui::Drag::new(im_str!("Camera Position")).speed(0.1).build_array(&ui, self.camera.position.as_mut());
                    imgui::Drag::new(im_str!("Camera Scale")).speed(0.1).range(0.1..).build(&ui, &mut self.camera.scale);
                });
        }

        display.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[self.camera.to_matrix()]));

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: frame,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
        render_pass.set_bind_group(1, &self.uniform_bind_group, &[]);
        render_pass.draw_indexed(0..self.num_indices, 0, 0..1);

        //Ok(())
    }
}

fn main() -> Result<()> {
    use futures::executor::block_on;

    block_on(run::<JumpAndRun>())?;

    Ok(())
}
