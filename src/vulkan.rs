pub mod compute;
pub mod context;
pub mod debug;
pub mod draw_system;
pub mod render_pass;
pub mod texture;

pub mod msg;

mod init;

pub use msg::*;

use context::*;
use init::*;
use render_pass::*;

use anyhow::Result;
use ash::{
    extensions::khr::{Surface, Swapchain},
    version::DeviceV1_0,
    vk, Device, Entry,
};

use bytemuck::{Pod, Zeroable};
use parking_lot::{Mutex, MutexGuard};
use std::{mem::size_of, sync::Arc};
use vk_mem::Allocator;

#[cfg(target_os = "linux")]
use winit::platform::unix::*;
use winit::{
    event_loop::EventLoop,
    window::{Window, WindowBuilder},
};

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use crate::{app::Args, view::ScreenDims};

pub struct GfaestusVk {
    pub allocator: Allocator,

    pub graphics_queue: vk::Queue,
    pub present_queue: vk::Queue,

    pub graphics_family_index: u32,
    pub present_family_index: u32,

    pub msaa_samples: vk::SampleCountFlags,

    pub swapchain: Swapchain,
    pub swapchain_khr: vk::SwapchainKHR,
    pub swapchain_props: SwapchainProperties,

    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,

    pub render_passes: RenderPasses,
    pub node_attachments: NodeAttachments,
    pub offscreen_attachment: OffscreenAttachment,
    pub framebuffers: Vec<Framebuffers>,

    pub command_pool: vk::CommandPool,
    pub transient_command_pool: vk::CommandPool,
    in_flight_frames: InFlightFrames,

    pub vk_context: VkContext,
    // dimensions: ScreenDims,
    // pub supported_features: SupportedFeatures,
}

