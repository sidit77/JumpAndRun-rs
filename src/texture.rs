use wgpu::util::DeviceExt;
use wgpu::Extent3d;
use std::path::PathBuf;
use anyhow::*;
use image::GenericImageView;

#[allow(dead_code)]
pub enum MipMaps {
    None,
    Some(u32),
    All
}

pub struct TextureData<T> where T : bytemuck::Pod{
    width: u32,
    height: u32,
    depth: u32,
    mipmaps: u32,
    depth_divisor: Option<u32>,
    pixels: Box<[T]>
}

impl TextureData<[u8; 4]> {
    pub fn parse_tileset(path: &PathBuf, tile_w: u32, tile_h: u32) -> Result<TextureData<[u8; 4]>, Error>{
        let image = image::open(path)?;
        let expand_x = image.width()  / tile_w;
        let expand_y = image.height() / tile_h;

        let mut image_data = TextureData::<[u8; 4]>::new(tile_w, tile_h, expand_x * expand_y, MipMaps::All);
        image_data.depth_divisor = Some(expand_x);

        for (i, x, y) in (0..expand_y).flat_map(|y| (0..expand_x).map(move |x| (x + expand_x * y, x, y))) {
            for (px, py) in (0..tile_h).flat_map(|y| (0..tile_w).map(move |x| (x, y))) {
                *image_data.get_pixel_mut(px, py, i, 0) = image.get_pixel(x * tile_w + px,y * tile_h + py).0;
            }
        }

        image_data.generate_mipmaps();

        Ok(image_data)
    }

    pub fn generate_mipmaps(&mut self){
        for layer in 0..self.depth() {
            for mipmap in 1..self.mipmaps() {
                for y in 0..self.mipmapped_height(mipmap){
                    for x in 0..self.mipmapped_width(mipmap){
                        *self.get_pixel_mut(x,y,layer, mipmap) = Self::average(&[
                            self.get_pixel(2 * x + 0, 2 * y + 0, layer, mipmap - 1),
                            self.get_pixel(2 * x + 1, 2 * y + 0, layer, mipmap - 1),
                            self.get_pixel(2 * x + 1, 2 * y + 1, layer, mipmap - 1),
                            self.get_pixel(2 * x + 0, 2 * y + 1, layer, mipmap - 1),
                        ]);
                    }
                }
            }
        }
    }

    fn average(pixels: &[&[u8; 4]]) -> [u8; 4] {
        let mut accum = [0u32; 4];
        for pixel in pixels {
            for i in 0..accum.len() {
                accum[i] += pixel[i] as u32;
            }
        }
        let mut result = [0u8; 4];
        for i in 0..accum.len() {
            result[i] = (accum[i] / pixels.len() as u32) as u8;
        }
        result
    }

}

impl<T> TextureData<T> where T : bytemuck::Pod{

    fn max_mapmap_levels(width: u32, height: u32) -> u32 {
        1 + f32::floor(f32::log2(u32::max(width, height) as f32)) as u32
    }

    fn mipmaped_size(width: u32, level: u32) -> u32{
        (width / (1 << level)).max(1)
    }

    fn size_per_layer(width: u32, height: u32, levels: u32) -> u32{
        (0u32..levels).map(|i| Self::mipmaped_size(width, i) * Self::mipmaped_size(height, i)).sum()
    }

    pub fn new(width: u32, height: u32, depth: u32, mipmaps: MipMaps) -> Self {
        let mipmaps= match mipmaps {
            MipMaps::None => 1,
            MipMaps::Some(levels) => {debug_assert!((1u32..Self::max_mapmap_levels(width, height)).contains(&levels)); levels},
            MipMaps::All=> Self::max_mapmap_levels(width, height)
        };
        Self {
            width,
            height,
            depth,
            mipmaps,
            depth_divisor: None,
            pixels: vec![T::zeroed(); (Self::size_per_layer(width, height, mipmaps) * depth) as usize].into_boxed_slice()
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {self.height }

    pub fn mipmapped_width(&self, mipmap: u32) -> u32 {
        Self::mipmaped_size(self.width, mipmap)
    }

    pub fn mipmapped_height(&self, mipmap: u32) -> u32 {Self::mipmaped_size(self.height, mipmap) }

    pub fn mipmaps(&self) -> u32 {self.mipmaps }

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
        Self::size_per_layer(self.width(), self.height(), self.mipmaps())
    }

    #[allow(dead_code)]
    pub fn get_layer_mut(&mut self, layer: u32) -> &mut [T]{
        std::debug_assert!(layer < self.depth());
        let layer_size = self.layer_size();
        &mut self.pixels[(layer * layer_size) as usize..((layer + 1) * layer_size) as usize]
    }

    #[allow(dead_code)]
    pub fn get_layer(&self, layer: u32) -> &[T]{
        std::debug_assert!(layer < self.depth());
        let layer_size = self.layer_size();
        &self.pixels[(layer * layer_size) as usize..((layer + 1) * layer_size) as usize]
    }

    #[allow(dead_code)]
    pub fn get_mipmap_mut(&mut self, layer: u32, mipmap: u32) -> &mut [T]{
        std::debug_assert!(mipmap < self.mipmaps());
        let start = Self::size_per_layer(self.width(), self.height(), mipmap + 0) as usize;
        let end   = Self::size_per_layer(self.width(), self.height(), mipmap + 1) as usize;
        &mut self.get_layer_mut(layer)[start..end]
    }

    #[allow(dead_code)]
    pub fn get_mipmap(&self, layer: u32, mipmap: u32) -> &[T]{
        std::debug_assert!(mipmap < self.mipmaps());
        let start = Self::size_per_layer(self.width(), self.height(), mipmap + 0) as usize;
        let end   = Self::size_per_layer(self.width(), self.height(), mipmap + 1) as usize;
        &self.get_layer(layer)[start..end]
    }

    #[allow(dead_code)]
    pub fn get_pixel_mut(&mut self, x: u32, y: u32, layer:u32, mipmap: u32) -> &mut T {
        std::debug_assert!(x < self.mipmapped_width(mipmap) && y < self.mipmapped_height(mipmap));
        let index = (x + y * self.mipmapped_width(mipmap)) as usize;
        &mut self.get_mipmap_mut(layer, mipmap)[index]
    }

    #[allow(dead_code)]
    pub fn get_pixel(&self, x: u32, y: u32, layer:u32, mipmap: u32) -> &T {
        std::debug_assert!(x < self.mipmapped_width(mipmap) && y < self.mipmapped_height(mipmap));
        let index = (x + y * self.mipmapped_width(mipmap)) as usize;
        &self.get_mipmap(layer, mipmap)[index]
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
            mip_level_count: self.mipmaps(),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            label: Some("tile_set_texture"),
        }, self.as_bytes())
    }

}