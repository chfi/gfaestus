use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

#[derive(Default, Debug, Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 2],
}
vulkano::impl_vertex!(Vertex, position);

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    #[inline]
    pub fn new<T: Into<f32>>(x: T, y: T) -> Self {
        let x = x.into();
        let y = y.into();
        Self { x, y }
    }

    #[inline]
    pub fn dist(&self, other: Point) -> f32 {
        let x_diff = (self.x - other.x).abs();
        let y_diff = (self.y - other.y).abs();
        x_diff.hypot(y_diff)
    }

    #[inline]
    pub fn dist_sqr(&self, other: Point) -> f32 {
        let x_diff = (self.x - other.x).abs();
        let y_diff = (self.y - other.y).abs();
        x_diff.powi(2) + y_diff.powi(2)
    }

    #[inline]
    pub fn vertex(&self) -> Vertex {
        Vertex {
            position: [self.x, self.y],
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Node {
    pub p0: Point,
    pub p1: Point,
}

impl Node {
    #[inline]
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        let p0 = Point::new(x0, y0);
        let p1 = Point::new(x1, y1);
        Self { p0, p1 }
    }

    #[inline]
    pub fn vertices(&self) -> [Vertex; 2] {
        [self.p0.vertex(), self.p1.vertex()]
    }
}

impl From<(f32, f32)> for Point {
    #[inline]
    fn from((x, y): (f32, f32)) -> Point {
        Point { x, y }
    }
}

impl From<(f64, f64)> for Point {
    #[inline]
    fn from((x, y): (f64, f64)) -> Point {
        let x = x as f32;
        let y = y as f32;
        Point { x, y }
    }
}

impl From<(i32, i32)> for Point {
    #[inline]
    fn from((x, y): (i32, i32)) -> Point {
        let x = x as f32;
        let y = y as f32;
        Point { x, y }
    }
}

macro_rules! impl_assign_binop {
    ($trait:ident, Rhs = $rhs:ty, $opfn:ident, $opassfn:ident) => {
        impl $trait<$rhs> for Point {
            #[inline]
            fn $opassfn(&mut self, other: $rhs) {
                *self = self.$opfn(other);
            }
        }
    };
    ($trait:ident, $opfn:ident, $opassfn:ident) => {
        impl_assign_binop!($trait, Rhs = Point, $opfn, $opassfn);
    };
}

macro_rules! impl_ref_binop {
    ($trait:ident, $rhs:ty, $opfn:ident) => {
        impl $trait<$rhs> for Point {
            type Output = Self;
            #[inline]
            fn $opfn(self, other: $rhs) -> Self {
                self.$opfn(*other)
            }
        }
    };
}

macro_rules! impl_ref_assign_binop {
    ($trait:ident, $rhs:ty, $opfn:ident) => {
        impl $trait<$rhs> for Point {
            #[inline]
            fn $opfn(&mut self, other: $rhs) {
                self.$opfn(*other)
            }
        }
    };
}

macro_rules! impl_point_ops {
    ($trait:ident, $traitass:ident, $opfn:ident, $opassfn:ident) => {
        impl $trait for Point {
            type Output = Self;

            #[inline]
            fn $opfn(self, other: Self) -> Self {
                Self {
                    x: f32::$opfn(self.x, other.x),
                    y: f32::$opfn(self.y, other.y),
                }
            }
        }

        impl_assign_binop!($traitass, $opfn, $opassfn);
        impl_ref_binop!($trait, &Point, $opfn);
        impl_ref_assign_binop!($traitass, &Point, $opassfn);
    };
}

impl_point_ops!(Add, AddAssign, add, add_assign);
impl_point_ops!(Sub, SubAssign, sub, sub_assign);

impl Mul<f32> for Point {
    type Output = Self;

    #[inline]
    fn mul(self, other: f32) -> Self {
        Self {
            x: self.x * other,
            y: self.y * other,
        }
    }
}

impl_assign_binop!(MulAssign, Rhs = f32, mul, mul_assign);
impl_ref_binop!(Mul, &f32, mul);
impl_ref_assign_binop!(MulAssign, &f32, mul_assign);

impl Div<f32> for Point {
    type Output = Self;

    #[inline]
    fn div(self, other: f32) -> Self {
        Self {
            x: self.x * other,
            y: self.y * other,
        }
    }
}

impl_assign_binop!(DivAssign, Rhs = f32, div, div_assign);
impl_ref_binop!(Div, &f32, div);
impl_ref_assign_binop!(DivAssign, &f32, div_assign);
