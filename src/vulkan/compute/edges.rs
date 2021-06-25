use crate::{
    geometry::{Point, Rect},
    view::{ScreenDims, View},
    vulkan::tiles::ScreenTiles,
};

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;
use handlegraph::handle::Handle;

use nalgebra_glm as glm;

use crate::app::node_flags::SelectionBuffer;

use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

use super::{ComputeManager, ComputePipeline};

pub struct EdgeRenderer {
    // render_pipeline: ComputePipeline,
    test_pipeline: ComputePipeline,
    test_desc_set: vk::DescriptorSet,

    bin_pipeline: ComputePipeline,
    bin_desc_set: vk::DescriptorSet,

    render_pipeline: ComputePipeline,
    render_desc_set: vk::DescriptorSet,

    preprocess_pipeline: ComputePipeline,
    preprocess_desc_set: vk::DescriptorSet,

    populate_slot_pipeline: ComputePipeline,
    populate_slot_desc_set: vk::DescriptorSet,

    slot_render_pipeline: ComputePipeline,
    slot_render_desc_set: vk::DescriptorSet,

    // edge_buffer: EdgeBuffer,
    pub tiles: ScreenTiles,

    pub edges: EdgeBuffers,
    pub mask: MaskBuffer,
    pub tile_slots: TileSlots,
    pub pixels: PixelBuffer,
}

