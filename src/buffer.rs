pub struct UniformBuffer<T> where T: bytemuck::Pod{
    buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    _marker: std::marker::PhantomData<T>
}

impl<T> UniformBuffer<T>  where T: bytemuck::Pod {
    pub fn new(device: &wgpu::Device) -> Self{

        let buffer= device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: std::mem::size_of::<T>() as u64,
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false
        });

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
            label: Some("Uniform Bind Group Layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }
            ],
            label: Some("Uniform Bind Group"),
        });

        Self{
            buffer,
            bind_group_layout,
            bind_group,
            _marker: std::marker::PhantomData {}
        }
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }
}

pub trait UpdateUniformBuffer<T>  where T: bytemuck::Pod{
    fn update_uniform_buffer(&self, buffer: &UniformBuffer<T>, value: &T);
}

impl<T> UpdateUniformBuffer<T> for wgpu::Queue where T: bytemuck::Pod{
    fn update_uniform_buffer(&self, buffer: &UniformBuffer<T>, value: &T) {
        self.write_buffer(&buffer.buffer, 0, bytemuck::bytes_of(value));
    }
}

pub trait BindUniformBuffer<'a, 'b, T> where 'b: 'a, T: bytemuck::Pod {
    fn set_uniform_buffer(&mut self, index: u32, buffer: &'b UniformBuffer<T>);
}

impl<'a, 'b, T> BindUniformBuffer<'a, 'b, T> for wgpu::RenderPass<'a>  where 'b: 'a, T: bytemuck::Pod {
    fn set_uniform_buffer(&mut self, index: u32, buffer: &'b UniformBuffer<T>) {
        self.set_bind_group(index, &buffer.bind_group, &[]);
    }
}