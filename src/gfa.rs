use crate::geometry::*;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Segment {
    pub p0: Point,
    pub p1: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
struct Link {
    left: usize,
    left_rev: bool,
    right: usize,
    right_rev: bool,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Path {
    segs: Vec<Segment>,
    links: Vec<Link>,
}

fn rotate(p: Point, angle: f32) -> Point {
    let x = p.x * angle.cos() - p.y * angle.sin();
    let y = p.x * angle.sin() + p.y * angle.cos();

    Point { x, y }
}

pub fn path_vertices(segs: &[Segment]) -> Vec<Vertex> {
    let mut res = Vec::with_capacity(segs.len() * 6);

    for seg in segs {
        res.extend(seg.vertices().iter());
    }

    res
}

impl Segment {
    pub fn new(p0: Point, p1: Point) -> Self {
        Self { p0, p1 }
    }

    pub fn vertices(&self) -> [Vertex; 6] {
        let diff = self.p0 - self.p1;

        let pos0_to_pos1_norm = diff / diff.length();

        let pos0_orthogonal = rotate(pos0_to_pos1_norm, 3.14159265 / 2.0);

        let width = 25.0;

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

    // `segs` is a series of connected segments; each element is the sequence length
    pub fn from_path(offset: Point, segs: &[usize]) -> Vec<Self> {
        let mut res = Vec::new();
        let mut x = 0.0;

        let pad = 15.0;

        let calc_len = |seq_len: usize| -> f32 { 15.0 * seq_len as f32 };

        for seg in segs {
            let len = calc_len(*seg);

            let x0 = x;
            let x1 = x0 + len;

            x = x1 + pad;

            let p0 = Point { x: x0, y: 0.0 };
            let p1 = Point { x: x1, y: 0.0 };

            res.push(Self {
                p0: p0 + offset,
                p1: p1 + offset,
            });
        }

        res
    }
}
