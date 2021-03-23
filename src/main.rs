use wgpu::util::DeviceExt;
use std::time::Duration;
use std::path::PathBuf;
use anyhow::*;
use imgui::Condition;
use imgui::im_str;
use glam::*;
use crate::framework::{run, Display, Game};
use wgpu::{BlendFactor, BlendOperation, Extent3d};
use ogmo3::{Level, Layer, Project};
use crate::camera::Camera;
use crate::buffer::{UniformBuffer, UpdateUniformBuffer, BindUniformBuffer};
use image::{EncodableLayout, GenericImageView};
use ogmo3::project::Tileset;

mod framework;
mod camera;
mod buffer;


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
    camera_buffer: UniformBuffer<Mat4>,
    diffuse_bind_group: wgpu::BindGroup,
}

struct TextureData<T> where T : bytemuck::Pod{
    width: u32,
    height: u32,
    depth: u32,
    pixels: Box<[T]>
}

impl<T> TextureData<T> where T : bytemuck::Pod{

    fn new(width: u32, height: u32, depth: u32) -> Self {
        Self {
            width,
            height,
            depth,
            pixels: vec![T::zeroed(); (width * height * depth) as usize].into_boxed_slice()
        }
    }

    #[allow(dead_code)]
    fn get_layer(&self, layer: u32) -> &[T]{
        std::assert!(layer < self.depth);
        &self.pixels[(layer * (self.width * self.height)) as usize..((layer + 1) * (self.width * self.height)) as usize]
    }

    #[allow(dead_code)]
    fn get_layer_mut(&mut self, layer: u32) -> &mut [T]{
        std::assert!(layer < self.depth);
        &mut self.pixels[(layer * (self.width * self.height)) as usize..((layer + 1) * (self.width * self.height)) as usize]
    }

    #[allow(dead_code)]
    fn get_pixel(&self, x: u32, y: u32, layer:u32) -> &T {
        std::assert!(x < self.width && y < self.height && layer < self.depth);
        let index = (x + y * self.height) as usize;
        &self.get_layer(layer)[index]
    }

    #[allow(dead_code)]
    fn get_pixel_mut(&mut self, x: u32, y: u32, layer:u32) -> &mut T {
        std::assert!(x < self.width && y < self.height && layer < self.depth);
        let index = (x + y * self.height) as usize;
        &mut self.get_layer_mut(layer)[index]
    }

    #[allow(dead_code)]
    fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.pixels)
    }

}

fn load_texture(device: &wgpu::Device, queue: &wgpu::Queue, tileset: &Tileset, base_path: PathBuf) -> Result<wgpu::Texture, Error> {
    let image = image::open(base_path.join(&tileset.path))?;
    let tile_w = tileset.tile_width  as u32;
    let tile_h = tileset.tile_height as u32;
    let expand_x = image.width()  / tile_w;
    let expand_y = image.height() / tile_h;

    let mut image_data = TextureData::<[u8; 4]>::new(tile_w, tile_h, expand_x * expand_y);

    for (i, x, y) in (0..expand_y).flat_map(|y| (0..expand_x).map(move |x| (x + expand_x * y, x, y))) {
        for (px, py) in (0..tile_h).flat_map(|y| (0..tile_w).map(move |x| (x, y))) {
            *image_data.get_pixel_mut(px, py, i) = image.get_pixel(x * tile_w + px,y * tile_h + py).0;
        }
    }

    Ok(device.create_texture_with_data(queue,
        &wgpu::TextureDescriptor {
            // All textures are stored as 3D, we represent our 2D texture
            // by setting depth to 1.
            size: Extent3d {
                width: tile_w,
                height: tile_h,
                depth: expand_x * expand_y
            },
            mip_level_count: 1, // We'll talk about this a little later
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            // SAMPLED tells wgpu that we want to use this texture in shaders
            // COPY_DST means that we want to copy data to this texture
            usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
            label: Some("array_texture"),
        }, image_data.as_bytes()
    ))
}

impl Game for JumpAndRun {

