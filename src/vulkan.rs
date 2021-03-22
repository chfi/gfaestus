pub mod context;
pub mod debug;
pub mod draw_system;
pub mod render_pass;
pub mod texture;

mod init;

use context::*;
use debug::*;
use draw_system::*;
use render_pass::*;
use texture::*;

use init::*;

use ash::{
    extensions::{
        ext::DebugReport,
        khr::{Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk::SurfaceKHR,
};
use ash::{vk, Device, Entry, Instance};

use winit::window::Window;

use std::{
    ffi::{CStr, CString},
    mem::{align_of, size_of},
    sync::Arc,
};

use anyhow::Result;

pub struct GfaestusVk {
    pub graphics_queue: vk::Queue,
    pub present_queue: vk::Queue,

    graphics_family_index: u32,
    present_family_index: u32,

    pub msaa_samples: vk::SampleCountFlags,
    pub render_pass: vk::RenderPass,
    transient_color: Texture,

    pub swapchain: Swapchain,
    pub swapchain_khr: vk::SwapchainKHR,
    pub swapchain_props: SwapchainProperties,

    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,
    swapchain_framebuffers: Vec<vk::Framebuffer>,

    command_pool: vk::CommandPool,
    transient_command_pool: vk::CommandPool,
    // command_buffers: Vec<vk::CommandBuffer>,
    in_flight_frames: InFlightFrames,

    pub descriptor_pool: Arc<vk::DescriptorPool>,

    vk_context: VkContext,
}

impl GfaestusVk {
    pub fn new(window: &Window) -> Result<Self> {
        let entry = Entry::new()?;
        let instance = create_instance(&entry, window)?;

        let surface = Surface::new(&entry, &instance);
        let surface_khr = unsafe {
            ash_window::create_surface(&entry, &instance, window, None)
        }?;

        let debug_report_callback =
            debug::setup_debug_messenger(&entry, &instance);

        let (physical_device, graphics_ix, present_ix) =
            choose_physical_device(&instance, &surface, surface_khr)?;

        let (device, graphics_queue, present_queue) = create_logical_device(
            &instance,
            physical_device,
            graphics_ix,
            present_ix,
        )?;

        let vk_context = VkContext::new(
            entry,
            instance,
            debug_report_callback,
            surface,
            surface_khr,
            physical_device,
            device,
        );

        let width = 800u32;
        let height = 600u32;

        let (swapchain, swapchain_khr, swapchain_props, images) =
            create_swapchain_and_images(
                &vk_context,
                graphics_ix,
                present_ix,
                [width, height],
            )?;
        let swapchain_image_views = create_swapchain_image_views(
            vk_context.device(),
            &images,
            swapchain_props,
        )?;

        let msaa_samples = vk_context.get_max_usable_sample_count();

        let render_pass = create_swapchain_render_pass(
            vk_context.device(),
            swapchain_props,
            msaa_samples,
        )?;

        let command_pool = Self::create_command_pool(
            vk_context.device(),
            graphics_ix,
            vk::CommandPoolCreateFlags::empty(),
        )?;
        let transient_command_pool = Self::create_command_pool(
            vk_context.device(),
            graphics_ix,
            vk::CommandPoolCreateFlags::TRANSIENT,
        )?;

        let transient_color = Texture::create_transient_color(
            &vk_context,
            transient_command_pool,
            graphics_queue,
            swapchain_props,
            msaa_samples,
        )?;

        let swapchain_framebuffers = create_swapchain_framebuffers(
            vk_context.device(),
            &swapchain_image_views,
            transient_color,
            render_pass,
            swapchain_props,
        );

        let in_flight_frames = Self::create_sync_objects(vk_context.device());

        let descriptor_pool = create_descriptor_pool(vk_context.device(), 1)?;

        Ok(Self {
            vk_context,

            graphics_queue,
            present_queue,

            graphics_family_index: graphics_ix,
            present_family_index: present_ix,

            msaa_samples,
            render_pass,
            transient_color,

            swapchain,
            swapchain_khr,
            swapchain_props,

            swapchain_images: images,
            swapchain_image_views,
            swapchain_framebuffers,

            command_pool,
            transient_command_pool,
            descriptor_pool: Arc::new(descriptor_pool),

            in_flight_frames,
        })
    }

    pub fn vk_context(&self) -> &VkContext {
        &self.vk_context
    }

    pub fn draw_frame_from<F>(&mut self, commands: F) -> Result<bool>
    where
        F: FnOnce(vk::CommandBuffer, vk::Framebuffer),
    {
        let sync_objects = self.in_flight_frames.next().unwrap();

        let img_available = sync_objects.image_available_semaphore;
        let render_finished = sync_objects.render_finished_semaphore;
        let in_flight_fence = sync_objects.fence;
        let wait_fences = [in_flight_fence];

        unsafe {
            self.vk_context.device().wait_for_fences(
                &wait_fences,
                true,
                std::u64::MAX,
            )
        }?;

        let result = unsafe {
            self.swapchain.acquire_next_image(
                self.swapchain_khr,
                std::u64::MAX,
                img_available,
                vk::Fence::null(),
            )
        };

        let img_index = match result {
            Ok((img_index, _)) => img_index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                return Ok(true);
            }
            Err(error) => panic!("Error while acquiring next image: {}", error),
        };

        unsafe { self.vk_context.device().reset_fences(&wait_fences) }?;

        // TODO update uniforms

        let device = self.vk_context.device();
        let wait_semaphores = [img_available];
        let signal_semaphores = [render_finished];

        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];

        let queue = self.graphics_queue;

        let framebuffer = self.swapchain_framebuffers[img_index as usize];

        self.execute_one_time_commands_semaphores(
            device,
            self.transient_command_pool,
            queue,
            &wait_semaphores,
            &wait_stages,
            &signal_semaphores,
            in_flight_fence,
            |cmd_buf| {
                commands(cmd_buf, framebuffer);
            },
        )?;

        let swapchains = [self.swapchain_khr];
        let img_indices = [img_index];

        {
            let present_info = vk::PresentInfoKHR::builder()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&img_indices)
                .build();

            let result = unsafe {
                self.swapchain
                    .queue_present(self.present_queue, &present_info)
            };

            match result {
                Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    return Ok(true);
                }
                Err(error) => panic!("Failed to present queue: {}", error),
                _ => {}
            }
        }

        unsafe {
            device.queue_wait_idle(queue)?;
        };

        Ok(false)
    }

    pub fn wait_gpu_idle(&self) -> Result<()> {
        let res = unsafe { self.vk_context.device().device_wait_idle() }?;
        Ok(res)
    }

    pub fn execute_one_time_commands_semaphores<F>(
        &self,
        device: &Device,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        wait_semaphores: &[vk::Semaphore],
        wait_stages: &[vk::PipelineStageFlags],
        signal_semaphores: &[vk::Semaphore],
        fence: vk::Fence,
        commands: F,
    ) -> Result<()>
    where
        F: FnOnce(vk::CommandBuffer),
    {
        let cmd_buf = {
            let alloc_info = vk::CommandBufferAllocateInfo::builder()
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_pool(command_pool)
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
            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.swapchain_props.extent.width as f32,
                height: self.swapchain_props.extent.height as f32,
                min_depth: 0.0,
                max_depth: 0.0,
            };

            let viewports = [viewport];

            device.cmd_set_viewport(cmd_buf, 0, &viewports);

            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.swapchain_props.extent,
            };
            let scissors = [scissor];

            device.cmd_set_scissor(cmd_buf, 0, &scissors);
        };

        commands(cmd_buf);

        unsafe { device.end_command_buffer(cmd_buf) }?;

        {
            let submit_info = vk::SubmitInfo::builder()
                .wait_semaphores(wait_semaphores)
                .wait_dst_stage_mask(wait_stages)
                .command_buffers(&[cmd_buf])
                .signal_semaphores(&signal_semaphores)
                .build();

            unsafe {
                device.queue_submit(queue, &[submit_info], fence)?;
                device.queue_wait_idle(queue)?;
            }
        }

        unsafe { device.free_command_buffers(command_pool, &[cmd_buf]) };

        Ok(())
    }

    pub fn execute_one_time_commands<F>(
        device: &Device,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        commands: F,
    ) -> Result<()>
    where
        F: FnOnce(vk::CommandBuffer),
    {
        let cmd_buf = {
            let alloc_info = vk::CommandBufferAllocateInfo::builder()
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_pool(command_pool)
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

        commands(cmd_buf);

        unsafe { device.end_command_buffer(cmd_buf) }?;

        {
            let submit_info = vk::SubmitInfo::builder()
                .command_buffers(&[cmd_buf])
                .build();

            unsafe {
                device.queue_submit(
                    queue,
                    &[submit_info],
                    vk::Fence::null(),
                )?;
                device.queue_wait_idle(queue)?;
            }
        }

        unsafe { device.free_command_buffers(command_pool, &[cmd_buf]) };

        Ok(())
    }

    pub fn transition_image(
        device: &Device,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        image: vk::Image,
        format: vk::Format,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) -> Result<()> {
        Self::execute_one_time_commands(
            device,
            command_pool,
            transition_queue,
            |buf| {
                let (src_access, dst_access, src_stage, dst_stage) =
                    match (old_layout, new_layout) {
                        (
                            vk::ImageLayout::UNDEFINED,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        ) => (
                            vk::AccessFlags::empty(),
                            vk::AccessFlags::TRANSFER_WRITE,
                            vk::PipelineStageFlags::TOP_OF_PIPE,
                            vk::PipelineStageFlags::TRANSFER,
                        ),
                        (
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        ) => (
                            vk::AccessFlags::TRANSFER_WRITE,
                            vk::AccessFlags::SHADER_READ,
                            vk::PipelineStageFlags::TRANSFER,
                            vk::PipelineStageFlags::FRAGMENT_SHADER,
                        ),
                        // (
                        //     vk::ImageLayout::UNDEFINED,
                        //     vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                        // ) => (
                        //     vk::AccessFlags::empty(),
                        //     vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                        //         | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                        //     vk::PipelineStageFlags::TOP_OF_PIPE,
                        //     vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                        // ),
                        (
                            vk::ImageLayout::UNDEFINED,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        ) => (
                            vk::AccessFlags::empty(),
                            vk::AccessFlags::COLOR_ATTACHMENT_READ
                                | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                            vk::PipelineStageFlags::TOP_OF_PIPE,
                            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                        ),
                        _ => panic!(
                            "Unsupported layout transition({:?} => {:?}).",
                            old_layout, new_layout
                        ),
                    };

                let aspect_mask = vk::ImageAspectFlags::COLOR;

                let barrier = vk::ImageMemoryBarrier::builder()
                    .old_layout(old_layout)
                    .new_layout(new_layout)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .src_access_mask(src_access)
                    .dst_access_mask(dst_access)
                    .build();

                unsafe {
                    device.cmd_pipeline_barrier(
                        buf,
                        src_stage,
                        dst_stage,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[barrier],
                    )
                };
            },
        )?;

        Ok(())
    }

    pub fn create_image(
        vk_context: &VkContext,
        mem_props: vk::MemoryPropertyFlags,
        extent: vk::Extent2D,
        sample_count: vk::SampleCountFlags,
        format: vk::Format,
        tiling: vk::ImageTiling,
        usage: vk::ImageUsageFlags,
    ) -> Result<(vk::Image, vk::DeviceMemory)> {
        let img_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            .tiling(tiling)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(sample_count)
            .flags(vk::ImageCreateFlags::empty())
            .build();

        let device = vk_context.device();

        let image = unsafe { device.create_image(&img_info, None) }?;
        let mem_reqs = unsafe { device.get_image_memory_requirements(image) };
        let mem_type_ix = find_memory_type(
            mem_reqs,
            vk_context.get_mem_properties(),
            mem_props,
        );

        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(mem_reqs.size)
            .memory_type_index(mem_type_ix)
            .build();

        let memory = unsafe {
            let mem = device.allocate_memory(&alloc_info, None)?;
            device.bind_image_memory(image, mem, 0)?;
            mem
        };

        Ok((image, memory))
    }

    pub fn create_image_view(
        device: &Device,
        image: vk::Image,
        mip_levels: u32,
        format: vk::Format,
        aspect_mask: vk::ImageAspectFlags,
    ) -> Result<vk::ImageView> {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask,
                base_mip_level: 0,
                level_count: mip_levels,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build();

        let img_view = unsafe { device.create_image_view(&create_info, None) }?;

        Ok(img_view)
    }

    pub fn copy_buffer(
        device: &Device,
        command_pool: vk::CommandPool,
        transfer_queue: vk::Queue,
        src: vk::Buffer,
        dst: vk::Buffer,
        size: vk::DeviceSize,
    ) {
        Self::execute_one_time_commands(
            &device,
            command_pool,
            transfer_queue,
            |buffer| {
                let region = vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size,
                };
                let regions = [region];

                unsafe { device.cmd_copy_buffer(buffer, src, dst, &regions) };
            },
        )
        .unwrap();
    }

    pub fn create_buffer(
        &self,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        mem_props: vk::MemoryPropertyFlags,
    ) -> Result<(vk::Buffer, vk::DeviceMemory, vk::DeviceSize)> {
        let vk_context = self.vk_context();
        let device = vk_context.device();

        let buffer = {
            let info = vk::BufferCreateInfo::builder()
                .size(size)
                .usage(usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .build();

            unsafe { device.create_buffer(&info, None) }
        }?;

        let mem_reqs = unsafe { device.get_buffer_memory_requirements(buffer) };

        let mem = {
            let mem_type = find_memory_type(
                mem_reqs,
                vk_context.get_mem_properties(),
                mem_props,
            );

            let info = vk::MemoryAllocateInfo::builder()
                .allocation_size(mem_reqs.size)
                .memory_type_index(mem_type)
                .build();

            unsafe { device.allocate_memory(&info, None) }
        }?;

        unsafe { device.bind_buffer_memory(buffer, mem, 0) }?;

        Ok((buffer, mem, mem_reqs.size))
    }

    pub fn create_vertex_buffer(
        &self,
        vertices: &[draw_system::Vertex],
    ) -> Result<(vk::Buffer, vk::DeviceMemory)> {
        use vk::BufferUsageFlags as Usage;
        let usage = Usage::VERTEX_BUFFER;

        let (buf, mem) = self
            .create_device_local_buffer_with_data::<u32, _>(usage, vertices)?;

        Ok((buf, mem))
    }

    pub fn create_device_local_buffer_with_data<A, T>(
        &self,
        // vk_context: &VkContext,
        // command_pool: vk::CommandPool,
        // transfer_queue: vk::Queue,
        usage: vk::BufferUsageFlags,
        data: &[T],
    ) -> Result<(vk::Buffer, vk::DeviceMemory)>
    where
        T: Copy,
    {
        use vk::BufferUsageFlags as Usage;
        use vk::MemoryPropertyFlags as MemPropFlags;

        let vk_context = &self.vk_context;
        let device = vk_context.device();
        let size = (data.len() * size_of::<T>()) as vk::DeviceSize;

        let (staging_buf, staging_mem, staging_mem_size) = self.create_buffer(
            size,
            Usage::TRANSFER_SRC,
            MemPropFlags::HOST_VISIBLE | MemPropFlags::HOST_COHERENT,
        )?;

        unsafe {
            let data_ptr = device.map_memory(
                staging_mem,
                0,
                size,
                vk::MemoryMapFlags::empty(),
            )?;

            let mut align = ash::util::Align::new(
                data_ptr,
                std::mem::align_of::<A>() as u64,
                staging_mem_size,
            );

            align.copy_from_slice(data);
            device.unmap_memory(staging_mem);
        }

        let (buffer, memory, _) = self.create_buffer(
            size,
            Usage::TRANSFER_DST | usage,
            MemPropFlags::DEVICE_LOCAL,
        )?;

        Self::copy_buffer(
            device,
            self.transient_command_pool,
            self.graphics_queue,
            staging_buf,
            buffer,
            size,
        );

        unsafe {
            device.destroy_buffer(staging_buf, None);
            device.free_memory(staging_mem, None);
        }

        Ok((buffer, memory))
    }

    pub fn recreate_swapchain(
        &mut self,
        dimensions: Option<[u32; 2]>,
    ) -> Result<()> {
        self.wait_gpu_idle()?;
        eprintln!("recreating swapchain");

        self.cleanup_swapchain();

        let device = self.vk_context.device();

        let dimensions = dimensions.unwrap_or([
            self.swapchain_props.extent.width,
            self.swapchain_props.extent.height,
        ]);

        let (swapchain, swapchain_khr, swapchain_props, images) =
            create_swapchain_and_images(
                &self.vk_context,
                self.graphics_family_index,
                self.present_family_index,
                dimensions,
            )?;

        let swapchain_image_views =
            create_swapchain_image_views(device, &images, swapchain_props)?;

        let render_pass = create_swapchain_render_pass(
            device,
            swapchain_props,
            self.msaa_samples,
        )?;

        let transient_color = Texture::create_transient_color(
            &self.vk_context,
            self.transient_command_pool,
            self.graphics_queue,
            swapchain_props,
            self.msaa_samples,
        )?;

        let swapchain_framebuffers = create_swapchain_framebuffers(
            device,
            &swapchain_image_views,
            transient_color,
            render_pass,
            swapchain_props,
        );

        // TODO recreate render pass, framebuffers, etc.

        self.swapchain = swapchain;
        self.swapchain_khr = swapchain_khr;
        self.swapchain_props = swapchain_props;

        self.swapchain_images = images;
        self.swapchain_image_views = swapchain_image_views;
        self.swapchain_framebuffers = swapchain_framebuffers;

        self.transient_color = transient_color;
        self.render_pass = render_pass;

        Ok(())
    }
}

