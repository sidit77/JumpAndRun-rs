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
