use ash::{version::DeviceV1_0, vk, Device};

use anyhow::Result;

use colorous::Gradient;

use crate::vulkan::GfaestusVk;

use super::Texture1D;

pub struct GradientTexture {
    texture: Texture1D,
    gradient: Gradient,
}

impl GradientTexture {
    pub fn new(
        app: &GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        gradient: Gradient,
        width: usize,
    ) -> Result<Self> {
        let mut colors: Vec<rgb::RGB<f32>> = Vec::with_capacity(width);

        for i in 0..width {
            let (r, g, b) = gradient.eval_rational(i, width).as_tuple();

            let r = (r as f32) / 255.0;
            let g = (g as f32) / 255.0;
            let b = (b as f32) / 255.0;

            let rgb_color = rgb::RGB::new(r, g, b);

            colors.push(rgb_color);
        }

        let texture = Texture1D::create_from_colors(
            app,
            command_pool,
            transition_queue,
            &colors,
        )?;

        Ok(Self { texture, gradient })
    }
}
