use crate::{
    geometry::{Point, Rect},
    view::ScreenDims,
    vulkan::tiles::ScreenTiles,
};

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;

use crate::app::node_flags::SelectionBuffer;

use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

use super::{ComputeManager, ComputePipeline};

pub struct EdgeRenderer {
    // bin_pipeline: ComputePipeline,
    // render_pipeline: ComputePipeline,
    test_pipeline: ComputePipeline,

    test_desc_set: vk::DescriptorSet,

    // edge_buffer: EdgeBuffer,
    pub tiles: ScreenTiles,
}

impl EdgeRenderer {
    pub fn new<Dims: Into<ScreenDims>>(
        app: &GfaestusVk,
        dims: Dims,
        edge_count: usize,
    ) -> Result<Self> {
        let tiles = ScreenTiles::new(app, 128, 128, Point::ZERO, dims)?;

        let device = app.vk_context().device();

        let test_pipeline = Self::create_test_pipeline(device)?;

        let descriptor_sets = {
            let layouts = vec![test_pipeline.descriptor_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(test_pipeline.descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        let test_desc_set = descriptor_sets[0];

        {
            let image_info = vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::GENERAL)
                .image_view(tiles.tile_texture.view)
                .sampler(tiles.sampler)
                .build();
            let image_infos = [image_info];

            let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
                .dst_set(test_desc_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&image_infos)
                .build();

            let descriptor_writes = [sampler_descriptor_write];

            unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) }
        }

        Ok(Self {
            test_pipeline,
            test_desc_set,
            tiles,
        })
    }

    pub fn test_draw_cmd(
        &self,
        cmd_buf: vk::CommandBuffer,
        viewport_dims: [f32; 2],
    ) -> Result<()> {
        let device = &self.test_pipeline.device;

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.test_pipeline.pipeline,
            )
        };

        unsafe {
            let desc_sets = [self.test_desc_set];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.test_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=0],
                &null,
            );
        };

        let screen_size = Point {
            x: viewport_dims[0],
            y: viewport_dims[1],
        };
        let push_constants = PushConstants::new(screen_size, 128, 128);
        let pc_bytes = push_constants.bytes();

        unsafe {
            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.test_pipeline.pipeline_layout,
                Flags::COMPUTE,
                0,
                &pc_bytes,
            )
        };

        let x_group_count = 128;
        let y_group_count = 128;

        unsafe {
            device.cmd_dispatch(cmd_buf, x_group_count, y_group_count, 1)
        };

        Ok(())
    }

    fn create_test_pipeline(device: &Device) -> Result<ComputePipeline> {
        let bindings = {
            use vk::ShaderStageFlags as Stages;

            let texture_output = vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            [texture_output]
        };

        let descriptor_set_layout = {
            let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings)
                .build();

            let layout = unsafe {
                device.create_descriptor_set_layout(&layout_info, None)
            }?;
            layout
        };

        let pipeline_layout = {
            use vk::ShaderStageFlags as Flags;

            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(Flags::COMPUTE)
                .offset(0)
                .size(PushConstants::PC_RANGE)
                .build();

            let pc_ranges = [pc_range];

            let layouts = [descriptor_set_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe { device.create_pipeline_layout(&layout_info, None) }
        }?;

        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_IMAGE,
            descriptor_count: 1,
        };

        let test_pipeline = ComputePipeline::new_with_pool_size(
            device,
            descriptor_set_layout,
            pool_size,
            pipeline_layout,
            crate::include_shader!("edges/edge.comp.spv"),
        )?;

        Ok(test_pipeline)
    }
}

pub struct EdgeBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,

    edge_count: usize,
}

impl EdgeBuffer {
    pub fn new(app: &GfaestusVk, edge_count: usize) -> Result<Self> {
        let size = ((edge_count * 2 * std::mem::size_of::<u32>()) as u32)
            as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::TRANSFER_SRC
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_CACHED
            | vk::MemoryPropertyFlags::HOST_COHERENT;

        let (buffer, memory, size) =
            app.create_buffer(size, usage, mem_props)?;

        // let latest_selection = FxHashSet::default();

        Ok(Self {
            // latest_selection,
            // node_count,
            buffer,
            memory,
            size,

            edge_count,
        })
    }
}

pub struct PushConstants {
    screen_size: Point,
    tile_texture_size: Point,
}

impl PushConstants {
    pub const PC_RANGE: u32 = (std::mem::size_of::<f32>() * 4) as u32;

    #[inline]
    pub fn new(
        screen_size: Point,
        tiles_wide: usize,
        tiles_high: usize,
    ) -> Self {
        let tile_texture_size = Point {
            x: (tiles_wide * 16) as f32,
            y: (tiles_high * 16) as f32,
        };

        Self {
            screen_size,
            tile_texture_size,
        }
    }

    #[inline]
    pub fn bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; Self::PC_RANGE as usize];

        {
            let mut offset = 0;

            let mut add_float = |f: f32| {
                let f_bytes = f.to_ne_bytes();
                for i in 0..4 {
                    bytes[offset] = f_bytes[i];
                    offset += 1;
                }
            };

            add_float(self.screen_size.x);
            add_float(self.screen_size.y);
            add_float(self.tile_texture_size.x);
            add_float(self.tile_texture_size.y);
        }

        bytes
    }
}