impl GfaestusVk {
    fn create_sync_objects(device: &Device) -> InFlightFrames {
        let mut sync_objects_vec = Vec::new();

        // for _ in 0..MAX_FRAMES_IN_FLIGHT {
        for _ in 0..2 {
            let image_available_semaphore = {
                let semaphore_info = vk::SemaphoreCreateInfo::builder().build();
                unsafe {
                    device.create_semaphore(&semaphore_info, None).unwrap()
                }
            };

            let render_finished_semaphore = {
                let semaphore_info = vk::SemaphoreCreateInfo::builder().build();
                unsafe {
                    device.create_semaphore(&semaphore_info, None).unwrap()
                }
            };

            let in_flight_fence = {
                let fence_info = vk::FenceCreateInfo::builder()
                    .flags(vk::FenceCreateFlags::SIGNALED)
                    .build();
                unsafe { device.create_fence(&fence_info, None).unwrap() }
            };

            let sync_objects = SyncObjects {
                image_available_semaphore,
                render_finished_semaphore,
                fence: in_flight_fence,
            };
            sync_objects_vec.push(sync_objects)
        }

        InFlightFrames::new(sync_objects_vec)
    }

    fn cleanup_swapchain(&mut self) {
        let device = self.vk_context.device();

        unsafe {
            // TODO handle framebuffers, pipelines, etc.
            self.transient_color.destroy(device);

            self.swapchain_framebuffers
                .iter()
                .for_each(|f| device.destroy_framebuffer(*f, None));
            device.destroy_render_pass(self.render_pass, None);

            self.swapchain_image_views
                .iter()
                .for_each(|v| device.destroy_image_view(*v, None));

            self.swapchain.destroy_swapchain(self.swapchain_khr, None);
        }
    }

