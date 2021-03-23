use wgpu::util::DeviceExt;
use wgpu::Extent3d;

pub struct TextureData<T> where T : bytemuck::Pod{
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pixels: Box<[T]>
}

impl<T> TextureData<T> where T : bytemuck::Pod{

    pub fn new(width: u32, height: u32, depth: u32) -> Self {
        Self {
            width,
            height,
            depth,
            pixels: vec![T::zeroed(); (width * height * depth) as usize].into_boxed_slice()
        }
    }

    #[allow(dead_code)]
    pub fn get_layer(&self, layer: u32) -> &[T]{
        std::assert!(layer < self.depth);
        &self.pixels[(layer * (self.width * self.height)) as usize..((layer + 1) * (self.width * self.height)) as usize]
    }

    #[allow(dead_code)]
    pub fn get_layer_mut(&mut self, layer: u32) -> &mut [T]{
        std::assert!(layer < self.depth);
        &mut self.pixels[(layer * (self.width * self.height)) as usize..((layer + 1) * (self.width * self.height)) as usize]
    }

    #[allow(dead_code)]
    pub fn get_pixel(&self, x: u32, y: u32, layer:u32) -> &T {
        std::assert!(x < self.width && y < self.height && layer < self.depth);
        let index = (x + y * self.width) as usize;
        &self.get_layer(layer)[index]
    }

    #[allow(dead_code)]
    pub fn get_pixel_mut(&mut self, x: u32, y: u32, layer:u32) -> &mut T {
        std::assert!(x < self.width && y < self.height && layer < self.depth);
        let index = (x + y * self.width) as usize;
        &mut self.get_layer_mut(layer)[index]
    }

    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.pixels)
    }

    pub fn to_texture(&self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, usage: wgpu::TextureUsage) -> wgpu::Texture {
        device.create_texture_with_data(queue, &wgpu::TextureDescriptor {
            size: Extent3d {
                width: self.width,
                height: self.height,
                depth: self.depth
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            label: Some("tile_set_texture"),
        }, self.as_bytes())
    }

}