impl GfaestusVk {
    pub fn new(args: &Args) -> Result<(Self, EventLoop<()>, Window)> {
        log::debug!("Initializing GfaestusVk context");
        let entry = unsafe { Entry::new() }?;

        let instance_exts = init::instance_extensions(&entry)?;

        let (event_loop, window) = {
            let event_loop: EventLoop<()>;

            #[cfg(target_os = "linux")]
            {
                event_loop = if args.force_x11 || !instance_exts.wayland_surface
                {
                    if let Ok(ev_loop) = EventLoop::new_x11() {
                        log::debug!("Using X11 event loop");
                        ev_loop
                    } else {
                        error!(
                            "Error initializing X11 window, falling back to default"
                        );
                        EventLoop::new()
                    }
                } else {
                    log::debug!("Using default event loop");
                    EventLoop::new()
                };
            }

            #[cfg(not(target_os = "linux"))]
            {
                log::debug!("Using default event loop");
                event_loop = EventLoop::new();
            }

            log::debug!("Creating window");
            let window = WindowBuilder::new()
                .with_title("Gfaestus")
                .with_inner_size(winit::dpi::PhysicalSize::new(800, 600))
                .build(&event_loop)?;

            (event_loop, window)
        };

        log::debug!("Created Vulkan entry");
        let instance = create_instance(&entry, &window)?;
        log::debug!("Created Vulkan instance");

        let surface = Surface::new(&entry, &instance);
        let surface_khr = unsafe {
            ash_window::create_surface(&entry, &instance, &window, None)
        }?;
        log::debug!("Created window surface");

        let debug_utils = debug::setup_debug_utils(&entry, &instance);

        let (physical_device, graphics_ix, present_ix, compute_ix) =
            choose_physical_device(
                &instance,
                &surface,
                surface_khr,
                args.force_graphics_device.as_deref(),
            )?;

        let (device, graphics_queue, present_queue, _compute_queue) =
            create_logical_device(
                &instance,
                physical_device,
                graphics_ix,
                present_ix,
                compute_ix,
            )?;

        let allocator_create_info = vk_mem::AllocatorCreateInfo {
            physical_device,
            device: device.clone(),
            instance: instance.clone(),
            flags: vk_mem::AllocatorCreateFlags::NONE,
            preferred_large_heap_block_size: 0,
            frame_in_use_count: 0,
            heap_size_limits: None,
        };

        let allocator = vk_mem::Allocator::new(&allocator_create_info)?;

        let vk_context = VkContext::new(
            entry,
            instance,
            debug_utils,
            surface,
            surface_khr,
            physical_device,
            device,
        )?;

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

        let in_flight_frames = Self::create_sync_objects(vk_context.device());

        let render_passes = RenderPasses::create(
            &vk_context,
            vk_context.device(),
            swapchain_props,
            msaa_samples,
        )?;

        let node_attachments = NodeAttachments::new(
            &vk_context,
            transient_command_pool,
            graphics_queue,
            swapchain_props,
            msaa_samples,
            render_passes.id_format,
        )?;

        let offscreen_attachment = OffscreenAttachment::new(
            &vk_context,
            transient_command_pool,
            graphics_queue,
            swapchain_props,
        )?;

        let framebuffers = swapchain_image_views
            .iter()
            .map(|view| {
                render_passes
                    .framebuffers(
                        vk_context.device(),
                        &node_attachments,
                        &offscreen_attachment,
                        *view,
                        swapchain_props,
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();

        let result = Self {
            vk_context,

            allocator,

            graphics_queue,
            present_queue,

            graphics_family_index: graphics_ix,
            present_family_index: present_ix,

            msaa_samples,

            swapchain,
            swapchain_khr,
            swapchain_props,

            swapchain_images: images,
            swapchain_image_views,

            render_passes,
            node_attachments,
            offscreen_attachment,
            framebuffers,

            command_pool,
            transient_command_pool,

            in_flight_frames,
        };

        result.render_passes.set_vk_debug_names(&result)?;

        for fb in result.framebuffers.iter() {
            fb.set_vk_debug_names(&result)?;
        }

        result.node_attachments.set_vk_debug_names(&result)?;

        result.set_debug_object_name(
            result.offscreen_attachment.color.image,
            "Offscreen Color Attachment",
        )?;

        Ok((result, event_loop, window))
    }

    pub fn swapchain_dims(&self) -> ScreenDims {
        let extent = self.swapchain_props.extent;

        ScreenDims {
            width: extent.width as f32,
            height: extent.height as f32,
        }
    }

    pub fn vk_context(&self) -> &VkContext {
        &self.vk_context
    }

    pub fn draw_frame_from<F>(
        &mut self,
        window_size: [u32; 2],
        commands: F,
    ) -> Result<bool>
    where
        F: FnOnce(&Device, vk::CommandBuffer, &Framebuffers),
    {
        let dims: [u32; 2] = self.swapchain_dims().into();

        if window_size != dims {
            return Ok(true);
        }

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

        let device = self.vk_context.device();

        let wait_semaphores = [img_available];
        let signal_semaphores = [render_finished];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];

        let queue = self.graphics_queue;

        let framebuffers = &self.framebuffers[img_index as usize];

        let cmd_buf = self.execute_one_time_commands_semaphores(
            device,
            self.command_pool,
            queue,
            &wait_semaphores,
            &wait_stages,
            &signal_semaphores,
            in_flight_fence,
            |cmd_buf| {
                commands(device, cmd_buf, framebuffers);
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

        unsafe {
            device.free_command_buffers(self.command_pool, &[cmd_buf]);
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
    ) -> Result<vk::CommandBuffer>
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
                max_depth: 1.0,
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
            }
        }

        Ok(cmd_buf)
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

    pub fn image_transition_barrier(
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) -> (
        vk::ImageMemoryBarrier,
        vk::PipelineStageFlags,
        vk::PipelineStageFlags,
    ) {
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
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                ) => (
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                ),
                (
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ) => (
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::SHADER_READ,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
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
                (
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                ) => (
                    vk::AccessFlags::SHADER_READ,
                    vk::AccessFlags::MEMORY_READ
                        | vk::AccessFlags::MEMORY_WRITE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                ),
                (
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ) => (
                    vk::AccessFlags::MEMORY_READ
                        | vk::AccessFlags::MEMORY_WRITE,
                    vk::AccessFlags::SHADER_READ,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                ),
                (vk::ImageLayout::UNDEFINED, vk::ImageLayout::GENERAL) => (
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                        | vk::AccessFlags::MEMORY_READ
                        | vk::AccessFlags::MEMORY_WRITE,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
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

        (barrier, src_stage, dst_stage)
    }

    pub fn transition_image(
        device: &Device,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) -> Result<()> {
        Self::execute_one_time_commands(
            device,
            command_pool,
            transition_queue,
            |buf| {
                let (barrier, src_stage, dst_stage) =
                    Self::image_transition_barrier(
                        image, old_layout, new_layout,
                    );

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
        let device = vk_context.device();

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

        log::debug!("Creating image {:?}", img_info);
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

        log::debug!("Allocating {} bytes of memory for image", mem_reqs.size);
        let memory = unsafe {
            let mem = device.allocate_memory(&alloc_info, None)?;
            device.bind_image_memory(image, mem, 0)?;
            mem
        };
        log::debug!("Image created: {:?}", image);

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

    pub fn copy_image_to_buffer(
        device: &Device,
        command_pool: vk::CommandPool,
        transfer_queue: vk::Queue,
        image: vk::Image,
        buffer: vk::Buffer,
        extent: vk::Extent2D,
    ) -> Result<()> {
        Self::execute_one_time_commands(
            device,
            command_pool,
            transfer_queue,
            |cmd_buf| {
                let region = vk::BufferImageCopy::builder()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                    .image_extent(vk::Extent3D {
                        width: extent.width,
                        height: extent.height,
                        depth: 1,
                    })
                    .build();

                let regions = [region];

                unsafe {
                    device.cmd_copy_image_to_buffer(
                        cmd_buf,
                        image,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        buffer,
                        &regions,
                    )
                }
            },
        )
    }

    pub fn copy_buffer_to_image(
        device: &Device,
        command_pool: vk::CommandPool,
        transfer_queue: vk::Queue,
        src: vk::Buffer,
        image: vk::Image,
        extent: vk::Extent2D,
    ) -> Result<()> {
        Self::execute_one_time_commands(
            device,
            command_pool,
            transfer_queue,
            |cmd_buf| {
                let region = vk::BufferImageCopy::builder()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                    .image_extent(vk::Extent3D {
                        width: extent.width,
                        height: extent.height,
                        depth: 1,
                    })
                    .build();

                let regions = [region];

                unsafe {
                    device.cmd_copy_buffer_to_image(
                        cmd_buf,
                        src,
                        image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &regions,
                    )
                }
            },
        )
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

    pub fn download_buffer<A, T>(
        &self,
        src: vk::Buffer,
        element_count: usize,
        dst: &mut Vec<T>,
    ) -> Result<()>
    where
        T: Copy,
    {
        use vk::BufferUsageFlags as Usage;
        use vk::MemoryPropertyFlags as MemPropFlags;

        let vk_context = &self.vk_context;
        let device = vk_context.device();
        let size = (element_count * size_of::<T>()) as vk::DeviceSize;

        let (staging_buf, staging_mem, _staging_mem_size) = self
            .create_buffer(
                size,
                Usage::TRANSFER_DST,
                MemPropFlags::HOST_VISIBLE
                    | MemPropFlags::HOST_COHERENT
                    | MemPropFlags::HOST_CACHED,
            )?;

        GfaestusVk::copy_buffer(
            device,
            self.transient_command_pool,
            self.graphics_queue,
            src,
            staging_buf,
            size,
        );

        dst.clear();

        if dst.capacity() < element_count {
            let extra = element_count - dst.capacity();
            dst.reserve(extra);
        }

        unsafe {
            let data_ptr = device.map_memory(
                staging_mem,
                0,
                size,
                vk::MemoryMapFlags::empty(),
            )?;

            let val_ptr = data_ptr as *const T;

            let slice = std::slice::from_raw_parts(val_ptr, element_count);

            dst.copy_from_slice(slice);

            device.unmap_memory(staging_mem);
        }

        unsafe {
            device.destroy_buffer(staging_buf, None);
            device.free_memory(staging_mem, None);
        }

        Ok(())
    }

    pub fn copy_data_to_buffer<A, T>(
        &self,
        data: &[T],
        dst: vk::Buffer,
    ) -> Result<()>
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

        GfaestusVk::copy_buffer(
            device,
            self.transient_command_pool,
            self.graphics_queue,
            staging_buf,
            dst,
            size,
        );

        unsafe {
            device.destroy_buffer(staging_buf, None);
            device.free_memory(staging_mem, None);
        }

        Ok(())
    }

    pub fn create_uninitialized_buffer<T>(
        &self,
        buffer_usage: vk::BufferUsageFlags,
        memory_usage: vk_mem::MemoryUsage,
        mapped: bool,
        element_count: usize,
    ) -> Result<(vk::Buffer, vk_mem::Allocation, vk_mem::AllocationInfo)>
    where
        T: Zeroable,
    {
        let size = element_count * std::mem::size_of::<T>();

        let buffer_info = vk::BufferCreateInfo::builder()
            .size(size as vk::DeviceSize)
            .usage(buffer_usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .build();

        let create_info = if mapped {
            vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::CpuOnly,
                flags: vk_mem::AllocationCreateFlags::MAPPED,
                ..Default::default()
            }
        } else {
            vk_mem::AllocationCreateInfo {
                usage: memory_usage,
                ..Default::default()
            }
        };

        let (buffer, alloc, alloc_info) =
            self.allocator.create_buffer(&buffer_info, &create_info)?;

        Ok((buffer, alloc, alloc_info))
    }

    pub fn create_buffer_with_data<T>(
        &self,
        usage: vk::BufferUsageFlags,
        memory_usage: vk_mem::MemoryUsage,
        mapped: bool,
        data: &[T],
    ) -> Result<(vk::Buffer, vk_mem::Allocation, vk_mem::AllocationInfo)>
    where
        T: Pod,
    {
        use vk::BufferUsageFlags as Usage;

        let vk_context = &self.vk_context;
        let device = vk_context.device();
        let size = (data.len() * size_of::<T>()) as vk::DeviceSize;

        let staging_buffer_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(Usage::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .build();

        let staging_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuToGpu,
            flags: vk_mem::AllocationCreateFlags::MAPPED,
            ..Default::default()
        };

        let (staging_buf, staging_alloc, staging_alloc_info) =
            self.allocator
                .create_buffer(&staging_buffer_info, &staging_create_info)?;

        unsafe {
            let mapped_ptr = staging_alloc_info.get_mapped_data();

            let target_slice =
                std::slice::from_raw_parts_mut(mapped_ptr, size as usize);

            target_slice.clone_from_slice(bytemuck::cast_slice(&data))
        }

        let buffer_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(Usage::TRANSFER_DST | usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .build();

        let create_info = if mapped {
            vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::CpuOnly,
                flags: vk_mem::AllocationCreateFlags::MAPPED,
                ..Default::default()
            }
        } else {
            vk_mem::AllocationCreateInfo {
                usage: memory_usage,
                ..Default::default()
            }
        };

        let (buffer, alloc, alloc_info) =
            self.allocator.create_buffer(&buffer_info, &create_info)?;

        Self::copy_buffer(
            device,
            self.transient_command_pool,
            self.graphics_queue,
            staging_buf,
            buffer,
            size,
        );

        self.allocator.destroy_buffer(staging_buf, &staging_alloc)?;

        Ok((buffer, alloc, alloc_info))
    }

    pub fn create_device_local_buffer_with_data<A, T>(
        &self,
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
            MemPropFlags::HOST_VISIBLE | MemPropFlags::HOST_COHERENT,
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

        let render_passes = RenderPasses::create(
            &self.vk_context,
            device,
            swapchain_props,
            self.msaa_samples,
        )?;

        render_passes.set_vk_debug_names(self)?;

        let node_attachments = NodeAttachments::new(
            self.vk_context(),
            self.transient_command_pool,
            self.graphics_queue,
            swapchain_props,
            self.msaa_samples,
            render_passes.id_format,
        )?;

        node_attachments.set_vk_debug_names(self)?;

        let offscreen_attachment = OffscreenAttachment::new(
            self.vk_context(),
            self.transient_command_pool,
            self.graphics_queue,
            swapchain_props,
        )?;

        self.set_debug_object_name(
            offscreen_attachment.color.image,
            "Offscreen Color Attachment",
        )?;

        let framebuffers = swapchain_image_views
            .iter()
            .map(|view| {
                render_passes
                    .framebuffers(
                        device,
                        &node_attachments,
                        &offscreen_attachment,
                        *view,
                        swapchain_props,
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();

        for fb in framebuffers.iter() {
            fb.set_vk_debug_names(self)?;
        }

        // TODO recreate render pass, framebuffers, etc.

        self.swapchain = swapchain;
        self.swapchain_khr = swapchain_khr;
        self.swapchain_props = swapchain_props;

        self.swapchain_images = images;
        self.swapchain_image_views = swapchain_image_views;

        self.render_passes = render_passes;
        self.node_attachments = node_attachments;
        self.offscreen_attachment = offscreen_attachment;
        self.framebuffers = framebuffers;

        Ok(())
    }

    pub(crate) fn set_debug_object_name<T: ash::vk::Handle>(
        &self,
        object: T,
        name: &str,
    ) -> Result<()> {
        use std::ffi::CString;

        if let Some(utils) = self.vk_context.debug_utils() {
            let name = CString::new(name)?;

            let debug_name_info = vk::DebugUtilsObjectNameInfoEXT::builder()
                .object_type(T::TYPE)
                .object_handle(object.as_raw())
                .object_name(&name)
                .build();

            unsafe {
                utils.debug_utils_set_object_name(
                    self.vk_context().device().handle(),
                    &debug_name_info,
                )?;
            }
        }

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
            self.framebuffers.iter().for_each(|f| f.destroy(device));
            self.render_passes.destroy(device);

            self.node_attachments.destroy(device);
            self.offscreen_attachment.destroy(device);

            self.swapchain_image_views
                .iter()
                .for_each(|v| device.destroy_image_view(*v, None));

            self.swapchain.destroy_swapchain(self.swapchain_khr, None);
        }
    }

    pub(crate) fn create_command_pool(
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
        self.cleanup_swapchain();

        let device = self.vk_context.device();
        self.in_flight_frames.destroy(device);

        unsafe {
            device.destroy_command_pool(self.transient_command_pool, None);
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
        let checkit = |v| available_present_modes.contains(&v).then(|| v);

        checkit(vk::PresentModeKHR::FIFO)
            .or(checkit(vk::PresentModeKHR::MAILBOX))
            .unwrap_or(vk::PresentModeKHR::IMMEDIATE)
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

#[derive(Clone)]
pub struct Queues {
    graphics_queue: Arc<Mutex<vk::Queue>>,
    present_queue: Arc<Mutex<vk::Queue>>,
    compute_queue: Arc<Mutex<vk::Queue>>,
}

impl Queues {
    #[allow(dead_code)]
    fn new(
        graphics: vk::Queue,
        present: vk::Queue,
        compute: vk::Queue,
    ) -> Self {
        let graphics_queue: Arc<Mutex<vk::Queue>>;
        let present_queue: Arc<Mutex<vk::Queue>>;
        let compute_queue: Arc<Mutex<vk::Queue>>;

        graphics_queue = Arc::new(Mutex::new(graphics));

        if present == graphics {
            present_queue = graphics_queue.clone();
        } else {
            present_queue = Arc::new(Mutex::new(present));
        }

        if compute == graphics {
            compute_queue = graphics_queue.clone();
        } else {
            compute_queue = Arc::new(Mutex::new(compute));
        }

        Self {
            graphics_queue,
            present_queue,
            compute_queue,
        }
    }

    pub fn lock_graphics(&self) -> MutexGuard<'_, vk::Queue> {
        self.graphics_queue.lock()
    }

    pub fn try_lock_graphics(&self) -> Option<MutexGuard<'_, vk::Queue>> {
        self.graphics_queue.try_lock()
    }

    pub fn lock_present(&self) -> MutexGuard<'_, vk::Queue> {
        self.present_queue.lock()
    }

    pub fn try_lock_present(&self) -> Option<MutexGuard<'_, vk::Queue>> {
        self.present_queue.try_lock()
    }

    pub fn lock_compute(&self) -> MutexGuard<'_, vk::Queue> {
        self.compute_queue.lock()
    }

    pub fn try_lock_compute(&self) -> Option<MutexGuard<'_, vk::Queue>> {
        self.compute_queue.try_lock()
    }

    pub fn is_graphics_locked(&self) -> bool {
        self.graphics_queue.is_locked()
    }

    pub fn is_present_locked(&self) -> bool {
        self.present_queue.is_locked()
    }

    pub fn is_compute_locked(&self) -> bool {
        self.compute_queue.is_locked()
    }
}
