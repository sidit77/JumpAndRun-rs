use winit::window::{WindowBuilder, Window};
use winit::event::*;
use winit::event_loop::{ControlFlow, EventLoop};
use wgpu::util::DeviceExt;
use std::time::{Duration, Instant};
use anyhow::*;
use imgui_wgpu::{Renderer, RendererConfig};
use imgui::{FontSource, Condition};
use imgui::im_str;

pub struct Display {
    pub window: Window,
    surface: wgpu::Surface,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub sc_desc: wgpu::SwapChainDescriptor,
    pub swap_chain: wgpu::SwapChain,
}

impl Display {
    async fn new(window: Window) -> Result<Self, Error> {

        let size = window.inner_size();

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
        let surface = unsafe { instance.create_surface(&window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                },
                None, // Trace path
            )
            .await
            .unwrap();

        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            format: adapter.get_swap_chain_preferred_format(&surface),
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };
        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        Ok(Self {
            window,
            surface,
            device,
            queue,
            sc_desc,
            swap_chain,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.sc_desc.width = width;
        self.sc_desc.height = height;
        self.swap_chain = self.device.create_swap_chain(&self.surface, &self.sc_desc);
    }
}

#[macro_export]
macro_rules! include_spirv_out {
    ($file:expr) => {
        {
            wgpu::include_spirv!(concat!(env!("OUT_DIR"), "/", file!() , "/../", $file))
        }
    };
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: glam::Vec3,
    color: glam::Vec3,
}

struct JumpAndRun {
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    imgui: imgui::Context,
    platform: imgui_winit_support::WinitPlatform,
    renderer: Renderer
}

impl Game for JumpAndRun {

    fn init(display: &Display) -> Result<Self, Error> {
        let vs_module = display.device.create_shader_module(&include_spirv_out!("shader.vert.spv"));
        let fs_module = display.device.create_shader_module(&include_spirv_out!("shader.frag.spv"));

        let render_pipeline_layout =
            display.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[],
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
                        attributes: &wgpu::vertex_attr_array![0 => Float3, 1 => Float3],
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
            Vertex { position: glam::vec3(-0.08682410,  0.49240386, 0.0), color: glam::vec3(0.5, 0.0, 0.5) }, // A
            Vertex { position: glam::vec3(-0.49513406,  0.06958647, 0.0), color: glam::vec3(0.5, 0.0, 0.5) }, // B
            Vertex { position: glam::vec3(-0.21918549, -0.44939706, 0.0), color: glam::vec3(0.5, 0.0, 0.5) }, // C
            Vertex { position: glam::vec3( 0.35966998, -0.34732910, 0.0), color: glam::vec3(0.5, 0.0, 0.5) }, // D
            Vertex { position: glam::vec3( 0.44147372,  0.23473590, 0.0), color: glam::vec3(0.5, 0.0, 0.5) }, // E
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


        let mut imgui = imgui::Context::create();
        let mut platform = imgui_winit_support::WinitPlatform::init(&mut imgui);
        platform.attach_window(
            imgui.io_mut(),
            &display.window,
            imgui_winit_support::HiDpiMode::Default,
        );
        imgui.set_ini_filename(None);

        let hidpi_factor = display.window.scale_factor();
        let font_size = (13.0 * hidpi_factor) as f32;
        imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;
        imgui.fonts().add_font(&[FontSource::DefaultFontData {
            config: Some(imgui::FontConfig {
                oversample_h: 1,
                pixel_snap_h: true,
                size_pixels: font_size,
                ..Default::default()
            }),
        }]);

        let renderer_config = RendererConfig {
            texture_format: display.sc_desc.format,
            ..Default::default()
        };
        let renderer = Renderer::new(&mut imgui, &display.device, &display.queue, renderer_config);

        Ok(Self {
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            imgui,
            platform,
            renderer
        })
    }

    #[allow(unused_variables)]
    fn resize(&mut self, display: &Display) {
        //unimplemented!()
    }

    #[allow(unused_variables)]
    fn update(&mut self, display: &Display, dt: Duration) {
        self.imgui.io_mut().update_delta_time(dt);
    }

