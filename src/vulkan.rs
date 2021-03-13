pub mod context;
pub mod debug;
pub mod draw_system;
pub mod render_pass;
pub mod texture;

use context::*;
use debug::*;
use draw_system::*;
use render_pass::*;
use texture::*;

use ash::{
    extensions::{
        ext::DebugReport,
        khr::{Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk::SurfaceKHR,
};
use ash::{vk, Device, Entry, Instance};

use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

use std::{
    ffi::{CStr, CString},
    mem::{align_of, size_of},
};

use anyhow::Result;

pub struct GfaestusVk {
    vk_context: VkContext,

    graphics_queue: vk::Queue,
    present_queue: vk::Queue,

    graphics_family_index: u32,
    present_family_index: u32,

    msaa_samples: vk::SampleCountFlags,

    swapchain: Swapchain,
    swapchain_khr: vk::SwapchainKHR,
    swapchain_props: SwapchainProperties,

    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,
    // swapchain_framebuffers: Vec<vk::Framebuffer>,
    command_pool: vk::CommandPool,
    transient_command_pool: vk::CommandPool,
    // command_buffers: Vec<vk::CommandBuffer>,
    in_flight_frames: InFlightFrames,
}

fn create_instance(entry: &Entry, window: &Window) -> Result<Instance> {
    let app_name = CString::new("Gfaestus")?;

    let app_info = vk::ApplicationInfo::builder()
        .application_name(app_name.as_c_str())
        .application_version(vk::make_version(0, 1, 0))
        .engine_name(app_name.as_c_str())
        .engine_version(vk::make_version(0, 1, 0))
        .api_version(vk::make_version(1, 0, 0))
        .build();

    let extension_names =
        ash_window::enumerate_required_extensions(window).unwrap();
    let mut extension_names = extension_names
        .iter()
        .map(|ext| ext.as_ptr())
        .collect::<Vec<_>>();

    if debug::ENABLE_VALIDATION_LAYERS {
        extension_names.push(DebugReport::name().as_ptr());
    }

    let (_layer_names, layer_names_ptrs) = get_layer_names_and_pointers();

    let mut instance_create_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names);
    if debug::ENABLE_VALIDATION_LAYERS {
        check_validation_layer_support(&entry);
        instance_create_info =
            instance_create_info.enabled_layer_names(&layer_names_ptrs);
    }

    let instance =
        unsafe { entry.create_instance(&instance_create_info, None) }?;

    Ok(instance)
}

fn find_queue_families(
    instance: &Instance,
    surface: &Surface,
    surface_khr: vk::SurfaceKHR,
    device: vk::PhysicalDevice,
) -> Result<(Option<u32>, Option<u32>)> {
    let mut graphics_ix: Option<u32> = None;
    let mut present_ix: Option<u32> = None;

    let props =
        unsafe { instance.get_physical_device_queue_family_properties(device) };

    for (ix, family) in
        props.iter().filter(|fam| fam.queue_count > 0).enumerate()
    {
        if family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            && graphics_ix.is_none()
        {
            graphics_ix = Some(ix as u32);
        }

        let supports_present = unsafe {
            surface.get_physical_device_surface_support(
                device,
                ix as u32,
                surface_khr,
            )
        }?;

        if supports_present && present_ix.is_none() {
            present_ix = Some(ix as u32);
        }

        if graphics_ix.is_some() && present_ix.is_some() {
            break;
        }
    }

    Ok((graphics_ix, present_ix))
}

fn device_supports_extensions(
    instance: &Instance,
    device: vk::PhysicalDevice,
) -> Result<bool> {
    let required_exts = required_device_extensions();

    let extension_props =
        unsafe { instance.enumerate_device_extension_properties(device) }?;

    for req in required_exts.iter() {
        let found = extension_props.iter().any(|ext| {
            let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
            req == &name
        });

        if !found {
            return Ok(false);
        }
    }

    Ok(true)
}

// may be expanded in the future
fn required_device_extensions() -> [&'static CStr; 1] {
    [Swapchain::name()]
}

fn device_is_suitable(
    instance: &Instance,
    surface: &Surface,
    surface_khr: SurfaceKHR,
    device: vk::PhysicalDevice,
) -> Result<bool> {
    let (graphics_ix, present_ix) =
        find_queue_families(instance, surface, surface_khr, device)?;

    if graphics_ix.is_none() || present_ix.is_none() {
        return Ok(false);
    }

    if !device_supports_extensions(instance, device)? {
        return Ok(false);
    }

    let swapchain_adequate = {
        let details =
            SwapchainSupportDetails::new(device, surface, surface_khr)?;
        !details.formats.is_empty() && !details.present_modes.is_empty()
    };

    if !swapchain_adequate {
        return Ok(false);
    }

    let features = unsafe { instance.get_physical_device_features(device) };

    // TODO this should be tailored
    Ok(features.sampler_anisotropy == vk::TRUE)
}

fn choose_physical_device(
    instance: &Instance,
    surface: &Surface,
    surface_khr: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, u32, u32)> {
    let device = {
        let devices = unsafe { instance.enumerate_physical_devices() }?;

        devices
            .into_iter()
            .find(|&dev| {
                device_is_suitable(instance, surface, surface_khr, dev).unwrap()
            })
            .unwrap()
    };

    let properties = unsafe { instance.get_physical_device_properties(device) };

    unsafe {
        eprintln!(
            "Selected physical device: {:?}",
            CStr::from_ptr(properties.device_name.as_ptr())
        );
    }

    let (graphics_ix, present_ix) =
        find_queue_families(instance, surface, surface_khr, device)?;

    Ok((device, graphics_ix.unwrap(), present_ix.unwrap()))
}

fn create_swapchain_and_images(
    vk_context: &VkContext,
    graphics_ix: u32,
    present_ix: u32,
    dimensions: [u32; 2],
) -> Result<(
    Swapchain,
    vk::SwapchainKHR,
    SwapchainProperties,
    Vec<vk::Image>,
)> {
    let details = SwapchainSupportDetails::new(
        vk_context.physical_device(),
        vk_context.surface(),
        vk_context.surface_khr(),
    )?;

    let props = details.get_ideal_swapchain_properties(dimensions);

    let image_count = {
        let max = details.capabilities.max_image_count;
        let mut preferred = details.capabilities.min_image_count + 1;
        if max > 0 && preferred > max {
            preferred = max;
        }
        preferred
    };

    eprintln!(
            "Creating swapchain.\n\tFormat: {:?}\n\tColorSpace: {:?}\n\tPresentMode: {:?}\n\tExtent: {:?}\n\tImageCount: {:?}",
            props.format.format,
            props.format.color_space,
            props.present_mode,
            props.extent,
            image_count,
        );

    let family_indices = [graphics_ix, present_ix];

    let create_info = {
        let mut builder = vk::SwapchainCreateInfoKHR::builder()
            .surface(vk_context.surface_khr())
            .min_image_count(image_count)
            .image_format(props.format.format)
            .image_color_space(props.format.color_space)
            .image_extent(props.extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);

        builder = if graphics_ix != present_ix {
            builder
                .image_sharing_mode(vk::SharingMode::CONCURRENT)
                .queue_family_indices(&family_indices)
        } else {
            builder.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        };

        builder
            .pre_transform(details.capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(props.present_mode)
            .clipped(true)
            .build()
    };

    let swapchain = Swapchain::new(vk_context.instance(), vk_context.device());
    let swapchain_khr =
        unsafe { swapchain.create_swapchain(&create_info, None) }?;
    let images = unsafe { swapchain.get_swapchain_images(swapchain_khr) }?;

    Ok((swapchain, swapchain_khr, props, images))
}

fn create_swapchain_image_views(
    device: &Device,
    swapchain_images: &[vk::Image],
    swapchain_properties: SwapchainProperties,
) -> Result<Vec<vk::ImageView>> {
    let mut img_views = Vec::with_capacity(swapchain_images.len());

    for image in swapchain_images.iter() {
        let view = GfaestusVk::create_image_view(
            device,
            *image,
            1,
            swapchain_properties.format.format,
            vk::ImageAspectFlags::COLOR,
        )?;

        img_views.push(view);
    }

    Ok(img_views)
}

fn create_logical_device(
    instance: &Instance,
    device: vk::PhysicalDevice,
    graphics_ix: u32,
    present_ix: u32,
) -> Result<(Device, vk::Queue, vk::Queue)> {
    let queue_priorities = [1.0f32];

    let queue_infos = {
        use rustc_hash::FxHashSet;
        let indices = [graphics_ix, present_ix]
            .iter()
            .copied()
            .collect::<FxHashSet<_>>();

        indices
            .iter()
            .map(|&ix| {
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(ix)
                    .queue_priorities(&queue_priorities)
                    .build()
            })
            .collect::<Vec<_>>()
    };

    let device_extensions = required_device_extensions();
    let device_extensions_ptrs = device_extensions
        .iter()
        .map(|ext| ext.as_ptr())
        .collect::<Vec<_>>();

    let device_features = vk::PhysicalDeviceFeatures::builder()
        .sampler_anisotropy(true)
        .build();

    let (_layer_names, layer_names_ptrs) = get_layer_names_and_pointers();

    let mut device_create_info_builder = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&device_extensions_ptrs)
        .enabled_features(&device_features);

    if debug::ENABLE_VALIDATION_LAYERS {
        device_create_info_builder =
            device_create_info_builder.enabled_layer_names(&layer_names_ptrs);
    }

    let device_create_info = device_create_info_builder.build();

    let device =
        unsafe { instance.create_device(device, &device_create_info, None) }?;

    let graphics_queue = unsafe { device.get_device_queue(graphics_ix, 0) };
    let present_queue = unsafe { device.get_device_queue(present_ix, 0) };

    Ok((device, graphics_queue, present_queue))
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

        Ok(Self {
            vk_context,

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

            command_pool,
            transient_command_pool,

            in_flight_frames,
        })
    }

    pub fn draw_frame(&mut self) -> Result<bool> {
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

        {
            // TODO submit command buffers
        }

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

        Ok(false)
    }

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

    pub fn wait_gpu_idle(&self) -> Result<()> {
        let res = unsafe { self.vk_context.device().device_wait_idle() }?;
        Ok(res)
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

        let (swapchain, swapchain_khr, properties, images) =
            create_swapchain_and_images(
                &self.vk_context,
                self.graphics_family_index,
                self.present_family_index,
                dimensions,
            )?;

        let swapchain_image_views =
            create_swapchain_image_views(device, &images, properties)?;

        // TODO recreate render pass, framebuffers, etc.

        self.swapchain = swapchain;
        self.swapchain_khr = swapchain_khr;
        self.swapchain_props = properties;
        self.swapchain_images = images;
        self.swapchain_image_views = swapchain_image_views;

        Ok(())
    }

    fn cleanup_swapchain(&mut self) {
        let device = self.vk_context.device();

        unsafe {
            // TODO handle framebuffers, pipelines, etc.
            self.swapchain_image_views
                .iter()
                .for_each(|v| device.destroy_image_view(*v, None));
            self.swapchain.destroy_swapchain(self.swapchain_khr, None);
        }
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
        self.cleanup_swapchain();

        let device = self.vk_context.device();
        self.in_flight_frames.destroy(device);

        unsafe {
            // TODO handle descriptor pool
            // TODO handle descriptor set layouts
            // TODO handle buffer memory

            device.destroy_command_pool(self.transient_command_pool, None);
            device.destroy_command_pool(self.command_pool, None);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SwapchainProperties {
    extent: vk::Extent2D,
    present_mode: vk::PresentModeKHR,
    format: vk::SurfaceFormatKHR,
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
