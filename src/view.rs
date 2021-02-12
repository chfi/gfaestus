use crate::geometry::Point;

use nalgebra_glm as glm;

/// the "default" scale is such that the node width is 10px
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct View {
    pub center: Point,
    pub scale: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            center: Point::new(0.0, 0.0),
            // scale: 1.0,
            scale: 10.0,
            width: 100.0,
            height: 100.0,
        }
    }
}

impl View {
    #[rustfmt::skip]
    pub fn to_scaled_matrix(&self) -> glm::Mat4 {

        let w = self.width;
        let h = self.height;

        let w_scale = 2.0 / (self.width * self.scale);
        let h_scale = 2.0 / (self.height * self.scale);

        let scaling =
            glm::mat4(w_scale, 0.0,     0.0, 0.0,
                      0.0,     h_scale, 0.0, 0.0,
                      0.0,     0.0,     1.0, 1.0,
                      0.0,     0.0,     0.0, 1.0);


        let ratio = w / h;

        let x_ = self.center.x * ratio;
        let y_ = self.center.y;

        let translation =
            glm::mat4(1.0, 0.0, 0.0, -x_,
                      0.0, 1.0, 0.0, -y_,
                      0.0, 0.0, 1.0, 0.0,
                      0.0, 0.0, 0.0, 1.0);

        scaling * translation
    }

    #[rustfmt::skip]
    pub fn to_rotated_scaled_matrix(&self, angle: f32) -> glm::Mat4 {

        let w = self.width;
        let h = self.height;

        let w_scale = 2.0 / (self.width * self.scale);
        let h_scale = 2.0 / (self.height * self.scale);

        let scaling =
            glm::mat4(w_scale, 0.0,     0.0, 0.0,
                      0.0,     h_scale, 0.0, 0.0,
                      0.0,     0.0,     1.0, 1.0,
                      0.0,     0.0,     0.0, 1.0);


        let ratio = w / h;

        let x_ = self.center.x * ratio;
        let y_ = self.center.y;

        let translation =
            glm::mat4(1.0, 0.0, 0.0, x_,
                      0.0, 1.0, 0.0, y_,
                      0.0, 0.0, 1.0, 0.0,
                      0.0, 0.0, 0.0, 1.0);

        let cos_t = angle.cos();
        let sin_t = angle.sin();

        let rotation = glm::mat4(cos_t, -sin_t, 0.0, 0.0,
                                 sin_t,  cos_t, 0.0, 0.0,
                                 0.0,    0.0,   1.0, 0.0,
                                 0.0,    0.0,   0.0, 1.0);


        scaling * translation
        // translation * scaling
    }
}

pub fn mat4_to_array(matrix: &glm::Mat4) -> [[f32; 4]; 4] {
    let s = glm::value_ptr(matrix);

    // let row0 = [s[0], s[4], s[8], s[12]];
    // let row1 = [s[1], s[5], s[9], s[13]];
    // let row2 = [s[2], s[6], s[10], s[14]];
    // let row3 = [s[3], s[7], s[11], s[15]];

    // [row0, row1, row2, row3]

    let col0 = [s[0], s[1], s[2], s[3]];
    let col1 = [s[4], s[5], s[6], s[7]];
    let col2 = [s[8], s[9], s[10], s[11]];
    let col3 = [s[12], s[13], s[14], s[15]];

    [col0, col1, col2, col3]
}