impl EdgeRenderer {
    pub fn new<Dims: Into<ScreenDims>>(
        app: &GfaestusVk,
        dims: Dims,
        edge_count: usize,
    ) -> Result<Self> {
        let tiles = ScreenTiles::new(app, 128, 128, Point::ZERO, dims)?;

        let mask = MaskBuffer::new(app, 128, 128, edge_count)?;

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

        let render_pipeline = Self::create_render_pipeline(device)?;

        let render_descriptor_sets = {
            let layouts = vec![render_pipeline.descriptor_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(render_pipeline.descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        let render_desc_set = render_descriptor_sets[0];

        let preprocess_pipeline = Self::create_preprocess_pipeline(device)?;

        let preprocess_descriptor_sets = {
            let layouts = vec![preprocess_pipeline.descriptor_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(preprocess_pipeline.descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        let preprocess_desc_set = preprocess_descriptor_sets[0];

        let populate_slot_pipeline =
            Self::create_populate_slot_pipeline(device)?;

        let populate_slot_descriptor_sets = {
            let layouts = vec![populate_slot_pipeline.descriptor_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(populate_slot_pipeline.descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        let populate_slot_desc_set = populate_slot_descriptor_sets[0];

        let slot_render_pipeline = Self::create_slot_render_pipeline(device)?;

        let slot_render_descriptor_sets = {
            let layouts = vec![slot_render_pipeline.descriptor_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(slot_render_pipeline.descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        let slot_render_desc_set = slot_render_descriptor_sets[0];

        let edges = EdgeBuffers::new(app, edge_count)?;

        let tile_slots = TileSlots::new(app, 96, 128)?;

        let pixels = {
            let pixels = PixelBuffer::new(app, 4096, 4096)?;

            let data: Vec<u32> = vec![4096 * 4096];

            app.copy_data_to_buffer::<u32, u32>(&data, pixels.buffer)?;

            pixels
        };

        Ok(Self {
            test_pipeline,
            test_desc_set,

            bin_pipeline,
            bin_desc_set,

            render_pipeline,
            render_desc_set,

            preprocess_pipeline,
            preprocess_desc_set,

            populate_slot_pipeline,
            populate_slot_desc_set,

            slot_render_pipeline,
            slot_render_desc_set,

            tiles,
            mask,
            tile_slots,
            edges,
            pixels,
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

    pub fn bin_render_memory_barrier(
        &self,
        cmd_buf: vk::CommandBuffer,
    ) -> Result<()> {
        let device = &self.bin_pipeline.device;

        let mask_barrier = vk::BufferMemoryBarrier::builder()
            .buffer(self.mask.buffer)
            .offset(0)
            .size(self.mask.size)
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .build();

        let memory_barriers = [];
        let buffer_memory_barriers = [mask_barrier];
        let image_memory_barriers = [];

        unsafe {
            device.cmd_pipeline_barrier(
                cmd_buf,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::BY_REGION,
                &memory_barriers,
                &buffer_memory_barriers,
                &image_memory_barriers,
            );
        }

        Ok(())
    }

    pub fn pixels_memory_barrier(
        &self,
        cmd_buf: vk::CommandBuffer,
    ) -> Result<()> {
        let device = &self.render_pipeline.device;

        let pixels_barrier = vk::BufferMemoryBarrier::builder()
            .buffer(self.pixels.buffer)
            .offset(0)
            .size(self.pixels.size)
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .build();

        let memory_barriers = [];
        let buffer_memory_barriers = [pixels_barrier];
        let image_memory_barriers = [];

        unsafe {
            device.cmd_pipeline_barrier(
                cmd_buf,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::BY_REGION,
                &memory_barriers,
                &buffer_memory_barriers,
                &image_memory_barriers,
            );
        }

        Ok(())
    }

    pub fn preprocess_cmd(
        &self,
        cmd_buf: vk::CommandBuffer,
        view: View,
        viewport_dims: [f32; 2],
    ) -> Result<()> {
        let device = &self.preprocess_pipeline.device;

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.preprocess_pipeline.pipeline,
            )
        };

        unsafe {
            let desc_sets = [self.preprocess_desc_set];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.preprocess_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=0],
                &null,
            );
        };

        // let screen_size = Point {
        //     x: viewport_dims[0],
        //     y: viewport_dims[1],
        // };

        let offset = [view.center.x, view.center.y];

        let push_constants = BinPushConstants::new(
            // offset,
            [0.0, 0.0],
            viewport_dims,
            view,
            self.edges.edge_count as u32,
        );
        let pc_bytes = push_constants.bytes();

        unsafe {
            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.preprocess_pipeline.pipeline_layout,
                Flags::COMPUTE,
                0,
                &pc_bytes,
            )
        };

        let x_group_count: u32 = {
            let mut size = self.edges.edge_count / 1024;
            if self.edges.edge_count % 1024 != 0 {
                size += 1;
            }
            size as u32
        };
        let y_group_count: u32 = 1;
        let z_group_count: u32 = 1;

        println!("edge preprocessing");
        println!("  x_group_count: {}", x_group_count);
        println!("  y_group_count: {}", y_group_count);
        println!("  z_group_count: {}", z_group_count);

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

    pub fn populate_slots_cmd(&self, cmd_buf: vk::CommandBuffer) -> Result<()> {
        let device = &self.populate_slot_pipeline.device;

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.populate_slot_pipeline.pipeline,
            )
        };

        unsafe {
            let desc_sets = [self.populate_slot_desc_set];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.populate_slot_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=0],
                &null,
            );
        };

        // let screen_size = Point {
        //     x: viewport_dims[0],
        //     y: viewport_dims[1],
        // };

        /*
        let offset = [view.center.x, view.center.y];

        let push_constants = BinPushConstants::new(
            // offset,
            [0.0, 0.0],
            viewport_dims,
            view,
            self.edges.edge_count as u32,
        );
        let pc_bytes = push_constants.bytes();

        unsafe {
            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.preprocess_pipeline.pipeline_layout,
                Flags::COMPUTE,
                0,
                &pc_bytes,
            )
        };
        */

        let x_group_count: u32 = 128 / 16;
        let y_group_count: u32 = 96 / 16;
        let z_group_count: u32 = 1;

        // TODO use edge count from the preprocessing output buffer
        // let z_group_count: u32 = {
        //     let mut size = self.edges.edge_count / 1024;
        //     if self.edges.edge_count % 1024 != 0 {
        //         size += 1;
        //     }
        //     size as u32
        // };

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

    pub fn slot_render_cmd(&self, cmd_buf: vk::CommandBuffer) -> Result<()> {
        let device = &self.slot_render_pipeline.device;

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.slot_render_pipeline.pipeline,
            )
        };

        unsafe {
            let desc_sets = [self.slot_render_desc_set];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.slot_render_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=0],
                &null,
            );
        };

        let x_group_count: u32 = 4096u32 / 64;
        let y_group_count: u32 = 4096u32 / 64;
        // let x_group_count: u32 = 4096u32 / 16;
        // let y_group_count: u32 = 4096u32 / 16;
        let z_group_count: u32 = 1;

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

    pub fn bin_draw_cmd(
        &self,
        cmd_buf: vk::CommandBuffer,
        view: View,
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

        // let screen_size = Point {
        //     x: viewport_dims[0],
        //     y: viewport_dims[1],
        // };

        let offset = [view.center.x, view.center.y];

        let push_constants = BinPushConstants::new(
            // offset,
            [0.0, 0.0],
            viewport_dims,
            view,
            self.edges.edge_count as u32,
        );
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
        let z_group_count: u32 = 1;
        // let z_group_count: u32 = {
        //     let mut size = self.edges.edge_count / 32;
        //     if self.edges.edge_count % 32 != 0 {
        //         size += 1;
        //     }
        //     size as u32
        // };

        // let x_group_count = {
        //     let mut size = self.edges.edge_count / 16;
        //     if self.edges.edge_count % 16 != 0 {
        //         size += 1;
        //     }
        //     size as u32
        // };

        // println!("dispatching edge bin groups: {}", x_group_count);

        println!("edge binning");
        println!("  x_group_count: {}", x_group_count);
        println!("  y_group_count: {}", y_group_count);
        println!("  z_group_count: {}", z_group_count);
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

    pub fn edge_render_cmd(
        &self,
        cmd_buf: vk::CommandBuffer,
        view: View,
        viewport_dims: [f32; 2],
    ) -> Result<()> {
        let device = &self.render_pipeline.device;

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.render_pipeline.pipeline,
            );

            let desc_sets = [self.render_desc_set];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.render_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=0],
                &null,
            );
        };

        let offset = [view.center.x, view.center.y];

        let push_constants = BinPushConstants::new(
            // offset,
            [0.0, 0.0],
            viewport_dims,
            view,
            self.edges.edge_count as u32,
        );
        let pc_bytes = push_constants.bytes();

        unsafe {
            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.render_pipeline.pipeline_layout,
                Flags::COMPUTE,
                0,
                &pc_bytes,
            )
        };

        // let x_group_count: u32 = 4096 / 16;
        // let y_group_count: u32 = 4096 / 16;
        // let z_group_count: u32 = {
        //     let mut size = self.edges.edge_count / 32;
        //     if self.edges.edge_count % 32 != 0 {
        //         size += 1;
        //     }
        //     size as u32
        // };

        // let edge_z_group_count: u32 = {
        //     let mut size = self.edges.edge_count / 32;
        //     if self.edges.edge_count % 32 != 0 {
        //         size += 1;
        //     }
        //     size as u32
        // };

        // println!("edge z group count: {}", edge_z_group_count);

        let x_group_count = 96u32;
        let y_group_count = 64u32;
        // let z_group_count = 16u32;
        // let z_group_count = 4u32;
        let z_group_count = 1u32;

        println!("edge rendering");
        println!("  x_group_count: {}", x_group_count);
        println!("  y_group_count: {}", y_group_count);
        println!("  z_group_count: {}", z_group_count);

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

    pub fn write_preprocess_descriptor_set(
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
            .dst_set(self.preprocess_desc_set)
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
            .dst_set(self.preprocess_desc_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&edge_buf_infos)
            .build();

        let bezier_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.edges.edges_pos_buf)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let bezier_buf_infos = [bezier_buf_info];

        let beziers = vk::WriteDescriptorSet::builder()
            .dst_set(self.preprocess_desc_set)
            .dst_binding(2)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&bezier_buf_infos)
            .build();

        let descriptor_writes = [nodes, edges, beziers];

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };

