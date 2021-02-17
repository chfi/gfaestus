use crate::geometry::Point;

use nalgebra_glm as glm;

#[rustfmt::skip]
pub fn viewport_scale(width: f32, height: f32) -> glm::Mat4 {
    let w = width;
    let h = height;
    glm::mat4(2.0 / w, 0.0,     0.0, 0.0,
              0.0,     2.0 / h, 0.0, 0.0,
              0.0,     0.0,     1.0, 0.0,
              0.0,     0.0,     0.0, 1.0)
}

/// the "default" scale is such that the node width is 10px
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct View {
    pub center: Point,
    pub scale: f32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            center: Point::new(0.0, 0.0),
            scale: 10.0,
        }
    }
}

impl View {
    #[rustfmt::skip]
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

        scaling  * translation
    }
}

pub fn mat4_to_array(matrix: &glm::Mat4) -> [[f32; 4]; 4] {
    let s = glm::value_ptr(matrix);

    let col0 = [s[0], s[1], s[2], s[3]];
    let col1 = [s[4], s[5], s[6], s[7]];
    let col2 = [s[8], s[9], s[10], s[11]];
    let col3 = [s[12], s[13], s[14], s[15]];

    [col0, col1, col2, col3]
}
