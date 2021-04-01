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
    sync::Arc,
};

use anyhow::Result;

use super::{
    context::*, debug::*, SwapchainProperties, SwapchainSupportDetails,
};

pub(super) fn create_instance(
    entry: &Entry,
    window: &Window,
) -> Result<Instance> {
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

    if super::debug::ENABLE_VALIDATION_LAYERS {
        extension_names.push(DebugReport::name().as_ptr());
    }

    let (_layer_names, layer_names_ptrs) = get_layer_names_and_pointers();

    let mut instance_create_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names);
    if super::debug::ENABLE_VALIDATION_LAYERS {
        check_validation_layer_support(&entry);
        instance_create_info =
            instance_create_info.enabled_layer_names(&layer_names_ptrs);
    }

    let instance =
        unsafe { entry.create_instance(&instance_create_info, None) }?;

    Ok(instance)
}

pub(super) fn find_queue_families(
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

pub(super) fn device_supports_extensions(
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
pub(super) fn required_device_extensions() -> [&'static CStr; 1] {
    [Swapchain::name()]
}

pub(super) fn device_is_suitable(
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

pub(super) fn choose_physical_device(
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

pub(super) fn create_swapchain_and_images(
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

pub(super) fn create_swapchain_image_views(
    device: &Device,
    swapchain_images: &[vk::Image],
    swapchain_properties: SwapchainProperties,
) -> Result<Vec<vk::ImageView>> {
    let mut img_views = Vec::with_capacity(swapchain_images.len());

    for image in swapchain_images.iter() {
        let view = super::GfaestusVk::create_image_view(
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

pub(super) fn create_logical_device(
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
        .geometry_shader(true)
        .independent_blend(true)
        .build();

    let (_layer_names, layer_names_ptrs) = get_layer_names_and_pointers();

    let mut device_create_info_builder = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&device_extensions_ptrs)
        .enabled_features(&device_features);

    if super::debug::ENABLE_VALIDATION_LAYERS {
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

pub(super) fn find_memory_type(
    reqs: vk::MemoryRequirements,
    mem_props: vk::PhysicalDeviceMemoryProperties,
    req_props: vk::MemoryPropertyFlags,
) -> u32 {
    for i in 0..mem_props.memory_type_count {
        if reqs.memory_type_bits & (1 << i) != 0
            && mem_props.memory_types[i as usize]
                .property_flags
                .contains(req_props)
        {
            return i;
        }
    }

    panic!("Failed to find suitable memory type");
}

pub(super) fn create_descriptor_pool(
    device: &Device,
    size: u32,
) -> Result<vk::DescriptorPool> {
    let ubo_pool_size = vk::DescriptorPoolSize {
        ty: vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: size,
    };
    let sampler_pool_size = vk::DescriptorPoolSize {
        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: size,
    };

    let pool_sizes = [ubo_pool_size, sampler_pool_size];

    let pool_info = vk::DescriptorPoolCreateInfo::builder()
        .pool_sizes(&pool_sizes)
        .max_sets(size)
        .build();

    let pool = unsafe { device.create_descriptor_pool(&pool_info, None) }?;

    Ok(pool)
}
