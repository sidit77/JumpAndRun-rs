use wgpu::util::DeviceExt;
use wgpu::Extent3d;
use std::path::PathBuf;
use anyhow::*;
use image::GenericImageView;

pub struct TextureData<T> where T : bytemuck::Pod{
    width: u32,
    height: u32,
    depth: u32,
    depth_divisor: Option<u32>,
    pixels: Box<[T]>
}

impl TextureData<[u8; 4]> {
    pub fn parse_tileset(path: &PathBuf, tile_w: u32, tile_h: u32) -> Result<TextureData<[u8; 4]>, Error>{
        let image = image::open(path)?;
        let expand_x = image.width()  / tile_w;
        let expand_y = image.height() / tile_h;

        let mut image_data = TextureData::<[u8; 4]>::new(tile_w, tile_h, expand_x * expand_y);
        image_data.depth_divisor = Some(expand_x);

        for (i, x, y) in (0..expand_y).flat_map(|y| (0..expand_x).map(move |x| (x + expand_x * y, x, y))) {
            for (px, py) in (0..tile_h).flat_map(|y| (0..tile_w).map(move |x| (x, y))) {
                *image_data.get_pixel_mut(px, py, i) = image.get_pixel(x * tile_w + px,y * tile_h + py).0;
            }
        }

        Ok(image_data)
    }
}

impl<T> TextureData<T> where T : bytemuck::Pod{

    pub fn new(width: u32, height: u32, depth: u32) -> Self {
        Self {
            width,
            height,
            depth,
            depth_divisor: None,
            pixels: vec![T::zeroed(); (width * height * depth) as usize].into_boxed_slice()
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    #[allow(dead_code)]
    pub fn depth_x(&self) -> Option<u32> {
        self.depth_divisor
    }

    #[allow(dead_code)]
    pub fn depth_y(&self) -> Option<u32> {
        self.depth_divisor.map(|dd|(self.depth / dd))
    }

    fn layer_size(&self) -> u32 {
        self.width() * self.height()
    }

    #[allow(dead_code)]
    pub fn get_layer(&self, layer: u32) -> &[T]{
        std::debug_assert!(layer < self.depth());
        &self.pixels[(layer * self.layer_size()) as usize..((layer + 1) * self.layer_size()) as usize]
    }

    #[allow(dead_code)]
    pub fn get_layer_mut(&mut self, layer: u32) -> &mut [T]{
        std::debug_assert!(layer < self.depth());
        let layer_size = self.layer_size();
        &mut self.pixels[(layer * layer_size) as usize..((layer + 1) * layer_size) as usize]
    }

    #[allow(dead_code)]
    pub fn get_pixel(&self, x: u32, y: u32, layer:u32) -> &T {
        std::debug_assert!(x < self.width() && y < self.height() && layer < self.depth());
        let index = (x + y * self.width()) as usize;
        &self.get_layer(layer)[index]
    }

    #[allow(dead_code)]
    pub fn get_pixel_mut(&mut self, x: u32, y: u32, layer:u32) -> &mut T {
        std::debug_assert!(x < self.width() && y < self.height() && layer < self.depth());
        let index = (x + y * self.width()) as usize;
        &mut self.get_layer_mut(layer)[index]
    }

    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.pixels)
    }

    pub fn to_texture(&self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, usage: wgpu::TextureUsage) -> wgpu::Texture {
        device.create_texture_with_data(queue, &wgpu::TextureDescriptor {
            size: Extent3d {
                width: self.width(),
                height: self.height(),
                depth: self.depth()
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