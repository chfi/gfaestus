use crate::{
    geometry::{Point, Rect},
    view::ScreenDims,
    vulkan::tiles::ScreenTiles,
};

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;
use handlegraph::handle::Handle;

use crate::app::node_flags::SelectionBuffer;

use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

use super::{ComputeManager, ComputePipeline};

pub struct EdgeRenderer {
    // render_pipeline: ComputePipeline,
    test_pipeline: ComputePipeline,
    test_desc_set: vk::DescriptorSet,

    bin_pipeline: ComputePipeline,
    bin_desc_set: vk::DescriptorSet,

    // edge_buffer: EdgeBuffer,
    pub tiles: ScreenTiles,

    pub edges: EdgeBuffers,
    pub mask: MaskBuffer,
}

impl EdgeRenderer {
    pub fn new<Dims: Into<ScreenDims>>(
        app: &GfaestusVk,
        dims: Dims,
        edge_count: usize,
    ) -> Result<Self> {
        let tiles = ScreenTiles::new(app, 128, 128, Point::ZERO, dims)?;

        let mask = MaskBuffer::new(app, 128, 128)?;

        let device = app.vk_context().device();

        let test_pipeline = Self::create_test_pipeline(device)?;

        let bin_pipeline = Self::create_bin_pipeline(device)?;

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

        let bin_descriptor_sets = {
            let layouts = vec![bin_pipeline.descriptor_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(bin_pipeline.descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        let bin_desc_set = bin_descriptor_sets[0];

        let edges = EdgeBuffers::new(app, edge_count)?;

        Ok(Self {
            test_pipeline,
            test_desc_set,
            bin_pipeline,
            bin_desc_set,
            tiles,
            mask,
            edges,
        })
    }

    pub fn upload_example_data(&self, app: &GfaestusVk) -> Result<()> {
        let edge_data: Vec<[u32; 2]> = vec![[1, 2], [3, 4], [3, 6]];

        app.copy_data_to_buffer::<[u32; 2], [u32; 2]>(
            &edge_data,
            self.edges.edges_by_id_buf,
        )?;

        let p1 = Point::new(170.0, 100.0);
        let p2 = Point::new(230.0, 200.0);
        let p3 = Point::new(270.0, 180.0);
        let p4 = Point::new(500.0, 500.0);
        let p6 = Point::new(300.0, 500.0);

        let pos_data: Vec<[f32; 4]> = vec![
            [p1.x, p1.y, p2.x, p2.y],
            [p3.x, p3.y, p4.x, p4.y],
            [p3.x, p3.y, p6.x, p6.y],
        ];

        app.copy_data_to_buffer::<[f32; 4], [f32; 4]>(
            &pos_data,
            self.edges.edges_pos_buf,
        )?;

        Ok(())
    }

    pub fn upload_edges<E>(&self, app: &GfaestusVk, edges: E) -> Result<()>
    where
        E: Iterator<Item = (Handle, Handle)>,
    {
        let edge_data: Vec<[u32; 2]> = edges
            .map(|(a, b)| [a.as_integer() as u32, b.as_integer() as u32])
            .collect();

        app.copy_data_to_buffer::<[u32; 2], [u32; 2]>(
            &edge_data,
            self.edges.edges_by_id_buf,
        )?;

        Ok(())
    }

    pub fn bin_draw_cmd(
        &self,
        cmd_buf: vk::CommandBuffer,
        viewport_dims: [f32; 2],
    ) -> Result<()> {
        let device = &self.bin_pipeline.device;

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.bin_pipeline.pipeline,
            )
        };

        unsafe {
            let desc_sets = [self.bin_desc_set];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.bin_pipeline.pipeline_layout,
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
                self.bin_pipeline.pipeline_layout,
                Flags::COMPUTE,
                0,
                &pc_bytes,
            )
        };

        let x_group_count: u32 = 128 / 32;
        let y_group_count: u32 = 96 / 32;
        let z_group_count: u32 = {
            let mut size = self.edges.edge_count / 16;
            if self.edges.edge_count % 16 != 0 {
                size += 1;
            }
            size as u32
        };

        // let x_group_count = {
        //     let mut size = self.edges.edge_count / 16;
        //     if self.edges.edge_count % 16 != 0 {
        //         size += 1;
        //     }
        //     size as u32
        // };

        // println!("dispatching edge bin groups: {}", x_group_count);

        /*
        println!("edge binning");
        println!("  x_group_count: {}", x_group_count);
        println!("  y_group_count: {}", y_group_count);
        println!("  z_group_count: {}", z_group_count);
        */
        // let y_group_count = 128;

        unsafe {
            device.cmd_dispatch(
                cmd_buf,
                x_group_count,
                y_group_count,
                z_group_count,
            )
        };

        Ok(())
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

    pub fn write_bin_descriptor_set(
        &self,
        device: &Device,
        nodes: &NodeVertices,
    ) -> Result<()> {
        let node_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(nodes.buffer())
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let node_buf_infos = [node_buf_info];

        let nodes = vk::WriteDescriptorSet::builder()
            .dst_set(self.bin_desc_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&node_buf_infos)
            .build();

        let edge_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.edges.edges_by_id_buf)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let edge_buf_infos = [edge_buf_info];

        let edges = vk::WriteDescriptorSet::builder()
            .dst_set(self.bin_desc_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&edge_buf_infos)
            .build();

        let mask_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.mask.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let mask_buf_infos = [mask_buf_info];

        let masks = vk::WriteDescriptorSet::builder()
            .dst_set(self.bin_desc_set)
            .dst_binding(2)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&mask_buf_infos)
            .build();

        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::GENERAL)
            .image_view(self.tiles.tile_texture.view)
            .sampler(self.tiles.sampler)
            .build();
        let image_infos = [image_info];

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.bin_desc_set)
            .dst_binding(3)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .image_info(&image_infos)
            .build();

        let descriptor_writes = [nodes, edges, masks, sampler_descriptor_write];

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };

        Ok(())
    }

    fn create_bin_pipeline(device: &Device) -> Result<ComputePipeline> {
        let bindings = {
            use vk::ShaderStageFlags as Stages;

            let nodes = vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            let edges = vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            let masks = vk::DescriptorSetLayoutBinding::builder()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            let image = vk::DescriptorSetLayoutBinding::builder()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            [nodes, edges, masks, image]
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

        let nodes_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };

        let edges_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };

        let masks_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };

        let image_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_IMAGE,
            descriptor_count: 1,
        };

        // let pool_sizes = [nodes_pool_size, edges_pool_size, masks_pool_size];
        let pool_sizes = [
            nodes_pool_size,
            edges_pool_size,
            masks_pool_size,
            image_pool_size,
        ];

        let bin_pipeline = ComputePipeline::new_with_pool_size(
            device,
            descriptor_set_layout,
            &pool_sizes,
            pipeline_layout,
            crate::include_shader!("edges/edge_binning.comp.spv"),
        )?;

        Ok(bin_pipeline)
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

        let pool_sizes = [pool_size];

        let test_pipeline = ComputePipeline::new_with_pool_size(
            device,
            descriptor_set_layout,
            &pool_sizes,
            pipeline_layout,
            crate::include_shader!("edges/edge.comp.spv"),
        )?;

        Ok(test_pipeline)
    }
}