    fn render(&mut self, display: &mut Display) {
        self.platform
            .prepare_frame(self.imgui.io_mut(), &display.window)
            .expect("Failed to prepare frame!");
        let ui = self.imgui.frame();
        {
            let window = imgui::Window::new(im_str!("Hello Imgui from WGPU!"));
            window
                .size([300.0, 100.0], Condition::FirstUseEver)
                .build(&ui, || {
                    ui.text(im_str!("Hello world!"));
                    ui.text(im_str!("This is a demo of imgui-rs using imgui-wgpu!"));
                    ui.separator();
                    let mouse_pos = ui.io().mouse_pos;
                    ui.text(im_str!(
                        "Mouse Position: ({:.1}, {:.1})",
                        mouse_pos[0],
                        mouse_pos[1],
                    ));
                    ui.text(im_str!(
                        "Time: ({})",
                        Instant::now().elapsed().as_secs()
                    ));
                });
        }
        //let frame = display.swap_chain.get_current_frame()?.output;
        let frame = display.swap_chain.get_current_frame().unwrap().output;

        let mut encoder = display
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &frame.view,
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
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }
        {
            self.platform.prepare_render(&ui, &display.window);

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UI RenderPass"),
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &frame.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });
            self.renderer
                .render(ui.render(), &display.queue, &display.device, &mut pass)
                .expect("Failed to render UI!");
        }



        display.queue.submit(Some(encoder.finish()));

        //Ok(())
    }

    fn handle_event(&mut self, display: &Display, event: &Event<()>) {
        self.platform.handle_event(self.imgui.io_mut(), &display.window, event);
    }
}

pub trait Game: 'static + Sized {
    fn init(display: &Display) -> Result<Self, Error>;
    fn resize(&mut self, display: &Display);
    fn update(&mut self, display: &Display, dt: Duration);
    fn render(&mut self, display: &mut Display);
    #[allow(unused_variables)]
    fn handle_event(&mut self, display: &Display, event: &Event<()>) {}
}

pub async fn run<G: Game>() -> Result<(), Error> {
    //wgpu_subscriber::initialize_default_subscriber(None);
    env_logger::init();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(env!("CARGO_PKG_NAME"))
        .build(&event_loop)?;
    let mut display = Display::new(window).await?;
    let mut game = G::init(&mut display)?;
    let mut last_update = Instant::now();
    let mut is_resumed = true;
    let mut is_focused = true;
    let mut is_redraw_requested = true;

    event_loop.run(move |event, _, control_flow| {
        game.handle_event(&mut display, &event);
        *control_flow = if is_resumed && is_focused {
            ControlFlow::Poll
        } else {
            ControlFlow::Wait
        };

        match event {
            Event::Resumed => is_resumed = true,
            Event::Suspended => is_resumed = false,
            Event::RedrawRequested(wid) => {
                if wid == display.window.id() {
                    let now = Instant::now();
                    let dt = now - last_update;
                    last_update = now;

                    game.update(&mut display, dt);
                    game.render(&mut display);
                    is_redraw_requested = false;
                }
            }
            Event::MainEventsCleared => {
                if is_focused && is_resumed && !is_redraw_requested {
                    display.window.request_redraw();
                    is_redraw_requested = true;
                } else {
                    // Freeze time while the demo is not in the foreground
                    last_update = Instant::now();
                }
            }
            Event::WindowEvent {
                event, window_id, ..
            } => {
                if window_id == display.window.id() {
                    match event {
                        WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                        WindowEvent::Focused(f) => is_focused = f,
                        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                            display.resize(new_inner_size.width, new_inner_size.height);
                            game.resize(&mut display);
                        }
                        WindowEvent::Resized(new_inner_size) => {
                            display.resize(new_inner_size.width, new_inner_size.height);
                            game.resize(&mut display);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

    });
}

fn main() -> Result<()> {
    /*
    env_logger::init();
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    use futures::executor::block_on;

    // Since main can't be async, we're going to need to block
    let mut display = block_on(Display::new(window));
    let mut state = block_on(State::new(&display.device, display.sc_desc.format));

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == window.id() => {
                if !state.input(event) {
                    match event {
                        WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                        WindowEvent::KeyboardInput { input, .. } => match input {
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            } => *control_flow = ControlFlow::Exit,
                            _ => {}
                        },
                        WindowEvent::Resized(physical_size) => {
                            display.resize(*physical_size);
                        }
                        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                            // new_inner_size is &mut so w have to dereference it twice
                            display.resize(**new_inner_size);
                        }
                        _ => {}
                    }
                }
            }
            Event::RedrawRequested(_) => {
                state.update();
                match state.render(&display) {
                    Ok(_) => {}
                    // Recreate the swap_chain if lost
                    Err(wgpu::SwapChainError::Lost) => display.resize(display.size),
                    // The system is out of memory, we should probably quit
                    Err(wgpu::SwapChainError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                    // All other errors (Outdated, Timeout) should be resolved by the next frame
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            Event::MainEventsCleared => {
                // RedrawRequested will only trigger once, unless we manually
                // request it.
                window.request_redraw();
            }
            _ => {}
        }
    });
    */
    use futures::executor::block_on;

    block_on(run::<JumpAndRun>())?;

    Ok(())
}
