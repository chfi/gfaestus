use crate::geometry::Point;

use nalgebra_glm as glm;

// use nalgebra::{Matrix3, Matrix4};

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
            scale: 1.0,
            width: 100.0,
            height: 100.0,
        }
    }
}

impl View {
    pub fn to_matrix(&self) -> glm::Mat4 {
        unit_scale_matrix(self.width, self.height)
    }

#[rustfmt::skip]
    pub fn to_scaled_matrix(&self) -> glm::Mat4 {

        let o_x = self.center.x;
        let o_y = self.center.y;

        let w = self.width;
        let h = self.height;

        let w_2 = w / 2.0;
        let h_2 = h / 2.0;

        let l = -w_2;
        let r = w_2;

        let t = -h_2;
        let b = h_2;

        let w_scale = (2.0 / self.width) / self.scale;
        let h_scale = (2.0 / self.height) / self.scale;

        let projection =
            glm::mat4(w_scale, 0.0,     0.0, -((r + l) / (r - l)),
                      0.0,     h_scale, 0.0, -((t + b) / (t - b)),
                      0.0,     0.0,     1.0, 1.0,
                      0.0,     0.0,     0.0, 1.0);

        // let x_ = self.center.x * self.scale;
        // let y_ = self.center.y * self.scale;

        let x_ = self.center.x;
        let y_ = self.center.y;

        // let x_ = self.center.x / w_scale;
        // let y_ = self.center.y / h_scale;

        let translation =
            glm::mat4(1.0, 0.0, 0.0, x_,
                      0.0, 1.0, 0.0, y_,
                      0.0, 0.0, 1.0, 0.0,
                      0.0, 0.0, 0.0, 1.0);

        projection * translation
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

#[rustfmt::skip]
pub fn unit_scale_matrix(width: f32, height: f32) -> glm::Mat4 {
    let w = width as f32;
    let h = height as f32;

    let scale = 10.0;

    let w_scale = 2.0 / width;
    let h_scale = 2.0 / height;

    // glm::mat4(1.0, 0.0, 0.0, 0.0,
    //           0.0, 1.0, 0.0, 0.0,
    //           0.0, 0.0, 1.0, 0.0,
    //           0.0, 0.0, 0.0, 1.0)

    glm::mat4(w_scale, 0.0,     0.0, 0.0,
              0.0,     h_scale, 0.0, 0.0,
              0.0,     0.0,     1.0, 0.0,
              0.0,     0.0,     0.0, 1.0)


    // glm::mat4(2.0 / r_sub_l, 0.0,           0.0,            -(r_add_l / r_sub_l),
    //           0.0,           2.0 / t_sub_b, 0.0,            -(t_add_b / t_sub_b),
    //           // 0.0,           0.0,           -2.0 / f_sub_n, -(f_add_n / f_sub_n),
    //           0.0,           0.0,           1.0, 1.0,
    //           0.0,           0.0,           0.0,             1.0)

    // glm::mat4(scale / w_2, 0.0,         0.0, 0.0,
    //           0.0,         scale / h_2, 0.0, 0.0,
    //           0.0,         0.0,         1.0, 0.0,
    //           0.0,         0.0,         0.0, 1.0)

    // glm::mat4(scale / w_2, 0.0,         0.0, 0.0,
    //           0.0,         scale / h_2, 0.0, 0.0,
    //           0.0,         0.0,         1.0, 0.0,
    //           0.0,         0.0,         0.0, 1.0)

    // glm::mat4(w_2 / scale, 0.0,         0.0, 0.0,
    //           0.0,         h_2 / scale, 0.0, 0.0,
    //           0.0,         0.0,         1.0, 0.0,
    //           0.0,         0.0,         0.0, 1.0)

    // glm::mat4(w_2 * scale, 0.0,         0.0, 0.0,
    //           0.0,         h_2 * scale, 0.0, 0.0,
    //           0.0,         0.0,         1.0, 0.0,
    //           0.0,         0.0,         0.0, 1.0)


    // glm::mat4((w * scale) / 2.0, 0.0,              0.0, 0.0,
    //           0.0,              (h * scale) / 2.0, 0.0, 0.0,
    //           0.0,               0.0,              1.0, 0.0,
    //           0.0,               0.0,              0.0, 1.0)
}

#[rustfmt::skip]
pub fn projection_matrix(w: f32, h: f32) -> glm::Mat4 {
    let scale = 10.0;

    let w_2 = w / 2.0;
    let h_2 = h / 2.0;


    // glm::mat4(scale / w_2, 0.0,         0.0, 0.0,
    //           0.0,         scale / h_2, 0.0, 0.0,
    //           0.0,         0.0,         1.0, 0.0,
    //           0.0,         0.0,         0.0, 1.0)

    // glm::mat4(w_2 / scale, 0.0,         0.0, 0.0,
    //           0.0,         h_2 / scale, 0.0, 0.0,
    //           0.0,         0.0,         1.0, 0.0,
    //           0.0,         0.0,         0.0, 1.0)

    // glm::mat4(w_2 * scale, 0.0,         0.0, 0.0,
    //           0.0,         h_2 * scale, 0.0, 0.0,
    //           0.0,         0.0,         1.0, 0.0,
    //           0.0,         0.0,         0.0, 1.0)

    unimplemented!();
}

// pub fn scale_matrix(dist: f32, width: usize, height: usize) -> glm::Mat4 {
// }
