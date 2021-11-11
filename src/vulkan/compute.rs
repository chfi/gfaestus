use ash::version::DeviceV1_0;
use ash::{vk, Device};

use std::{collections::HashMap, ffi::CString};

use anyhow::Result;

use super::GfaestusVk;

pub mod edges;
pub mod node_motion;
pub mod path_view;
pub mod selection;

pub use edges::*;
pub use node_motion::*;
pub use selection::*;

pub struct ComputeManager {
    pub(super) compute_cmd_pool: vk::CommandPool,

    compute_queue: vk::Queue,

    fences: HashMap<usize, vk::Fence>,
    command_buffers: HashMap<usize, vk::CommandBuffer>,

    next_fence: usize,

    pub(super) device: Device,
}

impl ComputeManager {
    pub fn new(
        device: Device,
        queue_ix: u32,
        queue: vk::Queue,
    ) -> Result<Self> {
        let command_pool = GfaestusVk::create_command_pool(
            &device,
            queue_ix,
            vk::CommandPoolCreateFlags::empty(),
        )?;

        Ok(Self {
            compute_cmd_pool: command_pool,

            compute_queue: queue,

            fences: HashMap::default(),
            command_buffers: HashMap::default(),

            next_fence: 0,

            device,
        })
    }

    pub fn is_fence_ready(&self, fence_id: usize) -> Result<bool> {
        let fence = *self.fences.get(&fence_id).unwrap();
        let status = unsafe { self.device.get_fence_status(fence) }?;

        Ok(status)
    }

    pub fn block_on_fence(&self, fence_id: usize) -> Result<()> {
        let fence = *self.fences.get(&fence_id).unwrap();
        let fences = [fence];
        let _status =
            unsafe { self.device.wait_for_fences(&fences, true, 100_000_000) }?;

        Ok(())
    }

    pub fn free_fence(&mut self, fence_id: usize, block: bool) -> Result<()> {
        let fence = *self.fences.get(&fence_id).unwrap();

        if block {
            let fences = [fence];
            let _status =
                unsafe { self.device.wait_for_fences(&fences, true, 0) }?;
        }

        let cmd_buf = *self.command_buffers.get(&fence_id).unwrap();

        unsafe {
            let cmd_bufs = [cmd_buf];
            self.device
                .free_command_buffers(self.compute_cmd_pool, &cmd_bufs);
            self.device.destroy_fence(fence, None);
        }

        Ok(())
    }

    pub fn dispatch_with<F>(&mut self, commands: F) -> Result<usize>
    where
        F: FnOnce(&Device, vk::CommandBuffer),
    {
        let device = &self.device;

        let fence = {
            let fence_info = vk::FenceCreateInfo::builder()
                .flags(vk::FenceCreateFlags::SIGNALED)
                .build();
            unsafe { device.create_fence(&fence_info, None).unwrap() }
        };

        let fences = [fence];

        unsafe { device.reset_fences(&fences).unwrap() };

        let cmd_buf = {
            let alloc_info = vk::CommandBufferAllocateInfo::builder()
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_pool(self.compute_cmd_pool)
                .command_buffer_count(1)
                .build();

            let bufs = unsafe { device.allocate_command_buffers(&alloc_info) }?;
            bufs[0]
        };

        {
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                .build();

            unsafe { device.begin_command_buffer(cmd_buf, &begin_info) }?;
        }

        commands(device, cmd_buf);

        unsafe { device.end_command_buffer(cmd_buf) }?;

        {
            let submit_info = vk::SubmitInfo::builder()
                .command_buffers(&[cmd_buf])
                .build();

            unsafe {
                device.queue_submit(
                    self.compute_queue,
                    &[submit_info],
                    fence,
                )?;
            }
        }

        self.fences.insert(self.next_fence, fence);
        self.command_buffers.insert(self.next_fence, cmd_buf);

        let fence_id = self.next_fence;

        self.next_fence += 1;

        Ok(fence_id)
    }
}

pub struct ComputePipeline {
    pub(super) descriptor_pool: vk::DescriptorPool,
    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,

    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,

    pub(super) device: Device,
}

impl ComputePipeline {
    pub fn new_with_pool_size(
        device: &Device,
        descriptor_set_layout: vk::DescriptorSetLayout,
        pool_sizes: &[vk::DescriptorPoolSize],
        pipeline_layout: vk::PipelineLayout,
        shader: &[u8],
    ) -> Result<Self> {
        let pipeline = Self::create_pipeline(device, pipeline_layout, shader)?;

        let descriptor_pool = {
            // let pool_sizes = [pool_size];

            let pool_info = vk::DescriptorPoolCreateInfo::builder()
                .pool_sizes(pool_sizes)
                .max_sets(1)
                .build();

            unsafe { device.create_descriptor_pool(&pool_info, None) }
        }?;

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout,

            pipeline_layout,
            pipeline,

            device: device.clone(),
        })
    }

    pub fn new(
        device: &Device,
        descriptor_set_layout: vk::DescriptorSetLayout,
        pipeline_layout: vk::PipelineLayout,
        shader: &[u8],
    ) -> Result<Self> {
        let pipeline = Self::create_pipeline(device, pipeline_layout, shader)?;

        let descriptor_pool = {
            let pool_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 2,
            };

            let pool_sizes = [pool_size];

            let pool_info = vk::DescriptorPoolCreateInfo::builder()
                .pool_sizes(&pool_sizes)
                .max_sets(1)
                .build();

            unsafe { device.create_descriptor_pool(&pool_info, None) }
        }?;

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout,

            pipeline_layout,
            pipeline,

            device: device.clone(),
        })
    }

    pub(crate) fn create_pipeline(
        device: &Device,
        pipeline_layout: vk::PipelineLayout,
        shader: &[u8],
    ) -> Result<vk::Pipeline> {
        let comp_src = {
            let mut cursor = std::io::Cursor::new(shader);
            ash::util::read_spv(&mut cursor)
        }?;

        let comp_module =
            super::draw_system::create_shader_module(device, &comp_src);

        let entry_point = CString::new("main").unwrap();

        let comp_state_info = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(comp_module)
            .name(&entry_point)
            .build();

        let pipeline_info = vk::ComputePipelineCreateInfo::builder()
            .layout(pipeline_layout)
            .stage(comp_state_info)
            .build();

        let pipeline_infos = [pipeline_info];

        let pipeline = unsafe {
            device
                .create_compute_pipelines(
                    vk::PipelineCache::null(),
                    &pipeline_infos,
                    None,
                )
                .unwrap()[0]
        };

        unsafe {
            device.destroy_shader_module(comp_module, None);
        }

        Ok(pipeline)
    }
}
