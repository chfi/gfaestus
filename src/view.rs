use crate::geometry::Point;

use nalgebra_glm as glm;

#[rustfmt::skip]
    #[inline]
pub fn viewport_scale(width: f32, height: f32) -> glm::Mat4 {
    let w = width;
    let h = height;
    glm::mat4(2.0 / w, 0.0,     0.0, 0.0,
              0.0,     2.0 / h, 0.0, 0.0,
              0.0,     0.0,     1.0, 0.0,
              0.0,     0.0,     0.0, 1.0)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, PartialOrd)]
pub struct ScreenDims {
    pub width: f32,
    pub height: f32,
}

impl From<Point> for ScreenDims {
    #[inline]
    fn from(point: Point) -> Self {
        Self {
            width: point.x,
            height: point.y,
        }
    }
}

impl Into<Point> for ScreenDims {
    #[inline]
    fn into(self) -> Point {
        Point {
            x: self.width,
            y: self.height,
        }
    }
}

impl From<(f32, f32)> for ScreenDims {
    #[inline]
    fn from((width, height): (f32, f32)) -> Self {
        Self { width, height }
    }
}

impl From<[f32; 2]> for ScreenDims {
    #[inline]
    fn from(dims: [f32; 2]) -> Self {
        Self {
            width: dims[0],
            height: dims[1],
        }
    }
}

impl From<[u32; 2]> for ScreenDims {
    #[inline]
    fn from(dims: [u32; 2]) -> Self {
        Self {
            width: dims[0] as f32,
            height: dims[1] as f32,
        }
    }
}

impl Into<[f32; 2]> for ScreenDims {
    #[inline]
    fn into(self) -> [f32; 2] {
        [self.width, self.height]
    }
}

impl Into<[u32; 2]> for ScreenDims {
    #[inline]
    fn into(self) -> [u32; 2] {
        [self.width as u32, self.height as u32]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct View {
    pub center: Point,
    pub scale: f32,
}

impl Default for View {
    #[inline]
    fn default() -> Self {
        Self {
            center: Point::new(0.0, 0.0),
            scale: 10.0,
        }
    }
}

impl View {
    pub fn from_dims_and_target<D: Into<ScreenDims>>(
        screen_dims: D,
        p_0: Point,
        p_1: Point,
    ) -> Self {
        let dims = screen_dims.into();

        let top_left = Point {
            x: p_0.x.min(p_1.x),
            y: p_0.y.min(p_1.y),
        };

        let bottom_right = Point {
            x: p_0.x.max(p_1.x),
            y: p_0.y.max(p_1.y),
        };

        let target_dims = Point {
            x: bottom_right.x - top_left.x,
            y: bottom_right.y - top_left.y,
        };

        let scale = if target_dims.x > target_dims.y {
            let width = target_dims.x;
            width / dims.width
        } else {
            let height = target_dims.y;
            height / dims.height
        };

        let scale = scale * 1.05; // add a bit extra so everything fits

        let center = top_left + (target_dims * 0.5);

        View { center, scale }
    }

    #[rustfmt::skip]
    #[inline]
    pub fn to_scaled_matrix(&self) -> glm::Mat4 {

        let scale = 1.0 / self.scale;

        let scaling =
            glm::mat4(scale, 0.0,   0.0, 0.0,
                      0.0,   scale, 0.0, 0.0,
                      0.0,   0.0,   1.0, 1.0,
                      0.0,   0.0,   0.0, 1.0);

        let x_ = self.center.x;
        let y_ = self.center.y;

        let translation =
            glm::mat4(1.0, 0.0, 0.0, -x_,
                      0.0, 1.0, 0.0, -y_,
                      0.0, 0.0, 1.0, 0.0,
                      0.0, 0.0, 0.0, 1.0);

        scaling * translation
    }

    #[rustfmt::skip]
    #[inline]
    pub fn world_to_screen_map(&self) -> glm::Mat4 {
        let s = 1.0 / self.scale;
        let vcx = self.center.x;

        let view_scale_screen =
            glm::mat4(s,   0.0, 0.0, (s * 0.5) - vcx,
                      0.0, s,   0.0, (s * 0.5) - vcx,
                      0.0, 0.0, 1.0, 0.0,
                      0.0, 0.0, 0.0, 1.0);

        view_scale_screen
    }

    #[rustfmt::skip]
    #[inline]
    pub fn screen_to_world_map<Dims: Into<ScreenDims>>(&self, dims: Dims) -> glm::Mat4 {
        let dims = dims.into();

        let w = dims.width;
        let h = dims.height;

        let s = self.scale;
        let vcx = self.center.x;
        let vcy = self.center.y;

        // transform from screen coords (top left (0, 0), bottom right (w, h))
        // to screen center = (0, 0), bottom right (w/2, h/2);
        //
        // then scale so bottom right = (s*w/2, s*h/2);
        //
        // finally translate by view center to world coordinates
        //
        // i.e. view_offset * scale * screen_center
        let view_scale_screen =
            glm::mat4(s,   0.0, 0.0, vcx - (w * s * 0.5),
                      0.0, s,   0.0, vcy - (h * s * 0.5),
                      0.0, 0.0, 1.0, 0.0,
                      0.0, 0.0, 0.0, 1.0);

        view_scale_screen
    }

    #[rustfmt::skip]
    #[inline]
    pub fn screen_point_to_world<Dims: Into<ScreenDims>>(&self, dims: Dims, screen_point: Point) -> Point {
        let to_world_mat = self.screen_to_world_map(dims);

        let projected = to_world_mat * glm::vec4(screen_point.x, screen_point.y, 0.0, 1.0);

        Point { x: projected[0], y: projected[1] }
    }

    pub fn world_point_to_screen(&self, world: Point) -> Point {
        let to_screen_mat = self.to_scaled_matrix();

        let projected = to_screen_mat * glm::vec4(world.x, world.y, 0.0, 1.0);

        Point {
            x: projected[0],
            y: projected[1],
        }
    }
}

#[inline]
pub fn mat4_to_array(matrix: &glm::Mat4) -> [[f32; 4]; 4] {
    let s = glm::value_ptr(matrix);

    let col0 = [s[0], s[1], s[2], s[3]];
    let col1 = [s[4], s[5], s[6], s[7]];
    let col2 = [s[8], s[9], s[10], s[11]];
    let col3 = [s[12], s[13], s[14], s[15]];

    [col0, col1, col2, col3]
}
