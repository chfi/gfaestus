use crate::geometry::{Point, Rect};
use ash::{
    extensions::{
        ext::DebugReport,
        khr::{Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk::SurfaceKHR,
};
use ash::{vk, Device, Entry};

use std::{collections::HashMap, ffi::CString, ops::RangeInclusive};

use anyhow::Result;

use crate::app::node_flags::SelectionBuffer;

use super::{draw_system::nodes::NodeVertices, GfaestusVk};

pub struct ComputeManager {
    pub(super) compute_cmd_pool: vk::CommandPool,

    compute_queue: vk::Queue,
    compute_queue_ix: u32,

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
            compute_queue_ix: queue_ix,

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
        let status =
            unsafe { self.device.wait_for_fences(&fences, true, 100_000_000) }?;

        Ok(())
    }

    pub fn free_fence(&mut self, fence_id: usize, block: bool) -> Result<()> {
        let fence = *self.fences.get(&fence_id).unwrap();

        if block {
            let fences = [fence];
            let status =
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

pub struct NodeTranslation {
    compute_pipeline: ComputePipeline,

    descriptor_set: vk::DescriptorSet,

    node_count: usize,
}

impl NodeTranslation {
    pub fn new(app: &GfaestusVk, node_count: usize) -> Result<Self> {
        let device = app.vk_context().device();

        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let pipeline_layout = {
            use vk::ShaderStageFlags as Flags;

            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(Flags::COMPUTE)
                .offset(0)
                .size(8)
                .build();

            let pc_ranges = [pc_range];

            let layouts = [desc_set_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe { device.create_pipeline_layout(&layout_info, None) }
        }?;

        let compute_pipeline = ComputePipeline::new(
            device,
            desc_set_layout,
            pipeline_layout,
            include_bytes!("../../shaders/rect_select.comp.spv"),
        )?;

        let descriptor_sets = {
            let layouts = vec![desc_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(compute_pipeline.descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        // let selection_buffer = SelectionBuffer::new(app, node_count)?;

        Ok(Self {
            compute_pipeline,

            descriptor_set: descriptor_sets[0],
            // selection_buffer,
            node_count,
        })
    }

    pub fn translate_nodes(
        &self,
        comp_manager: &mut ComputeManager,
        vertices: &NodeVertices,
        selection_buffer: &SelectionBuffer,
        delta: Point,
    ) -> Result<usize> {
        self.write_descriptor_set(selection_buffer, vertices);

        let fence_id = comp_manager.dispatch_with(|_device, cmd_buf| {
            self.translate_cmd(cmd_buf, delta).unwrap();
        })?;

        Ok(fence_id)
    }

    pub fn translate_cmd(
        &self,
        cmd_buf: vk::CommandBuffer,
        delta: Point,
    ) -> Result<()> {
        let device = &self.compute_pipeline.device;

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.compute_pipeline.pipeline,
            )
        };

        unsafe {
            let desc_sets = [self.descriptor_set];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.compute_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=0],
                &null,
            );
        };

        let push_constants = DeltaPushConstants::new(delta);
        let pc_bytes = push_constants.bytes();

        unsafe {
            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.compute_pipeline.pipeline_layout,
                Flags::COMPUTE,
                0,
                &pc_bytes,
            )
        };

        let x_group_count = {
            let div = self.node_count / 64;
            let rem = self.node_count % 64;

            let mut count = div;
            if rem > 0 {
                count += 1;
            }
            count as u32
        };

        println!("dispatch with x_group_count {}", x_group_count);

        unsafe { device.cmd_dispatch(cmd_buf, x_group_count, 1, 1) };
        // unsafe { device.cmd_dispatch(cmd_buf, 64, 1, 1) };

        Ok(())
    }

    pub fn write_descriptor_set(
        &self,
        selection_buffer: &SelectionBuffer,
        vertices: &NodeVertices,
    ) {
        let sel_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(selection_buffer.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let sel_buf_infos = [sel_buf_info];

        let sel_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&sel_buf_infos)
            .build();

        let node_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(vertices.buffer())
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let node_buf_infos = [node_buf_info];

        let node_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&node_buf_infos)
            .build();

        let desc_writes = [sel_write, node_write];

        unsafe {
            self.compute_pipeline
                .device
                .update_descriptor_sets(&desc_writes, &[])
        };
    }

    fn layout_binding() -> [vk::DescriptorSetLayoutBinding; 2] {
        use vk::ShaderStageFlags as Stages;

        let selection = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build();

        let node_vertices = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build();

        [selection, node_vertices]
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let bindings = Self::layout_binding();

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout =
            unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        Ok(layout)
    }
}

// pub struct NodeTranslatePipeline {
//     pub(super) descriptor_pool: vk::DescriptorPool,
//     pub(super) descriptor_set_layout: vk::DescriptorSetLayout,

//     pub(super) vertices_set: vk::DescriptorSet,

//     pub(super) pipeline_layout: vk::PipelineLayout,
//     pub(super) pipeline: vk::Pipeline,

//     pub(super) device: Device,

//     pub fences: HashMap<usize, vk::Fence>,
//     command_buffers: HashMap<usize, vk::CommandBuffer>,

//     next_fence: usize,
// }

pub struct GpuSelection {
    compute_pipeline: ComputePipeline,

    descriptor_set: vk::DescriptorSet,

    pub selection_buffer: SelectionBuffer,

    node_count: usize,
}

impl GpuSelection {
    pub fn new(app: &GfaestusVk, node_count: usize) -> Result<Self> {
        println!("node count {}", node_count);

        let device = app.vk_context().device();

        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let pipeline_layout = {
            use vk::ShaderStageFlags as Flags;

            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(Flags::COMPUTE)
                .offset(0)
                .size(20)
                .build();

            let pc_ranges = [pc_range];

            let layouts = [desc_set_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe { device.create_pipeline_layout(&layout_info, None) }
        }?;

        let compute_pipeline = ComputePipeline::new(
            device,
            desc_set_layout,
            pipeline_layout,
            include_bytes!("../../shaders/rect_select.comp.spv"),
        )?;

        let descriptor_sets = {
            let layouts = vec![desc_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(compute_pipeline.descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        let selection_buffer = SelectionBuffer::new(app, node_count)?;

        Ok(Self {
            compute_pipeline,

            descriptor_set: descriptor_sets[0],

            selection_buffer,

            node_count,
        })
    }

    pub fn rectangle_select(
        &self,
        comp_manager: &mut ComputeManager,
        vertices: &NodeVertices,
        rect: Rect,
        // returns fence ID
    ) -> Result<usize> {
        self.write_descriptor_set(vertices);

        let fence_id = comp_manager.dispatch_with(|_device, cmd_buf| {
            self.rectangle_select_cmd(cmd_buf, rect).unwrap();
        })?;

        Ok(fence_id)
    }

    fn rectangle_select_cmd(
        &self,
        // comp_manager: &mut ComputeManager,
        // vertices: &NodeVertices,
        cmd_buf: vk::CommandBuffer,
        rect: Rect,
    ) -> Result<()> {
        let device = &self.compute_pipeline.device;

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.compute_pipeline.pipeline,
            )
        };

        unsafe {
            let desc_sets = [self.descriptor_set];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.compute_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=0],
                &null,
            );
        };

        let push_constants = RectPushConstants::new(
            self.node_count as u32,
            rect.min(),
            rect.max(),
        );
        let pc_bytes = push_constants.bytes();

        unsafe {
            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.compute_pipeline.pipeline_layout,
                Flags::COMPUTE,
                0,
                &pc_bytes,
            )
        };

        let x_group_count = {
            let div = self.node_count / 64;
            let rem = self.node_count % 64;

            let mut count = div;
            if rem > 0 {
                count += 1;
            }
            count as u32
        };

        println!("dispatch with x_group_count {}", x_group_count);

        unsafe { device.cmd_dispatch(cmd_buf, x_group_count, 1, 1) };
        // unsafe { device.cmd_dispatch(cmd_buf, 64, 1, 1) };

        Ok(())
    }

    pub fn write_descriptor_set(&self, vertices: &NodeVertices) {
        let sel_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.selection_buffer.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let sel_buf_infos = [sel_buf_info];

        let sel_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&sel_buf_infos)
            .build();

        let node_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(vertices.buffer())
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let node_buf_infos = [node_buf_info];

        let node_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&node_buf_infos)
            .build();

        let desc_writes = [sel_write, node_write];

        unsafe {
            self.compute_pipeline
                .device
                .update_descriptor_sets(&desc_writes, &[])
        };
    }

    fn layout_binding() -> [vk::DescriptorSetLayoutBinding; 2] {
        use vk::ShaderStageFlags as Stages;

        let selection = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build();

        let node_vertices = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build();

        [selection, node_vertices]
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let bindings = Self::layout_binding();

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout =
            unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        Ok(layout)
    }
}

