use ash::{version::DeviceV1_0, vk, Device};

use anyhow::Result;

use colorous::Gradient;

use crate::vulkan::GfaestusVk;

use super::Texture1D;

pub struct GradientTexture {
    pub texture: Texture1D,
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
        assert!(
            width.is_power_of_two(),
            "GradientTexture width has to be a power of two"
        );

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

    pub fn create_sampler(device: &Device) -> Result<vk::Sampler> {
        let sampler = {
            let sampler_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .anisotropy_enable(false)
                .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
                .unnormalized_coordinates(false)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .mip_lod_bias(0.0)
                .min_lod(0.0)
                .max_lod(1.0)
                .build();

            unsafe { device.create_sampler(&sampler_info, None) }
        }?;

        Ok(sampler)
    }
}
