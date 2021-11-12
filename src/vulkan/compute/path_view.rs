use crate::geometry::{Point, Rect};
use crate::reactor::Reactor;
use crate::vulkan::texture::Texture;

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;

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

            GfaestusVk::transition_image(
                device,
                app.transient_command_pool,
                app.graphics_queue,
                texture.image,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageLayout::GENERAL,
            )?;

            texture
        };

        /*
        let (output_buffer, output_allocation, output_allocation_info) = {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_SRC;

            let memory_usage = vk_mem::MemoryUsage::GpuToCpu;

            // let pixels = vec![[0u8; 4]; size];
            // let pixels = vec![[255u8; 4]; size];
            let pixels = vec![[255u8, 0, 0, 255]; size];

            let (buffer, allocation, allocation_info) = app
                .create_buffer_with_data(usage, memory_usage, true, &pixels)?;
            // .create_buffer_with_data(usage, memory_usage, false, &pixels)?;

            app.set_debug_object_name(
                buffer,
                "Path View Renderer (Output Buffer)",
            )?;

            (buffer, allocation, allocation_info)
        };
        */

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
        let mut node_buf: Vec<u32> =
            Vec::with_capacity(self.width * self.height);

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

                node_buf.push(handle.id().0 as u32);
            }
        }

        app.copy_data_to_buffer::<u32, u32>(&node_buf, self.path_buffer)?;

        Ok(())
    }

    pub fn dispatch(
        &self,
        app: &GfaestusVk,
        overlay_desc: vk::DescriptorSet,
        path_count: usize,
    ) -> Result<()> {
        let device = app.vk_context().device();

        // GfaestusVk::transition_image(
        //     device,
        //     app.transient_command_pool,
        //     app.graphics_queue,
        //     self.output_image.image,
        //     vk::ImageLayout::UNDEFINED,
        //     vk::ImageLayout::GENERAL,
        // )?;

        let cmd_buf = {
            let alloc_info = vk::CommandBufferAllocateInfo::builder()
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_pool(app.command_pool)
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

    /*
    pub fn copy_pixels(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();

        unsafe {
            let ptr = self.output_allocation_info.get_mapped_data();

            let pixels = std::slice::from_raw_parts(
                ptr as *const u8,
                self.width * self.height * 4,
            );

            out.extend_from_slice(pixels);
            // for color in pixels.chunks_exact(4) {
            //     if let [r, g, b, a] = color {
            //         out.push((*r as f32) / 255.0);
            //         out.push((*g as f32) / 255.0);
            //         out.push((*b as f32) / 255.0);
            //         out.push((*a as f32) / 255.0);
            //     }
            // }
        }

        Ok(out)
    }

    pub fn copy_output(&self) -> Result<Vec<rgb::RGBA<f32>>> {
        let mut out = Vec::new();

        unsafe {
            let ptr = self.output_allocation_info.get_mapped_data();

            let pixels =
                std::slice::from_raw_parts(ptr as *const u8, self.width * 4);

            for color in pixels.chunks_exact(4) {
                if let [r, g, b, a] = color {
                    let r = (*r as f32) / 255.0;
                    let g = (*g as f32) / 255.0;
                    let b = (*b as f32) / 255.0;
                    let a = (*a as f32) / 255.0;
                    out.push(rgb::RGBA::new(r, g, b, a));
                }
            }
        }

        Ok(out)
    }
    */

    fn layout_binding() -> [vk::DescriptorSetLayoutBinding; 2] {
        use vk::ShaderStageFlags as Stages;

        //

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