// struct

// descriptor set:
// vertex buffer
// selection buffer

pub struct ComputePipeline {
    pub(super) descriptor_pool: vk::DescriptorPool,
    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,

    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,

    pub(super) device: Device,
}

impl ComputePipeline {
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

// impl NodeTranslatePipeline {
//     pub fn new(device: &Device) -> Result<Self> {
//         let desc_set_layout = Self::create_descriptor_set_layout(device)?;

//         let (pipeline, pipeline_layout) =
//             Self::create_pipeline(device, desc_set_layout);

//         let descriptor_pool = {
//             let pool_size = vk::DescriptorPoolSize {
//                 ty: vk::DescriptorType::STORAGE_BUFFER,
//                 descriptor_count: 1,
//             };

//             let pool_sizes = [pool_size];

//             let pool_info = vk::DescriptorPoolCreateInfo::builder()
//                 .pool_sizes(&pool_sizes)
//                 .max_sets(1)
//                 .build();

//             unsafe { device.create_descriptor_pool(&pool_info, None) }
//         }?;

//         let descriptor_sets = {
//             let layouts = vec![desc_set_layout];

//             let alloc_info = vk::DescriptorSetAllocateInfo::builder()
//                 .descriptor_pool(descriptor_pool)
//                 .set_layouts(&layouts)
//                 .build();

//             unsafe { device.allocate_descriptor_sets(&alloc_info) }
//         }?;

//         Ok(Self {
//             descriptor_pool,
//             descriptor_set_layout: desc_set_layout,

//             vertices_set: descriptor_sets[0],

//             pipeline_layout,
//             pipeline,

//             device: device.clone(),

//             command_buffers: HashMap::default(),
//             fences: HashMap::default(),
//             next_fence: 0,
//         })
//     }

//     pub fn is_fence_ready(&self, fence_id: usize) -> Result<bool> {
//         let fence = *self.fences.get(&fence_id).unwrap();
//         let status = unsafe { self.device.get_fence_status(fence) }?;

//         Ok(status)
//     }

//     pub fn block_on_fence(&self, fence_id: usize) -> Result<()> {
//         let fence = *self.fences.get(&fence_id).unwrap();
//         let fences = [fence];
//         let status =
//             unsafe { self.device.wait_for_fences(&fences, true, 100_000_000) }?;

//         Ok(())
//     }

//     pub fn free_fence(
//         &mut self,
//         command_pool: vk::CommandPool,
//         fence_id: usize,
//         block: bool,
//     ) -> Result<()> {
//         let fence = *self.fences.get(&fence_id).unwrap();

//         if block {
//             let fences = [fence];
//             let status =
//                 unsafe { self.device.wait_for_fences(&fences, true, 0) }?;
//         }

//         let cmd_buf = *self.command_buffers.get(&fence_id).unwrap();

//         unsafe {
//             let cmd_bufs = [cmd_buf];
//             self.device.free_command_buffers(command_pool, &cmd_bufs);
//             self.device.destroy_fence(fence, None);
//         }

//         Ok(())
//     }

//     pub fn dispatch(
//         &mut self,
//         queue: vk::Queue,
//         cmd_pool: vk::CommandPool,
//         vertices: &NodeVertices,
//     ) -> Result<usize> {
//         let fence = {
//             let fence_info = vk::FenceCreateInfo::builder()
//                 .flags(vk::FenceCreateFlags::SIGNALED)
//                 .build();
//             unsafe { self.device.create_fence(&fence_info, None).unwrap() }
//         };

//         // let buffer_info = vk::DescriptorBufferInfo::
//         let buf_info = vk::DescriptorBufferInfo::builder()
//             .buffer(vertices.buffer())
//             .offset(0)
//             .range(vk::WHOLE_SIZE)
//             .build();

//         let buf_infos = [buf_info];

//         let desc_write = vk::WriteDescriptorSet::builder()
//             .dst_set(self.vertices_set)
//             .dst_binding(0)
//             .dst_array_element(0)
//             .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
//             .buffer_info(&buf_infos)
//             .build();

//         let desc_writes = [desc_write];

//         unsafe { self.device.update_descriptor_sets(&desc_writes, &[]) };

//         let device = &self.device;

//         let cmd_buf = {
//             let alloc_info = vk::CommandBufferAllocateInfo::builder()
//                 .level(vk::CommandBufferLevel::PRIMARY)
//                 .command_pool(cmd_pool)
//                 .command_buffer_count(1)
//                 .build();

//             let bufs = unsafe { device.allocate_command_buffers(&alloc_info) }?;
//             bufs[0]
//         };

//         {
//             let begin_info = vk::CommandBufferBeginInfo::builder()
//                 .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
//                 .build();

//             unsafe { device.begin_command_buffer(cmd_buf, &begin_info) }?;
//         }

//         unsafe {
//             device.cmd_bind_pipeline(
//                 cmd_buf,
//                 vk::PipelineBindPoint::COMPUTE,
//                 self.pipeline,
//             );

//             let desc_sets = [self.vertices_set];

//             let null = [];
//             device.cmd_bind_descriptor_sets(
//                 cmd_buf,
//                 vk::PipelineBindPoint::COMPUTE,
//                 self.pipeline_layout,
//                 0,
//                 &desc_sets[0..=0],
//                 &null,
//             );
//         };

//         unsafe { device.cmd_dispatch(cmd_buf, 1024, 1, 1) };

//         unsafe { device.end_command_buffer(cmd_buf) }?;

//         {
//             let submit_info = vk::SubmitInfo::builder()
//                 .command_buffers(&[cmd_buf])
//                 .build();

//             unsafe {
//                 device.queue_submit(queue, &[submit_info], fence)?;
//             }
//         }

//         self.fences.insert(self.next_fence, fence);
//         self.command_buffers.insert(self.next_fence, cmd_buf);

//         let fence_id = self.next_fence;

//         self.next_fence += 1;

//         Ok(fence_id)
//     }

//     fn create_descriptor_set_layout(
//         device: &Device,
//     ) -> Result<vk::DescriptorSetLayout> {
//         let binding = Self::layout_binding();
//         let bindings = [binding];

//         let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
//             .bindings(&bindings)
//             .build();

//         let layout =
//             unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

//         Ok(layout)
//     }

//     fn layout_binding() -> vk::DescriptorSetLayoutBinding {
//         use vk::ShaderStageFlags as Stages;

//         vk::DescriptorSetLayoutBinding::builder()
//             .binding(0)
//             .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
//             .descriptor_count(1)
//             .stage_flags(Stages::COMPUTE)
//             .build()
//     }

//     fn create_pipeline(
//         device: &Device,
//         descriptor_set_layout: vk::DescriptorSetLayout,
//     ) -> (vk::Pipeline, vk::PipelineLayout) {
//         let comp_src =
//             crate::load_shader!("../../shaders/node_translate.comp.spv");

//         let comp_module =
//             super::draw_system::create_shader_module(device, &comp_src);

//         let entry_point = CString::new("main").unwrap();

//         let comp_state_info = vk::PipelineShaderStageCreateInfo::builder()
//             .stage(vk::ShaderStageFlags::COMPUTE)
//             .module(comp_module)
//             .name(&entry_point)
//             .build();

//         let shader_state_infos = [comp_state_info];

//         let layout = {
//             use vk::ShaderStageFlags as Flags;

//             let pc_ranges = [];

//             let layouts = [descriptor_set_layout];

//             let layout_info = vk::PipelineLayoutCreateInfo::builder()
//                 .set_layouts(&layouts)
//                 .push_constant_ranges(&pc_ranges)
//                 .build();

//             unsafe {
//                 device.create_pipeline_layout(&layout_info, None).unwrap()
//             }
//         };

//         let pipeline_info = vk::ComputePipelineCreateInfo::builder()
//             .layout(layout)
//             .stage(comp_state_info)
//             .build();

//         let pipeline_infos = [pipeline_info];

//         let pipeline = unsafe {
//             device
//                 .create_compute_pipelines(
//                     vk::PipelineCache::null(),
//                     &pipeline_infos,
//                     None,
//                 )
//                 .unwrap()[0]
//         };

//         unsafe {
//             device.destroy_shader_module(comp_module, None);
//         }

//         (pipeline, layout)
//     }
// }

pub struct RectPushConstants {
    node_count: u32,
    rect: Rect,
}

impl RectPushConstants {
    #[inline]
    pub fn new(node_count: u32, p0: Point, p1: Point) -> Self {
        let rect = Rect::new(p0, p1);

        Self { node_count, rect }
    }

