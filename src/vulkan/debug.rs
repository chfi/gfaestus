use ash::{extensions::ext::DebugReport, version::EntryV1_0};
use ash::{vk, Entry, Instance};
use std::{
    ffi::{CStr, CString},
    os::raw::{c_char, c_void},
};

use ash::extensions::ext::DebugUtils;

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

#[cfg(debug_assertions)]
pub const ENABLE_VALIDATION_LAYERS: bool = true;
#[cfg(not(debug_assertions))]
pub const ENABLE_VALIDATION_LAYERS: bool = false;

const REQUIRED_LAYERS: [&str; 1] = ["VK_LAYER_KHRONOS_validation"];

unsafe extern "system" fn vulkan_debug_utils_callback(
    msg_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    msg_type: vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    user_data: *mut c_void,
) -> u32 {
    use vk::DebugUtilsMessageSeverityFlagsEXT as MsgSeverity;
    use vk::DebugUtilsMessageTypeFlagsEXT as MsgType;

    // might be better to use the ordering like this, but i'll fix
    // that later if it's worthwhile

    // if msg_severity <= MsgSeverity::VERBOSE {
    // } else if msg_severity <= MsgSeverity::INFO {
    // } else if msg_severity <= MsgSeverity::WARNING {
    // } else if msg_severity <= MsgSeverity::ERROR {
    // }

    let p_message_id = (*callback_data).p_message_id_name as *const c_char;
    let p_message = (*callback_data).p_message as *const c_char;

    match msg_severity {
        MsgSeverity::VERBOSE => {
            debug!(
                "{:?} - {:?} - {:?}",
                CStr::from_ptr(p_message_id),
                msg_type,
                CStr::from_ptr(p_message)
            );
        }
        MsgSeverity::INFO => {
            info!(
                "{:?} - {:?} - {:?}",
                CStr::from_ptr(p_message_id),
                msg_type,
                CStr::from_ptr(p_message)
            );
        }
        MsgSeverity::WARNING => {
            warn!(
                "{:?} - {:?} - {:?}",
                CStr::from_ptr(p_message_id),
                msg_type,
                CStr::from_ptr(p_message)
            );
        }
        MsgSeverity::ERROR => {
            error!(
                "{:?} - {:?} - {:?}",
                CStr::from_ptr(p_message_id),
                msg_type,
                CStr::from_ptr(p_message)
            );
        }
        _ => {
            error!(
                "{:?} - {:?} - {:?}",
                CStr::from_ptr(p_message_id),
                msg_type,
                CStr::from_ptr(p_message)
            );
        }
    }

    //
    // match msg_severity {
    // }

    vk::FALSE
}

unsafe extern "system" fn vulkan_debug_callback(
    flag: vk::DebugReportFlagsEXT,
    typ: vk::DebugReportObjectTypeEXT,
    _: u64,
    _: usize,
    _: i32,
    _: *const c_char,
    p_message: *const c_char,
    _: *mut c_void,
) -> u32 {
    if flag == vk::DebugReportFlagsEXT::DEBUG {
        debug!("{:?} - {:?}", typ, CStr::from_ptr(p_message));
    } else if flag == vk::DebugReportFlagsEXT::INFORMATION {
        info!("{:?} - {:?}", typ, CStr::from_ptr(p_message));
    } else if flag == vk::DebugReportFlagsEXT::WARNING {
        warn!("{:?} - {:?}", typ, CStr::from_ptr(p_message));
    } else if flag == vk::DebugReportFlagsEXT::PERFORMANCE_WARNING {
        warn!("{:?} - {:?}", typ, CStr::from_ptr(p_message));
    } else {
        info!("{:?} - {:?}", typ, CStr::from_ptr(p_message));
    }
    vk::FALSE
}

/// Get the pointers to the validation layers names.
/// Also return the corresponding `CString` to avoid dangling pointers.
pub fn get_layer_names_and_pointers() -> (Vec<CString>, Vec<*const c_char>) {
    let layer_names = REQUIRED_LAYERS
        .iter()
        .map(|name| CString::new(*name).unwrap())
        .collect::<Vec<_>>();
    let layer_names_ptrs = layer_names
        .iter()
        .map(|name| name.as_ptr())
        .collect::<Vec<_>>();
    (layer_names, layer_names_ptrs)
}

/// Check if the required validation set in `REQUIRED_LAYERS`
/// are supported by the Vulkan instance.
///
/// # Panics
///
/// Panic if at least one on the layer is not supported.
pub fn check_validation_layer_support(entry: &Entry) {
    for required in REQUIRED_LAYERS.iter() {
        let found = entry
            .enumerate_instance_layer_properties()
            .unwrap()
            .iter()
            .any(|layer| {
                let name = unsafe { CStr::from_ptr(layer.layer_name.as_ptr()) };
                let name =
                    name.to_str().expect("Failed to get layer name pointer");
                required == &name
            });

        if !found {
            panic!("Validation layer not supported: {}", required);
        }
    }
}

/// Setup the debug message if validation layers are enabled.
pub fn setup_debug_messenger(
    entry: &Entry,
    instance: &Instance,
) -> Option<(DebugReport, vk::DebugReportCallbackEXT)> {
    if !ENABLE_VALIDATION_LAYERS {
        return None;
    }
    let create_info = vk::DebugReportCallbackCreateInfoEXT::builder()
        .flags(vk::DebugReportFlagsEXT::all())
        .pfn_callback(Some(vulkan_debug_callback))
        .build();
    let debug_report = DebugReport::new(entry, instance);
    let debug_report_callback = unsafe {
        debug_report
            .create_debug_report_callback(&create_info, None)
            .unwrap()
    };
    Some((debug_report, debug_report_callback))
}
