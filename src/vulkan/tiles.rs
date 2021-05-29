use ash::version::DeviceV1_0;
use ash::{vk, Device};

use std::{collections::HashMap, ffi::CString};

use anyhow::Result;

use super::{render_pass::Framebuffers, GfaestusVk};
use crate::geometry::*;
use crate::view::*;
use crate::vulkan::texture::Texture;

pub struct ScreenTiles {
    pub tile_texture: Texture,
    pub sampler: vk::Sampler,

    rows: usize,
    columns: usize,

    max_rows: usize,
    max_columns: usize,

    offset: Point,
    size: Point,
}

impl ScreenTiles {
    pub const TILE_WIDTH: u32 = 16;
    pub const TILE_HEIGHT: u32 = 16;

    pub fn new<Dims: Into<ScreenDims>>(
        app: &GfaestusVk,
        max_rows: usize,
        max_columns: usize,
        offset: Point,
        dims: Dims,
    ) -> Result<Self> {
        assert!(max_rows.is_power_of_two() && max_columns.is_power_of_two());

        let dims = dims.into();

        let width = dims.width as usize;
        let height = dims.height as usize;

        let mut rows = height / 16;
        if height % 16 != 0 {
            rows += 1;
        }

        let mut columns = width / 16;
        if width % 16 != 0 {
            columns += 1;
        }

        assert!(rows <= max_rows && columns <= max_columns);

        let size = Point::new(dims.width, dims.height);

        let tile_texture = Texture::allocate(
            app,
            app.transient_command_pool,
            app.graphics_queue,
            4096,
            4096,
            vk::Format::R8G8B8A8_UNORM,
        )?;

        let device = app.vk_context().device();

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
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .mip_lod_bias(0.0)
                .min_lod(0.0)
                .max_lod(1.0)
                .build();

            unsafe { device.create_sampler(&sampler_info, None) }
        }?;

        /*
        use vk::ImageLayout as Layout;
        super::GfaestusVk::transition_image(
            device,
            app.transient_command_pool,
            app.graphics_queue,
            tile_texture.image,
            vk::Format::R8G8B8A8_UNORM,
            Layout::SHADER_READ_ONLY_OPTIMAL,
            Layout::GENERAL,
        )?;
        */

        Ok(Self {
            tile_texture,
            sampler,

            rows,
            columns,

            max_rows,
            max_columns,

            offset,
            size,
        })
    }

    pub fn transition_to_shader_read_only(
        &self,
        app: &GfaestusVk,
    ) -> Result<()> {
        use vk::ImageLayout as Layout;

        super::GfaestusVk::transition_image(
            app.vk_context().device(),
            app.transient_command_pool,
            app.graphics_queue,
            self.tile_texture.image,
            vk::Format::R8G8B8A8_UNORM,
            Layout::GENERAL,
            Layout::SHADER_READ_ONLY_OPTIMAL,
        )?;

        Ok(())
    }

    pub fn transition_to_general(&self, app: &GfaestusVk) -> Result<()> {
        use vk::ImageLayout as Layout;

        super::GfaestusVk::transition_image(
            app.vk_context().device(),
            app.transient_command_pool,
            app.graphics_queue,
            self.tile_texture.image,
            vk::Format::R8G8B8A8_UNORM,
            Layout::SHADER_READ_ONLY_OPTIMAL,
            Layout::GENERAL,
        )?;

        Ok(())
    }
}