    fn create_command_pool(
        device: &Device,
        graphics_ix: u32,
        create_flags: vk::CommandPoolCreateFlags,
    ) -> Result<vk::CommandPool> {
        let command_pool_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(graphics_ix)
            .flags(create_flags)
            .build();

        let command_pool =
            unsafe { device.create_command_pool(&command_pool_info, None) }?;

        Ok(command_pool)
    }
}

impl Drop for GfaestusVk {
    fn drop(&mut self) {
        println!("GfaestusVk - cleanup swapchain");
        self.cleanup_swapchain();

        let device = self.vk_context.device();
        println!("GfaestusVk - in flight frames");
        self.in_flight_frames.destroy(device);

        unsafe {
            // TODO handle descriptor pool
            println!("GfaestusVk - desc pool");
            device.destroy_descriptor_pool(*self.descriptor_pool, None);
            // TODO handle descriptor set layouts
            // TODO handle buffer memory

            println!("GfaestusVk - transient cmd pool");
            device.destroy_command_pool(self.transient_command_pool, None);
            println!("GfaestusVk - primary cmd pool");
            device.destroy_command_pool(self.command_pool, None);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SwapchainProperties {
    pub extent: vk::Extent2D,
    pub present_mode: vk::PresentModeKHR,
    pub format: vk::SurfaceFormatKHR,
}

struct SwapchainSupportDetails {
    capabilities: vk::SurfaceCapabilitiesKHR,
    formats: Vec<vk::SurfaceFormatKHR>,
    present_modes: Vec<vk::PresentModeKHR>,
}

impl SwapchainSupportDetails {
    fn new(
        device: vk::PhysicalDevice,
        surface: &Surface,
        surface_khr: vk::SurfaceKHR,
    ) -> Result<Self> {
        unsafe {
            let capabilities = surface
                .get_physical_device_surface_capabilities(
                    device,
                    surface_khr,
                )?;

            let formats = surface
                .get_physical_device_surface_formats(device, surface_khr)?;

            let present_modes = surface
                .get_physical_device_surface_present_modes(
                    device,
                    surface_khr,
                )?;

            Ok(Self {
                capabilities,
                formats,
                present_modes,
            })
        }
    }

    fn get_ideal_swapchain_properties(
        &self,
        preferred_dimensions: [u32; 2],
    ) -> SwapchainProperties {
        let format = Self::choose_swapchain_surface_format(&self.formats);
        let present_mode =
            Self::choose_swapchain_surface_present_mode(&self.present_modes);
        let extent = Self::choose_swapchain_extent(
            self.capabilities,
            preferred_dimensions,
        );
        SwapchainProperties {
            format,
            present_mode,
            extent,
        }
    }

    /// Choose the swapchain surface format.
    ///
    /// Will choose B8G8R8A8_UNORM/SRGB_NONLINEAR if possible or
    /// the first available otherwise.
    fn choose_swapchain_surface_format(
        available_formats: &[vk::SurfaceFormatKHR],
    ) -> vk::SurfaceFormatKHR {
        if available_formats.len() == 1
            && available_formats[0].format == vk::Format::UNDEFINED
        {
            return vk::SurfaceFormatKHR {
                format: vk::Format::B8G8R8A8_UNORM,
                color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
            };
        }

        *available_formats
            .iter()
            .find(|format| {
                format.format == vk::Format::B8G8R8A8_UNORM
                    && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .unwrap_or(&available_formats[0])
    }

    /// Choose the swapchain present mode.
    ///
    /// Will favor MAILBOX if present otherwise FIFO.
    /// If none is present it will fallback to IMMEDIATE.
    fn choose_swapchain_surface_present_mode(
        available_present_modes: &[vk::PresentModeKHR],
    ) -> vk::PresentModeKHR {
        if available_present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else if available_present_modes.contains(&vk::PresentModeKHR::FIFO) {
            vk::PresentModeKHR::FIFO
        } else {
            vk::PresentModeKHR::IMMEDIATE
        }
    }

    /// Choose the swapchain extent.
    ///
    /// If a current extent is defined it will be returned.
    /// Otherwise the surface extent clamped between the min
    /// and max image extent will be returned.
    fn choose_swapchain_extent(
        capabilities: vk::SurfaceCapabilitiesKHR,
        preferred_dimensions: [u32; 2],
    ) -> vk::Extent2D {
        if capabilities.current_extent.width != std::u32::MAX {
            return capabilities.current_extent;
        }

        let min = capabilities.min_image_extent;
        let max = capabilities.max_image_extent;
        let width = preferred_dimensions[0].min(max.width).max(min.width);
        let height = preferred_dimensions[1].min(max.height).max(min.height);
        vk::Extent2D { width, height }
    }
}

#[derive(Clone, Copy)]
struct SyncObjects {
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphore: vk::Semaphore,
    fence: vk::Fence,
}

impl SyncObjects {
    fn destroy(&self, device: &Device) {
        unsafe {
            device.destroy_semaphore(self.image_available_semaphore, None);
            device.destroy_semaphore(self.render_finished_semaphore, None);
            device.destroy_fence(self.fence, None);
        }
    }
}

struct InFlightFrames {
    sync_objects: Vec<SyncObjects>,
    current_frame: usize,
}

impl InFlightFrames {
    fn new(sync_objects: Vec<SyncObjects>) -> Self {
        Self {
            sync_objects,
            current_frame: 0,
        }
    }

    fn destroy(&self, device: &Device) {
        self.sync_objects.iter().for_each(|o| o.destroy(&device));
    }
}

impl Iterator for InFlightFrames {
    type Item = SyncObjects;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.sync_objects[self.current_frame];

        self.current_frame = (self.current_frame + 1) % self.sync_objects.len();

        Some(next)
    }
}