pub struct EdgeBuffers {
    edges_by_id_buf: vk::Buffer,
    edges_by_id_mem: vk::DeviceMemory,
    edges_by_id_size: vk::DeviceSize,

    edges_pos_buf: vk::Buffer,
    edges_pos_mem: vk::DeviceMemory,
    edges_pos_size: vk::DeviceSize,

    edge_count: usize,
}

pub struct MaskBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,

    // tiles: usize
    rows: usize,
    columns: usize,
    // depth: usize,
}

impl MaskBuffer {
    // pub fn new(app: &GfaestusVk, rows: usize, columns: usize, depth: usize) -> Result<Self> {
    pub fn new(app: &GfaestusVk, rows: usize, columns: usize) -> Result<Self> {
        let tile_count = rows * columns;

        let (buffer, memory, size) = {
            let size = ((tile_count * std::mem::size_of::<u32>()) as u32)
                as vk::DeviceSize;

            let usage = vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::STORAGE_BUFFER;

            let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_CACHED
                | vk::MemoryPropertyFlags::HOST_COHERENT;

            app.create_buffer(size, usage, mem_props)
        }?;

        Ok(Self {
            buffer,
            memory,
            size,

            rows,
            columns,
        })
    }
}

impl EdgeBuffers {
    pub fn new(app: &GfaestusVk, edge_count: usize) -> Result<Self> {
        let (edges_by_id_buf, edges_by_id_mem, edges_by_id_size) = {
            let size = ((edge_count * 2 * std::mem::size_of::<u32>()) as u32)
                as vk::DeviceSize;

            let usage = vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::STORAGE_BUFFER;

            let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_CACHED
                | vk::MemoryPropertyFlags::HOST_COHERENT;

            app.create_buffer(size, usage, mem_props)
        }?;

        let (edges_pos_buf, edges_pos_mem, edges_pos_size) = {
            let size = ((edge_count * 2 * 2 * std::mem::size_of::<f32>())
                as u32) as vk::DeviceSize;

            let usage = vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::STORAGE_BUFFER;

            let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_CACHED
                | vk::MemoryPropertyFlags::HOST_COHERENT;

            app.create_buffer(size, usage, mem_props)
        }?;

        Ok(Self {
            edges_by_id_buf,
            edges_by_id_mem,
            edges_by_id_size,

            edges_pos_buf,
            edges_pos_mem,
            edges_pos_size,

            edge_count,
        })
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
