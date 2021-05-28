use ash::version::DeviceV1_0;
use ash::{vk, Device};

use std::{collections::HashMap, ffi::CString};

use anyhow::Result;

use super::GfaestusVk;
use crate::geometry::*;
use crate::view::*;
use crate::vulkan::texture::Texture;

pub struct ScreenTiles {
    tile_texture: Texture,

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

        Ok(Self {
            tile_texture,

            rows,
            columns,

            max_rows,
            max_columns,

            offset,
            size,
        })
    }
}
