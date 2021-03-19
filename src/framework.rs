use winit::window::{WindowBuilder, Window};
use winit::event::*;
use winit::event_loop::{ControlFlow, EventLoop};
use std::time::{Duration, Instant};
use anyhow::*;
use imgui_wgpu::{Renderer, RendererConfig};
use imgui::FontSource;
use wgpu::RenderPass;

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

pub trait Game: 'static + Sized {
    fn init(display: &Display) -> Result<Self, Error>;
    fn resize(&mut self, display: &Display, width: u32, height: u32);
    fn update(&mut self, display: &Display, dt: Duration);
    fn render(&mut self, display: &mut Display, encoder: &mut wgpu::CommandEncoder, frame: &wgpu::TextureView, ui: Option<&imgui::Ui>);
}

pub async fn run<G: Game>() -> Result<(), Error> {
    //wgpu_subscriber::initialize_default_subscriber(None);
    env_logger::init();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(env!("CARGO_PKG_NAME"))
        .build(&event_loop)?;
    let mut display = Display::new(window).await?;
    let mut game = G::init(&display)?;
    let mut imgui : Option<ImguiWrapper> = Some(ImguiWrapper::new(&display)?);

    let mut last_update = Instant::now();
    let mut is_resumed = true;
    let mut is_focused = true;
    let mut is_redraw_requested = true;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = if is_resumed && is_focused {
            ControlFlow::Poll
        } else {
            ControlFlow::Wait
        };

        if let Some(imgui) = imgui.as_mut() {
            imgui.handle_events(&display.window, &event);
        }

        match event {
            Event::Resumed => is_resumed = true,
            Event::Suspended => is_resumed = false,
            Event::RedrawRequested(wid) => {
                if wid == display.window.id() {
                    let now = Instant::now();
                    let dt = now - last_update;
                    last_update = now;

                    game.update(&display, dt);


                    if let Some(imgui) = imgui.as_mut() {
                        imgui.prepare(&display.window);
                        imgui.update_delta_time(dt);
                    }

                    let frame = display.swap_chain.get_current_frame().unwrap().output;

                    let mut encoder = display
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Render Encoder"),
                        });

                    match imgui.as_mut() {
                        Some(imgui) => {
                            let ui = imgui.imgui.frame();
                            game.render(&mut display, &mut encoder, &frame.view, Some(&ui));

                            imgui.platform.prepare_render(&ui, &display.window);

                            imgui.renderer
                                .render(ui.render(), &display.queue, &display.device, &mut ImguiWrapper::render_pass(&mut encoder, &frame.view))
                                .expect("Failed to render UI!");
                        }
                        None => game.render(&mut display, &mut encoder, &frame.view, None)
                    }

                    display.queue.submit(Some(encoder.finish()));

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
                            game.resize(&mut display, new_inner_size.width, new_inner_size.height);
                        }
                        WindowEvent::Resized(new_inner_size) => {
                            display.resize(new_inner_size.width, new_inner_size.height);
                            game.resize(&mut display, new_inner_size.width, new_inner_size.height);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

    });
}

struct ImguiWrapper {
    imgui: imgui::Context,
    platform: imgui_winit_support::WinitPlatform,
    renderer: Renderer,
}

impl ImguiWrapper {
    fn new(display: &Display) -> Result<Self, Error> {

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
            imgui,
            platform,
            renderer,
        })
    }

    fn handle_events(&mut self, window: &Window, event: &Event<()>){
        self.platform.handle_event(self.imgui.io_mut(), window, event);
    }

    fn prepare(&mut self, window: &Window){
        self.platform
            .prepare_frame(self.imgui.io_mut(), window)
            .expect("Failed to prepare frame!");
    }

    fn update_delta_time(&mut self, dt: Duration){
        self.imgui.io_mut().update_delta_time(dt);
    }

    fn render_pass<'a>(encoder: &'a mut wgpu::CommandEncoder, frame: &'a wgpu::TextureView) -> RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("UI RenderPass"),
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: frame,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        })
    }

}