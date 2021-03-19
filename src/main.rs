use wgpu::util::DeviceExt;
use std::time::Duration;
use anyhow::*;
use imgui::Condition;
use imgui::im_str;
use glam::*;
use crate::framework::{run, Display, Game};

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
    color: glam::Vec3,
}

struct JumpAndRun {
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    camera: Camera,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
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

        let render_pipeline_layout =
            display.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&uniform_bind_group_layout],
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
                        attributes: &wgpu::vertex_attr_array![0 => Float2, 1 => Float3],
                    }
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: display.sc_desc.format,
                    alpha_blend: wgpu::BlendState::REPLACE,
                    color_blend: wgpu::BlendState::REPLACE,
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

        let vertices : Vec<Vertex> = vec![
            Vertex { position: glam::vec2(-0.08682410,  0.49240386), color: glam::vec3(0.5, 0.0, 0.5) }, // A
            Vertex { position: glam::vec2(-0.49513406,  0.06958647), color: glam::vec3(0.5, 0.2, 0.5) }, // B
            Vertex { position: glam::vec2(-0.21918549, -0.44939706), color: glam::vec3(0.5, 0.4, 0.5) }, // C
            Vertex { position: glam::vec2( 0.35966998, -0.34732910), color: glam::vec3(0.5, 0.6, 0.5) }, // D
            Vertex { position: glam::vec2( 0.44147372,  0.23473590), color: glam::vec3(0.5, 0.8, 0.5) }, // E
        ];

        let vertex_buffer = display.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsage::VERTEX,
        });

        let indices : Vec<u16> = vec![0, 1, 4, 1, 2, 4, 2, 3, 4];

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
            uniform_bind_group
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
        render_pass.set_bind_group( 0, &self.uniform_bind_group, &[]);
        render_pass.draw_indexed(0..self.num_indices, 0, 0..1);

        //Ok(())
    }
}

fn main() -> Result<()> {
    use futures::executor::block_on;

    block_on(run::<JumpAndRun>())?;

    Ok(())
}
