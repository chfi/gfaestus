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

struct SwapchainConfig {
    extent: vk::Extent2D,
    present_mode: vk::PresentModeKHR,
    format: vk::SurfaceFormatKHR,
}

pub struct GfaestusVk {
    vk_context: VkContext,

    graphics_queue: vk::Queue,
    present_queue: vk::Queue,

    graphics_family_index: u32,
    present_family_index: u32,

    msaa_samples: vk::SampleCountFlags,

    swapchain: Swapchain,
    swapchain_khr: vk::SwapchainKHR,
    swapchain_cfg: SwapchainConfig,

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

    let supports_extensions = device_supports_extensions(instance, device)?;

    // let swapchain_adequate = {
    //     let details = Swap
    // };

    unimplemented!();
}

