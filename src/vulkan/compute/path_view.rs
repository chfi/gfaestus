use crate::geometry::{Point, Rect};
use crate::reactor::Reactor;
use crate::vulkan::texture::Texture;

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;

use handlegraph::handle::Handle;
use handlegraph::pathhandlegraph::PathId;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use crate::app::selection::SelectionBuffer;

use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

use super::{ComputeManager, ComputePipeline};

pub struct PathViewRenderer {
    pipeline: ComputePipeline,
    descriptor_set_layout: vk::DescriptorSetLayout,

    descriptor_pool: vk::DescriptorPool,
    buffer_desc_set: vk::DescriptorSet,
    // path_desc_set: vk::DescriptorSet,
    // output_desc_set: vk::DescriptorSet,
    pub width: usize,
    pub height: usize,

    path_data: Vec<u32>,

    path_buffer: vk::Buffer,
    path_allocation: vk_mem::Allocation,
    path_allocation_info: vk_mem::AllocationInfo,

    pub output_image: Texture,
    // path_allocation_info: Option<vk_mem::AllocationInfo>,
    // output_buffer: vk::Buffer,
    // output_allocation: vk_mem::Allocation,
    // output_allocation_info: vk_mem::AllocationInfo,
    // path_buffer:
}

impl PathViewRenderer {
    pub fn new(
        app: &GfaestusVk,
        overlay_desc_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let width = 2048;
        let height = 64;
        let size = width * height;

        let device = app.vk_context().device();

        dbg!();

        let (path_buffer, path_allocation, path_allocation_info) = {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST;
            // | vk::BufferUsageFlags::TRANSFER_SRC;
            let memory_usage = vk_mem::MemoryUsage::CpuToGpu;

            let data = vec![0u32; size];

            let (buffer, allocation, allocation_info) =
                app.create_buffer_with_data(usage, memory_usage, true, &data)?;

            app.set_debug_object_name(
                buffer,
                "Path View Renderer (Path Buffer)",
            )?;

            (buffer, allocation, allocation_info)
        };

        dbg!();

        let output_image = {
            let format = vk::Format::R8G8B8A8_UNORM;

            let texture = Texture::allocate(
                app,
                app.transient_command_pool,
                app.graphics_queue,
                width,
                height,
                format,
                vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED,
            )?;

            texture
        };

        dbg!();

        let descriptor_pool = {
            let buffer_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                // ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 1,
            };

            let image_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                // ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 1,
            };

            let pool_sizes = [buffer_size, image_size];

            let pool_info = vk::DescriptorPoolCreateInfo::builder()
                .pool_sizes(&pool_sizes)
                .max_sets(2)
                .build();

