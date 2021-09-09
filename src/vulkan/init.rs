use ash::{
    extensions::{
        ext::DebugUtils,
        khr::{PushDescriptor, Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk::SurfaceKHR,
};
use ash::{vk, Device, Entry, Instance};

use bstr::ByteSlice;
use winit::window::Window;

use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
};

use anyhow::Result;

use super::{
    context::*, debug::*, SwapchainProperties, SwapchainSupportDetails,
};

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

pub(super) fn create_instance(
    entry: &Entry,
    window: &Window,
) -> Result<Instance> {
    log::debug!("Creating instance");
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
    log::debug!("Enumerated required extensions");
    let mut extension_names = extension_names
        .iter()
        .map(|ext| ext.as_ptr())
        .collect::<Vec<_>>();

    if super::debug::ENABLE_VALIDATION_LAYERS {
        extension_names.push(DebugUtils::name().as_ptr());
    }

    let phys_device_properties2 =
        CString::new("VK_KHR_get_physical_device_properties2")?;
    extension_names.push(phys_device_properties2.as_ptr());

    log::debug!("getting layer names and pointers");
    let (_layer_names, layer_names_ptrs) = get_layer_names_and_pointers();

    let mut instance_create_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names);

    if super::debug::ENABLE_VALIDATION_LAYERS {
        check_validation_layer_support(&entry);
        instance_create_info =
            instance_create_info.enabled_layer_names(&layer_names_ptrs);
    }

    for ext in extension_names.iter() {
        let name = unsafe { CStr::from_ptr(*ext) };
        log::debug!("Loading instance extension {:?}", name);
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
) -> Result<(Option<u32>, Option<u32>, Option<u32>)> {
    let mut graphics_ix: Option<u32> = None;
    let mut present_ix: Option<u32> = None;
    let mut compute_ix: Option<u32> = None;

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

        if family.queue_flags.contains(vk::QueueFlags::COMPUTE)
            && !family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            && compute_ix.is_none()
        {
            compute_ix = Some(ix as u32);
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

        if graphics_ix.is_some() && present_ix.is_some() && compute_ix.is_some()
        {
            break;
        }
    }

    if compute_ix.is_none() {
        compute_ix = graphics_ix;
    }

    Ok((graphics_ix, present_ix, compute_ix))
}

pub(super) fn device_supports_extensions(
    instance: &Instance,
    device: vk::PhysicalDevice,
) -> Result<bool> {
    let required_exts = required_device_extensions();

    let extension_props =
        unsafe { instance.enumerate_device_extension_properties(device) }?;

    let mut result = true;

    for req in required_exts.iter() {
        let found = extension_props.iter().any(|ext| {
            let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
            req == &name
        });

        result = found;

        if !found {
            error!("Device does not support extension {:?}", req);
        }
    }

    Ok(result)
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
    let (graphics_ix, present_ix, compute_ix) =
        find_queue_families(instance, surface, surface_khr, device)?;

    if graphics_ix.is_none() || present_ix.is_none() || compute_ix.is_none() {
        error!("Device is missing a queue family");
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
        error!("Swapchain inadequate");
        return Ok(false);
    }

    device_supports_features(instance, device)
}

pub(super) fn choose_physical_device(
    instance: &Instance,
    surface: &Surface,
    surface_khr: vk::SurfaceKHR,
    force_device: Option<&str>,
) -> Result<(vk::PhysicalDevice, u32, u32, u32)> {
    let devices = unsafe { instance.enumerate_physical_devices() }?;

    log::debug!("Enumerating physical devices");

    let (_, device) = if let Some(preferred_device) = force_device {
        log::warn!("Attempting to force use of device {}", preferred_device);

        let device_name = CString::new(preferred_device)?;

        let device = devices
            .into_iter()
            .enumerate()
            .find(|(_ix, dev)| {
                let name = unsafe {
                    let props = instance.get_physical_device_properties(*dev);
                    CStr::from_ptr(props.device_name.as_ptr())
                };
                (name == device_name.as_c_str())
                    && device_is_suitable(instance, surface, surface_khr, *dev)
                        .unwrap()
            })
            .expect("No suitable physical device found!");

        device
    } else {
        for (ix, device) in devices.iter().enumerate() {
            unsafe {
                let props = instance.get_physical_device_properties(*device);
                log::debug!(
                    "Device {} - {:?}",
                    ix,
                    CStr::from_ptr(props.device_name.as_ptr())
                );
            }
        }

        devices
            .into_iter()
            .enumerate()
            .find(|(_ix, dev)| {
                device_is_suitable(instance, surface, surface_khr, *dev)
                    .unwrap()
            })
            .expect("No suitable physical device found!")
    };

    let properties = unsafe { instance.get_physical_device_properties(device) };

    unsafe {
        info!(
            "Selected physical device: {:?}",
            CStr::from_ptr(properties.device_name.as_ptr())
        );
    }

    let (graphics_ix, present_ix, compute_ix) =
        find_queue_families(instance, surface, surface_khr, device)?;
    log::debug!(
        "Found queue families; graphics: {:?}, present: {:?}, compute: {:?}",
        graphics_ix,
        present_ix,
        compute_ix
    );

    Ok((
        device,
        graphics_ix.unwrap(),
        present_ix.unwrap(),
        compute_ix.unwrap(),
    ))
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

    if super::debug::ENABLE_VALIDATION_LAYERS {
        trace!(
            "Creating swapchain.\n\tFormat: {:?}\n\tColorSpace: {:?}\n\tPresentMode: {:?}\n\tExtent: {:?}\n\tImageCount: {:?}",
            props.format.format,
            props.format.color_space,
            props.present_mode,
            props.extent,
            image_count,
        );
    }

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
    compute_ix: u32,
) -> Result<(Device, vk::Queue, vk::Queue, vk::Queue)> {
    let queue_priorities = [1.0f32];

    let queue_infos = {
        use rustc_hash::FxHashSet;
        let indices = [graphics_ix, present_ix, compute_ix]
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
    let mut device_extensions_ptrs = device_extensions
        .iter()
        .map(|ext| ext.as_ptr())
        .collect::<Vec<_>>();

    device_extensions_ptrs.push(PushDescriptor::name().as_ptr());

    let available_features =
        unsafe { instance.get_physical_device_features(device) };

    let mut device_features = vk::PhysicalDeviceFeatures::builder()
        .sampler_anisotropy(true)
        .tessellation_shader(true)
        .independent_blend(true);

    if available_features.wide_lines == vk::TRUE {
        device_features = device_features.wide_lines(true);
    }

    let device_features = device_features.build();

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
    let compute_queue = unsafe { device.get_device_queue(compute_ix, 0) };

    Ok((device, graphics_queue, present_queue, compute_queue))
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

fn device_supports_features(
    instance: &Instance,
    device: vk::PhysicalDevice,
) -> Result<bool> {
    let features = unsafe { instance.get_physical_device_features(device) };

    let mut result = true;

    macro_rules! mandatory {
        ($path:tt) => {
            if features.$path == vk::FALSE {
                error!(
                    "Device is missing the mandatory feature: {}",
                    stringify!($path)
                );
                result = false;
            }
        };
    }

    macro_rules! optional {
        ($path:tt) => {
            if features.$path == vk::FALSE {
                warn!(
                    "Device is missing the optional feature: {}",
                    stringify!($path)
                );
            }
        };
    }

    mandatory!(sampler_anisotropy);
    mandatory!(tessellation_shader);
    mandatory!(independent_blend);

    // optional features
    optional!(wide_lines);

    Ok(result)
}