        Ok(())
    }

    pub fn write_populate_slots_descriptor_set(
        &self,
        device: &Device,
    ) -> Result<()> {
        let bezier_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.edges.edges_pos_buf)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let bezier_buf_infos = [bezier_buf_info];

        let beziers = vk::WriteDescriptorSet::builder()
            .dst_set(self.populate_slot_desc_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&bezier_buf_infos)
            .build();

        let slots_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.tile_slots.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let slots_buf_infos = [slots_buf_info];

        let slots = vk::WriteDescriptorSet::builder()
            .dst_set(self.populate_slot_desc_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&bezier_buf_infos)
            .build();

        let descriptor_writes = [beziers, slots];

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };

        Ok(())
    }

    pub fn write_slot_render_descriptor_set(
        &self,
        device: &Device,
    ) -> Result<()> {
        let slots_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.tile_slots.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let slots_buf_infos = [slots_buf_info];

        let slots = vk::WriteDescriptorSet::builder()
            .dst_set(self.slot_render_desc_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&slots_buf_infos)
            .build();

        let pixels_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.pixels.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let pixels_buf_infos = [pixels_buf_info];

        let pixels = vk::WriteDescriptorSet::builder()
            .dst_set(self.slot_render_desc_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&pixels_buf_infos)
            .build();

        let descriptor_writes = [slots, pixels];

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };

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

        let descriptor_writes = [nodes, edges, masks];

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

            [nodes, edges, masks]
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
                .size(BinPushConstants::PC_RANGE)
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

        let pool_sizes = [nodes_pool_size, edges_pool_size, masks_pool_size];

        let bin_pipeline = ComputePipeline::new_with_pool_size(
            device,
            descriptor_set_layout,
            &pool_sizes,
            pipeline_layout,
            crate::include_shader!("edges/edge_binning.comp.spv"),
        )?;

        Ok(bin_pipeline)
    }

    fn create_preprocess_pipeline(device: &Device) -> Result<ComputePipeline> {
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

            let edge_beziers = vk::DescriptorSetLayoutBinding::builder()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            [nodes, edges, edge_beziers]
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
                .size(BinPushConstants::PC_RANGE)
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

        let bezier_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };

        let pool_sizes = [nodes_pool_size, edges_pool_size, bezier_pool_size];

        let pipeline = ComputePipeline::new_with_pool_size(
            device,
            descriptor_set_layout,
            &pool_sizes,
            pipeline_layout,
            crate::include_shader!("edges/edge_preprocess.comp.spv"),
        )?;

        Ok(pipeline)
    }

    fn create_populate_slot_pipeline(
        device: &Device,
    ) -> Result<ComputePipeline> {
        let bindings = {
            use vk::ShaderStageFlags as Stages;

            let beziers = vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            let slots = vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            [beziers, slots]
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

            /*
            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(Flags::COMPUTE)
                .offset(0)
                .size(BinPushConstants::PC_RANGE)
                .build();

            let pc_ranges = [pc_range];
            */

            let pc_ranges = [];

            let layouts = [descriptor_set_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe { device.create_pipeline_layout(&layout_info, None) }
        }?;

        let bezier_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };

        let slots_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };

        let pool_sizes = [bezier_pool_size, slots_pool_size];

        let pipeline = ComputePipeline::new_with_pool_size(
            device,
            descriptor_set_layout,
            &pool_sizes,
            pipeline_layout,
            crate::include_shader!("edges/edge_populate_slots.comp.spv"),
        )?;

        Ok(pipeline)
    }

    fn create_slot_render_pipeline(device: &Device) -> Result<ComputePipeline> {
        let bindings = {
            use vk::ShaderStageFlags as Stages;

            let slots = vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            let pixels = vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            [slots, pixels]
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

            /*
            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(Flags::COMPUTE)
                .offset(0)
                .size(BinPushConstants::PC_RANGE)
                .build();

            let pc_ranges = [pc_range];
            */

            let pc_ranges = [];

            let layouts = [descriptor_set_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe { device.create_pipeline_layout(&layout_info, None) }
        }?;

        let slots_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };

        let pixels_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };

        let pool_sizes = [slots_pool_size, pixels_pool_size];

        let pipeline = ComputePipeline::new_with_pool_size(
            device,
            descriptor_set_layout,
            &pool_sizes,
            pipeline_layout,
            crate::include_shader!("edges/edge_slot_render.comp.spv"),
        )?;

        Ok(pipeline)
    }

    pub fn write_render_descriptor_set(
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
            .dst_set(self.render_desc_set)
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
            .dst_set(self.render_desc_set)
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
            .dst_set(self.render_desc_set)
            .dst_binding(2)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&mask_buf_infos)
            .build();

        let pixels_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.pixels.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let pixels_buf_infos = [pixels_buf_info];

        let pixels = vk::WriteDescriptorSet::builder()
            .dst_set(self.render_desc_set)
            .dst_binding(3)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&pixels_buf_infos)
            .build();

        let descriptor_writes = [nodes, edges, masks, pixels];

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };

        Ok(())
    }

    fn create_render_pipeline(device: &Device) -> Result<ComputePipeline> {
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

            let pixels = vk::DescriptorSetLayoutBinding::builder()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            [nodes, edges, masks, pixels]
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
                .size(BinPushConstants::PC_RANGE)
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

        let pixels_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };

        let pool_sizes = [
            nodes_pool_size,
            edges_pool_size,
            masks_pool_size,
            pixels_pool_size,
        ];

        let bin_pipeline = ComputePipeline::new_with_pool_size(
            device,
            descriptor_set_layout,
            &pool_sizes,
            pipeline_layout,
            crate::include_shader!("edges/edge_render.comp.spv"),
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

pub struct TileSlots {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,

    rows: usize,
    columns: usize,
}

impl TileSlots {
    // pub fn new(app: &GfaestusVk, rows: usize, columns: usize, depth: usize) -> Result<Self> {
    pub fn new(app: &GfaestusVk, rows: usize, columns: usize) -> Result<Self> {
        let tile_count = rows * columns;

        /*
        // we use one bit per edge in the mask, and the masks are
        // `uint`s on the GPU, i.e. `u32`s in Rust
        let mut mask_depth = edge_count / 32;

        if edge_count % 32 != 0 {
            mask_depth += 1;
        }
        */

        let buffer_size = tile_count * 32 * std::mem::size_of::<u32>();

        let (buffer, memory, size) = {
            let size = buffer_size as vk::DeviceSize;

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
    pub fn new(
        app: &GfaestusVk,
        rows: usize,
        columns: usize,
        edge_count: usize,
    ) -> Result<Self> {
        let tile_count = rows * columns;

        // we use one bit per edge in the mask, and the masks are
        // `uint`s on the GPU, i.e. `u32`s in Rust
        let mut mask_depth = edge_count / 32;

        if edge_count % 32 != 0 {
            mask_depth += 1;
        }

        let buffer_size =
            (tile_count * mask_depth) * std::mem::size_of::<u32>();

        println!();
        println!("------------------------------------");
        println!("edge: {}", edge_count);
        println!("tiles: {}", tile_count);
        println!("mask depth: {}", mask_depth);
        println!("mask buffer size: {}", buffer_size);
        println!("------------------------------------");
        println!();

        let (buffer, memory, size) = {
            let size = buffer_size as vk::DeviceSize;
            // let size = ((tile_count * std::mem::size_of::<u32>()) as u32)
            //     as vk::DeviceSize;

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
            // three pairs of points to encode quadratic beziers
            let size = ((edge_count * 2 * 3 * std::mem::size_of::<f32>())
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

pub struct PixelBuffer {
    pub buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,

    pub width: usize,
    pub height: usize,
}

impl PixelBuffer {
    pub fn new(app: &GfaestusVk, width: usize, height: usize) -> Result<Self> {
        let size = ((width * height * std::mem::size_of::<u32>()) as u32)
            as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::TRANSFER_SRC
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::DEVICE_LOCAL;

        let (buffer, memory, size) =
            app.create_buffer(size, usage, mem_props)?;

        Ok(Self {
            buffer,
            memory,
            size,

            width,
            height,
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

pub struct BinPushConstants {
    view_transform: glm::Mat4,
    viewport_dims: [f32; 2],
    edge_count: u32,
}

impl BinPushConstants {
    pub const PC_RANGE: u32 = (std::mem::size_of::<f32>() * 19) as u32;

    #[inline]
    pub fn new(
        offset: [f32; 2],
        viewport_dims: [f32; 2],
        view: crate::view::View,
        edge_count: u32,
        // node_width: f32,
        // texture_period: u32,
    ) -> Self {
        use crate::view;

        let mut view = view;

        // let offset = Point {
        //     x: viewport_dims[0],
        //     y: viewport_dims[1],
        // } * 0.5;

        // view.center -= offset;

        let model_mat = glm::mat4(
            1.0,
            0.0,
            0.0,
            viewport_dims[0] * 0.5,
            0.0,
            1.0,
            0.0,
            viewport_dims[1] * 0.5,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        );

        let view_mat = view.to_scaled_matrix();

        let matrix = model_mat * view_mat;

        // let s = view.scale;

        // let matrix = glm::mat4(

        /*

        let view_mat = view.to_scaled_matrix();

        let width = viewport_dims[0];
        let height = viewport_dims[1];

        // let viewport_mat = view::viewport_scale(1.0 / width, 1.0 / height);
        // let viewport_mat = view::viewport_scale(width, height);
        // let viewport_mat = view::viewport_scale(1.0, 1.0);

        let matrix = view_mat * model_mat;
        // let matrix = viewport_mat * view_mat * model_mat;
        */

        Self {
            view_transform: matrix,
            // node_width,
            viewport_dims,
            edge_count,
            // scale: view.scale,
            // texture_period,
        }
    }

    #[inline]
    pub fn bytes(&self) -> [u8; Self::PC_RANGE as usize] {
        use crate::view;

        let mut bytes = [0u8; 76];

        let view_transform_array = view::mat4_to_array(&self.view_transform);

        let mut offset = 0;

        {
            let mut add_float = |f: f32| {
                let f_bytes = f.to_ne_bytes();
                for i in 0..4 {
                    bytes[offset] = f_bytes[i];
                    offset += 1;
                }
            };

            for i in 0..4 {
                let row = view_transform_array[i];
                for j in 0..4 {
                    let val = row[j];
                    add_float(val);
                }
            }

            add_float(self.viewport_dims[0]);
            add_float(self.viewport_dims[1]);
        }

        let ec_bytes = self.edge_count.to_ne_bytes();
        for i in 0..4 {
            bytes[offset] = ec_bytes[i];
            offset += 1;
        }

        bytes
    }
}
