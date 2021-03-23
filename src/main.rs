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
use image::{GenericImageView, GenericImage};
use ogmo3::project::Tileset;
use crate::texture::TextureData;

mod framework;
mod camera;
mod buffer;
mod texture;


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

struct TileSetResult {
    texture: wgpu::Texture,
    tiles_per_row: u32,
    tiles_per_column: u32
}

impl TileSetResult {

    fn get_tile_id(&self, coords: ogmo3::Vec2<i32>) -> Option<u32> {
        if !(0i32..self.tiles_per_row as i32).contains(&coords.x) || !(0i32..self.tiles_per_column as i32).contains(&coords.y) {
            return None;
        }
        Some((coords.x + self.tiles_per_row as i32 * coords.y) as u32)
    }

    fn parse(device: &wgpu::Device, queue: &wgpu::Queue, tileset: &Tileset, base_path: &PathBuf) -> Result<Self, Error> {
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

        Ok(Self{
            texture: image_data.to_texture(device, queue, wgpu::TextureFormat::Rgba8UnormSrgb, wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST),
            tiles_per_row: expand_x,
            tiles_per_column: expand_y
        })
    }
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

        let tileset = TileSetResult::parse(&display.device, &display.queue, project.tilesets.first().unwrap(), &base_path)?;
        let tileset_texture_view = tileset.texture.create_view(&Default::default());

        let pt_data = match level.layers.first().unwrap() {
            Layer::TileCoords(layer) => {
                let mut pt_data = TextureData::<u16>::new(layer.grid_cells_x as u32, layer.grid_cells_y as u32, 1);
                for tile in layer.unpack() {
                    if let Some(coords) = tile.grid_coords {
                        *pt_data.get_pixel_mut(
                            tile.grid_position.x as u32,
                            tile.grid_position.y as u32, 0)  = (1 + tileset.get_tile_id(coords).unwrap()) as u16

                    }
                }
                pt_data
            }
            _ => panic!("layer type not supported")
        };

        let placement_texture = pt_data.to_texture(&display.device, &display.queue, wgpu::TextureFormat::R16Uint, wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST);
        let placement_texture_view = placement_texture.create_view(&wgpu::TextureViewDescriptor::default());

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
                            view_dimension: wgpu::TextureViewDimension::D2Array,
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
                        resource: wgpu::BindingResource::TextureView(&tileset_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&placement_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
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

        let vertices = vec![
            Vertex { position: glam::vec2(0.0, 0.0) * glam::vec2(pt_data.width as f32, pt_data.height as f32), tex_coords: glam::vec2(0.0, 1.0)},
            Vertex { position: glam::vec2(1.0, 0.0) * glam::vec2(pt_data.width as f32, pt_data.height as f32), tex_coords: glam::vec2(1.0, 1.0)},
            Vertex { position: glam::vec2(1.0, 1.0) * glam::vec2(pt_data.width as f32, pt_data.height as f32), tex_coords: glam::vec2(1.0, 0.0)},
            Vertex { position: glam::vec2(0.0, 1.0) * glam::vec2(pt_data.width as f32, pt_data.height as f32), tex_coords: glam::vec2(0.0, 0.0)},
        ];
        let indices : Vec<u16> = vec![0, 1, 2, 0, 2, 3];

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