    #[inline]
    pub fn bytes(&self) -> [u8; 20] {
        let mut bytes = [0u8; 20];

        {
            let mut offset = 0;

            {
                let mut add_float = |f: f32| {
                    let f_bytes = f.to_ne_bytes();
                    for i in 0..4 {
                        bytes[offset] = f_bytes[i];
                        offset += 1;
                    }
                };
                add_float(self.rect.min().x);
                add_float(self.rect.min().y);

                add_float(self.rect.max().x);
                add_float(self.rect.max().y);
            }

            {
                let nc_bytes = self.node_count.to_ne_bytes();
                for i in 0..4 {
                    bytes[offset] = nc_bytes[i];
                    offset += 1;
                }
            }
        }

        bytes
    }
}

pub struct DeltaPushConstants {
    delta: Point,
}

impl DeltaPushConstants {
    #[inline]
    pub fn new(delta: Point) -> Self {
        Self { delta }
    }

    #[inline]
    pub fn bytes(&self) -> [u8; 8] {
        let mut bytes = [0u8; 8];

        {
            let mut offset = 0;

            {
                let mut add_float = |f: f32| {
                    let f_bytes = f.to_ne_bytes();
                    for i in 0..4 {
                        bytes[offset] = f_bytes[i];
                        offset += 1;
                    }
                };
                add_float(self.delta.x);
                add_float(self.delta.y);
            }
        }

        bytes
    }
}
