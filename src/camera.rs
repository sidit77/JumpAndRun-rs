use wgpu::util::DeviceExt;
use glam::*;

pub struct Camera {
    pub position: Vec2,
    pub aspect: f32,
    pub scale: f32
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

    pub fn calc_aspect(&mut self, width: u32, height: u32){
        self.aspect = width as f32 / height as f32;
    }

    pub fn to_matrix(&self) -> Mat4 {
        Mat4::orthographic_rh(self.position.x - (self.scale * self.aspect),
                              self.position.x + (self.scale * self.aspect),
                              self.position.y - self.scale,
                              self.position.y + self.scale, 0.0, 100.0)
    }
}

pub struct CameraBuffer {
    buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl CameraBuffer {
    pub fn new(device: &wgpu::Device) -> Self{
        let buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Camera Buffer"),
                contents: bytemuck::cast_slice(&[Mat4::IDENTITY]),
                usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            }
        );

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            label: Some("Camera Bind Group Layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }
            ],
            label: Some("Camera Bind Group"),
        });

        Self{
            buffer,
            bind_group_layout,
            bind_group
        }
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn update(&self, queue: &wgpu::Queue, camera: &Camera){
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[camera.to_matrix()]));
    }

    pub fn bind<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, index: u32) {
        pass.set_bind_group(index, &self.bind_group, &[]);
    }
}

pub trait UpdateCameraBuffer {
    fn update_camera_buffer(&self, buffer: &CameraBuffer, camera: &Camera);
}

impl UpdateCameraBuffer for wgpu::Queue {
    fn update_camera_buffer(&self, buffer: & CameraBuffer, camera: & Camera) {
        self.write_buffer(&buffer.buffer, 0, bytemuck::cast_slice(&[camera.to_matrix()]));
    }
}

pub trait BindCameraBuffer<'a, 'b> where 'b: 'a {
    fn set_camera_buffer(&mut self, index: u32, buffer: &'b CameraBuffer);
}

impl<'a, 'b> BindCameraBuffer<'a, 'b> for wgpu::RenderPass<'a>  where 'b: 'a {
    fn set_camera_buffer(&mut self, index: u32, buffer: &'b CameraBuffer) {
        self.set_bind_group(index, &buffer.bind_group, &[]);
    }
}