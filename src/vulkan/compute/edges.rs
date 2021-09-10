use crate::{view::View, vulkan::draw_system::edges::EdgeIndices};

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;
use handlegraph::handle::Handle;

use crate::vulkan::{
    draw_system::{edges::PreprocessPushConstants, nodes::NodeVertices},
    GfaestusVk,
};

use super::ComputePipeline;

pub struct EdgePreprocess {
    preprocess_pipeline: ComputePipeline,
    preprocess_desc_set: vk::DescriptorSet,

    pub edges: EdgeBuffers,

    pub edges2: EdgeBuffers2,
}

impl EdgePreprocess {
    pub fn new(app: &GfaestusVk, edge_count: usize) -> Result<Self> {
        let device = app.vk_context().device();

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

        let edges = EdgeBuffers::new(app, edge_count)?;

        let edges2 = EdgeBuffers2::new(app, edge_count)?;

        Ok(Self {
            preprocess_pipeline,
            preprocess_desc_set,

            edges,
            edges2,
        })
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

    pub fn preprocess_memory_barrier(
        &self,
        cmd_buf: vk::CommandBuffer,
    ) -> Result<()> {
        let device = &self.preprocess_pipeline.device;

        let curve_barrier = vk::MemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .build();

        let memory_barriers = [curve_barrier];
        let buffer_memory_barriers = [];
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

    pub fn preprocess_cmd(
        &self,
        cmd_buf: vk::CommandBuffer,
        _edges: &EdgeIndices,
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

        let push_constants = PreprocessPushConstants::new(
            self.edges.edge_count,
            viewport_dims,
            view,
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
            let block_size = 1024;

            let mut size = self.edges.edge_count / block_size;
            if self.edges.edge_count % block_size != 0 {
                size += 1;
            }
            size as u32
        };
        let y_group_count: u32 = 1;
        let z_group_count: u32 = 1;

        // println!("edge preprocessing");
        // println!("  x_group_count: {}", x_group_count);
        // println!("  y_group_count: {}", y_group_count);
        // println!("  z_group_count: {}", z_group_count);

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

        let curve_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.edges.edges_pos_buf)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let curve_buf_infos = [curve_buf_info];

        let curves = vk::WriteDescriptorSet::builder()
            .dst_set(self.preprocess_desc_set)
            .dst_binding(2)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&curve_buf_infos)
            .build();

        let descriptor_writes = [nodes, edges, curves];

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };

        Ok(())
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

            let edge_curves = vk::DescriptorSetLayoutBinding::builder()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            let edge_count = vk::DescriptorSetLayoutBinding::builder()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build();

            [nodes, edges, edge_curves, edge_count]
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
                .size(PreprocessPushConstants::PC_RANGE)
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

        let curve_pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };

        let pool_sizes = [nodes_pool_size, edges_pool_size, curve_pool_size];

        let pipeline = ComputePipeline::new_with_pool_size(
            device,
            descriptor_set_layout,
            &pool_sizes,
            pipeline_layout,
            crate::include_shader!("edges/edge_preprocess.comp.spv"),
        )?;

        Ok(pipeline)
    }
}

#[allow(dead_code)]
pub struct EdgeBuffers {
    edges_by_id_buf: vk::Buffer,
    edges_by_id_mem: vk::DeviceMemory,
    edges_by_id_size: vk::DeviceSize,

    pub(crate) edges_pos_buf: vk::Buffer,
    pub(crate) edges_pos_mem: vk::DeviceMemory,
    pub(crate) edges_pos_size: vk::DeviceSize,

    edge_count_buf: vk::Buffer,
    edge_count_alloc: vk_mem::Allocation,
    edge_count_alloc_info: vk_mem::AllocationInfo,

    edge_count: usize,
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
            let size = ((edge_count * 2 * 2 * std::mem::size_of::<f32>()
                + std::mem::size_of::<u32>()) as u32)
                as vk::DeviceSize;

            let usage = vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::STORAGE_BUFFER;

            let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_CACHED
                | vk::MemoryPropertyFlags::HOST_COHERENT;

            app.create_buffer(size, usage, mem_props)
        }?;

        let edge_count_data: [u8; 4] = [0u8; 4];

        let edge_count_usage = vk::BufferUsageFlags::STORAGE_BUFFER;

        let edge_count_mem_usage = vk_mem::MemoryUsage::CpuOnly;

        let (edge_count_buf, edge_count_alloc, edge_count_alloc_info) = app
            // .create_buffer_with_data::<u32, _>(
            .create_buffer_with_data(
                edge_count_usage,
                edge_count_mem_usage,
                false,
                &edge_count_data,
            )?;

        Ok(Self {
            edges_by_id_buf,
            edges_by_id_mem,
            edges_by_id_size,

            edges_pos_buf,
            edges_pos_mem,
            edges_pos_size,

            edge_count_buf,
            edge_count_alloc,
            edge_count_alloc_info,

            edge_count,
        })
    }
}

#[allow(dead_code)]
pub struct EdgeBuffers2 {
    edges_output_buf: vk::Buffer,
    edges_output_alloc: vk_mem::Allocation,
    edges_output_alloc_info: vk_mem::AllocationInfo,

    edge_count_buf: vk::Buffer,
    edge_count_alloc: vk_mem::Allocation,
    edge_count_alloc_info: vk_mem::AllocationInfo,
}

impl EdgeBuffers2 {
    fn new(app: &GfaestusVk, edge_count: usize) -> Result<Self> {
        let edges_data = vec![0u32; edge_count * 2];

        let usage = vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::INDEX_BUFFER;

        let memory_usage = vk_mem::MemoryUsage::GpuOnly;

        let (edges_output_buf, edges_output_alloc, edges_output_alloc_info) =
            // app.create_buffer_with_data::<u32, _>(
            app.create_buffer_with_data(
                usage,
                memory_usage,
                false,
                &edges_data,
            )?;

        let usage = vk::BufferUsageFlags::UNIFORM_BUFFER;

        let memory_usage = vk_mem::MemoryUsage::CpuOnly;

        let edge_count_data = [0];

        let (edge_count_buf, edge_count_alloc, edge_count_alloc_info) = app
            // .create_buffer_with_data::<u32, _>(
            .create_buffer_with_data(
                usage,
                memory_usage,
                true,
                &edge_count_data,
            )?;

        Ok(Self {
            edges_output_buf,
            edges_output_alloc,
            edges_output_alloc_info,
            edge_count_buf,
            edge_count_alloc,
            edge_count_alloc_info,
        })
    }
}
