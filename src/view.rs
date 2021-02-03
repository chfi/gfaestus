use crate::geometry::Point;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct View {
    pub center: Point,
    pub scale: f32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            center: Point::new(0.0, 0.0),
            scale: 1.0,
        }
    }
}

// pub struct
