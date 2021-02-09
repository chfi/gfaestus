use crate::geometry::*;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Segment {
    pub p0: Point,
    pub p1: Point,
}

fn rotate(p: Point, angle: f32) -> Point {
    let x = p.x * angle.cos() - p.y * angle.sin();
    let y = p.x * angle.sin() + p.y * angle.cos();

    Point { x, y }
}

impl Segment {
    pub fn new(p0: Point, p1: Point) -> Self {
        Self { p0, p1 }
    }

    pub fn vertices(&self) -> [Vertex; 6] {
        let diff = self.p0 - self.p1;

        let pos0_to_pos1_norm = diff / diff.length();

        let pos0_orthogonal = rotate(pos0_to_pos1_norm, 3.14159265 / 2.0);

        let width = 10.0;

        let p0 = self.p0 + pos0_orthogonal * (width / 2.0);
        let p1 = self.p0 + pos0_orthogonal * (-width / 2.0);

        let p2 = self.p1 + pos0_orthogonal * (width / 2.0);
        let p3 = self.p1 + pos0_orthogonal * (-width / 2.0);

        [
            p0.vertex(true),
            p2.vertex(true),
            p1.vertex(true),
            p3.vertex(true),
            p2.vertex(true),
            p1.vertex(true),
        ]
    }
}
