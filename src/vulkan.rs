pub mod context;
pub mod debug;

use context::*;
use debug::*;

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
    swapchain_framebuffers: Vec<vk::Framebuffer>,

    command_pool: vk::CommandPool,
    transient_command_pool: vk::CommandPool,

    command_buffers: Vec<vk::CommandBuffer>,
}

fn create_instance(entry: &Entry, window: &Window) -> Result<Instance> {
    let app_name = CString::new("Gfaestus")?;
    let info = vk::ApplicationInfo::builder()
        .application_name(app_name.as_c_str())
        .application_version(vk::make_version(0, 1, 0))
        .engine_name(app_name.as_c_str())
        .engine_version(vk::make_version(0, 1, 0))
        .api_version(vk::make_version(1, 0, 0))
        .build();

    let mut extensions = ash_window::enumerate_required_extensions(window)?
        .iter()
        .map(|ext| ext.as_ptr())
        .collect::<Vec<_>>();

    if debug::ENABLE_VALIDATION_LAYERS {
        extensions.push(DebugReport::name().as_ptr());
    }

    let mut instance_info = vk::InstanceCreateInfo::builder()
        .application_info(&info)
        .enabled_extension_names(&extensions);

    let (_, layer_name_ptrs) = get_layer_names_and_pointers();

    if debug::ENABLE_VALIDATION_LAYERS {
        check_validation_layer_support(&entry);
        instance_info = instance_info.enabled_layer_names(&layer_name_ptrs);
    }

    let instance = unsafe { entry.create_instance(&instance_info, None) }?;

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

// fn required_device_extensions

fn device_supports_extensions(
    instance: &Instance,
    device: vk::PhysicalDevice,
) -> Result<bool> {
    // may be expanded in the future
    let required_exts = [Swapchain::name()];

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


}

struct SwapchainProperties {
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