            unsafe { device.create_descriptor_pool(&pool_info, None) }
        }?;

        dbg!();

        let descriptor_set_layout = Self::create_descriptor_set_layout(device)?;

        let descriptor_sets = {
            let layouts = vec![descriptor_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        dbg!();

        let buffer_desc_set = descriptor_sets[0];

        {
            let path_buf_info = vk::DescriptorBufferInfo::builder()
                .buffer(path_buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)
                .build();

            let path_buf_infos = [path_buf_info];

            let path_write = vk::WriteDescriptorSet::builder()
                .dst_set(buffer_desc_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&path_buf_infos)
                .build();

            let output_img_info = vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::GENERAL)
                .image_view(output_image.view)
                // .sampler(sampler)
                .build();
            let image_infos = [output_img_info];

            let output_write = vk::WriteDescriptorSet::builder()
                .dst_set(buffer_desc_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&image_infos)
                .build();

            let desc_writes = [path_write, output_write];

            unsafe { device.update_descriptor_sets(&desc_writes, &[]) };
        }

        dbg!();

        let pipeline_layout = {
            use vk::ShaderStageFlags as Flags;

            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(Flags::COMPUTE)
                .offset(0)
                .size(16)
                .build();

            let pc_ranges = [pc_range];
            // let pc_ranges = [];

            let layouts = [descriptor_set_layout, overlay_desc_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe { device.create_pipeline_layout(&layout_info, None) }
        }?;

        dbg!();

        let pipeline = ComputePipeline::new(
            device,
            descriptor_set_layout,
            pipeline_layout,
            crate::include_shader!("compute/path_view.comp.spv"),
        )?;

        dbg!();

        Ok(Self {
            pipeline,
            descriptor_set_layout,

            descriptor_pool,
            buffer_desc_set,

            width,
            height,

            path_data: Vec::with_capacity(width * height),

            path_buffer,
            path_allocation,
            path_allocation_info,

            output_image,
            // output_buffer,
            // output_allocation,
            // output_allocation_info,
        })
    }

    pub fn load_paths(
        &mut self,
        app: &GfaestusVk,
        reactor: &mut Reactor,
        paths: impl IntoIterator<Item = PathId>,
    ) -> Result<()> {
        self.path_data.clear();

        // TODO for now hardcoded to max 64 paths
        for path in paths.into_iter().take(64) {
            let steps = reactor.graph_query.path_pos_steps(path).unwrap();
            let (_, _, path_len) = steps.last().unwrap();

            for x in 0..self.width {
                let n = (x as f64) / (self.width as f64);

                let p = (n * (*path_len as f64)) as usize;

                let ix = match steps.binary_search_by_key(&p, |(_, _, p)| *p) {
                    Ok(i) => i,
                    Err(i) => i,
                };

                let ix = ix.min(steps.len() - 1);

                let (handle, _step, _pos) = steps[ix];

                self.path_data.push(handle.id().0 as u32);
            }
        }

        app.copy_data_to_buffer::<u32, u32>(&self.path_data, self.path_buffer)?;

        Ok(())
    }

    pub fn get_handle_at(&self, x: usize, y: usize) -> Option<Handle> {
        let ix = y * self.width + x;

        let raw = self.path_data.get(ix)?;
        let handle = Handle::from_integer(*raw as u64);

        Some(handle)
    }

    pub fn dispatch_managed(
        &self,
        comp_manager: &mut ComputeManager,
        app: &GfaestusVk,
        overlay_desc: vk::DescriptorSet,
        path_count: usize,
    ) -> Result<usize> {
        let fence_id = comp_manager.dispatch_with(|_device, cmd_buf| {
            self.dispatch_cmd(cmd_buf, app, overlay_desc, path_count)
                .unwrap();
        })?;

        Ok(fence_id)
    }

    pub fn dispatch_cmd(
        &self,
        cmd_buf: vk::CommandBuffer,
        app: &GfaestusVk,
        overlay_desc: vk::DescriptorSet,
        path_count: usize,
    ) -> Result<()> {
        log::warn!("in dispatch()");
        let device = app.vk_context().device();

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline.pipeline,
            );

            let desc_sets = [self.buffer_desc_set, overlay_desc];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline.pipeline_layout,
                0,
                &desc_sets[0..=1],
                &null,
            );

            let push_constants = [
                path_count as u32,
                self.width as u32,
                self.height as u32,
                0u32,
            ];

            let pc_bytes = bytemuck::cast_slice(&push_constants);

            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.pipeline.pipeline_layout,
                Flags::COMPUTE,
                0,
                pc_bytes,
            )
        };

        let x_group_count = self.width / 256;
        // let y_group_count = path_count;
        let y_group_count = 64;
        let z_group_count = 1;

        unsafe {
            device.cmd_dispatch(
                cmd_buf,
                x_group_count as u32,
                y_group_count as u32,
                z_group_count as u32,
            )
        };

        Ok(())
    }

    fn layout_binding() -> [vk::DescriptorSetLayoutBinding; 2] {
        use vk::ShaderStageFlags as Stages;

        let path_buffer = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build();

        let output_image = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build();

        // let output_buffer = vk::DescriptorSetLayoutBinding::builder()
        //     .binding(1)
        //     .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        //     .descriptor_count(1)
        //     .stage_flags(Stages::COMPUTE)
        //     .build();

        // let overlay_sampler = vk::DescriptorSetLayoutBinding::builder()
        //     .binding(2)
        //     .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        //     .descriptor_count(1)
        //     .stage_flags(Stages::COMPUTE)
        //     .build();

        // [path_buffer, output_buffer, overlay_sampler]
        [path_buffer, output_image]
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