    fn init(display: &Display) -> Result<Self, Error> {
        display.window.set_title("Jump and Run");

        let vs_module = display.device.create_shader_module(&include_spirv_out!("shader.vert.spv"));
        let fs_module = display.device.create_shader_module(&include_spirv_out!("shader.frag.spv"));

        let camera = Camera {
            scale: 13.0,
            aspect: display.sc_desc.width as f32 / display.sc_desc.height as f32,
            position: glam::vec2(16.0, -12.0)
        };

        let camera_buffer = UniformBuffer::<Mat4>::new(&display.device);

        let base_path = PathBuf::from("./assets/");
        let project = Project::from_file(base_path.join("project.ogmo"))?;
        let level = Level::from_file(base_path.join("levels/level1.json"))?;

        let (tex_scale, diffuse_image) = project.tilesets.first().map(|ts| {
            let diffuse_image = image::open(base_path.join(&ts.path)).unwrap();
            (glam::vec2(ts.tile_width as f32 / diffuse_image.width() as f32, ts.tile_height as f32 / diffuse_image.height() as f32), diffuse_image)
        }).unwrap();

        let test = load_texture(&display.device, &display.queue, project.tilesets.first().unwrap(), base_path)?.create_view(&Default::default());

        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let (pi, pi_width, pi_height) = match level.layers.first().unwrap() {
            Layer::TileCoords(layer) => {
                let x_tile = (1.0 / tex_scale.x).round() as i32;
                let pi_width = layer.grid_cells_x;
                let pi_height = layer.grid_cells_y;
                let mut pi = vec![0u16; (pi_width * pi_height) as usize].into_boxed_slice();
                for tile in layer.unpack() {
                    if let Some(coords) = tile.grid_coords {

                        pi[(tile.grid_position.x + tile.grid_position.y * pi_width) as usize] = (1 + coords.x + x_tile * coords.y) as u16;

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
                (pi, pi_width, pi_height)
            }
            _ => panic!("layer type not supported")
        };

        let diffuse_rgba = diffuse_image.as_rgba8().unwrap();

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

        let placement_texture = display.device.create_texture_with_data(&display.queue, &wgpu::TextureDescriptor {
            // All textures are stored as 3D, we represent our 2D texture
            // by setting depth to 1.
            size: Extent3d {
                width: pi_width as u32,
                height: pi_height as u32,
                depth: 1,
            },
            mip_level_count: 1, // We'll talk about this a little later
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Uint,
            // SAMPLED tells wgpu that we want to use this texture in shaders
            // COPY_DST means that we want to copy data to this texture
            usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
            label: Some("placement_texture"),
        }, pi.as_bytes());

        let placement_texture_view = placement_texture.create_view(&wgpu::TextureViewDescriptor::default());

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
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Uint,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            comparison: false,
                            filtering: true,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
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
                        resource: wgpu::BindingResource::TextureView(&placement_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&test),
                    },
                ],
                label: Some("diffuse_bind_group"),
            }
        );


        let render_pipeline_layout =
            display.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &texture_bind_group_layout,
                    camera_buffer.layout()
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

        vertices.clear();
        vertices.push(Vertex { position: glam::vec2(0.0, 0.0) * glam::vec2(pi_width as f32, pi_height as f32), tex_coords: glam::vec2(0.0, 1.0)});
        vertices.push(Vertex { position: glam::vec2(1.0, 0.0) * glam::vec2(pi_width as f32, pi_height as f32), tex_coords: glam::vec2(1.0, 1.0)});
        vertices.push(Vertex { position: glam::vec2(1.0, 1.0) * glam::vec2(pi_width as f32, pi_height as f32), tex_coords: glam::vec2(1.0, 0.0)});
        vertices.push(Vertex { position: glam::vec2(0.0, 1.0) * glam::vec2(pi_width as f32, pi_height as f32), tex_coords: glam::vec2(0.0, 0.0)});

        indices.clear();
        indices.push(0);
        indices.push(1);
        indices.push(2);
        indices.push(0);
        indices.push(2);
        indices.push(3);

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
            camera_buffer,
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

        display.queue.update_uniform_buffer(&self.camera_buffer, &self.camera.to_matrix());

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
        render_pass.set_uniform_buffer(1, &self.camera_buffer);
        render_pass.draw_indexed(0..self.num_indices, 0, 0..1);

        //Ok(())
    }
}

fn main() -> Result<()> {
    use futures::executor::block_on;

    block_on(run::<JumpAndRun>())?;

    Ok(())
}
