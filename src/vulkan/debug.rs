use ash::version::EntryV1_0;
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
    _user_data: *mut c_void,
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

    let _queue_labels = {
        let queue_label_count = (*callback_data).queue_label_count as usize;
        dbg!(queue_label_count);
        let ptr = (*callback_data).p_queue_labels;
        std::slice::from_raw_parts(ptr, queue_label_count)
    };

    let cmd_buf_labels = {
        let cmd_buf_label_count = (*callback_data).cmd_buf_label_count as usize;
        dbg!(cmd_buf_label_count);
        let ptr = (*callback_data).p_cmd_buf_labels;
        std::slice::from_raw_parts(ptr, cmd_buf_label_count)
    };

    let objects = {
        let object_count = (*callback_data).object_count as usize;
        dbg!(object_count);
        let ptr = (*callback_data).p_objects;
        std::slice::from_raw_parts(ptr, object_count)
    };

    let p_msg_id_str = if p_message_id.is_null() {
        "0".to_string()
    } else {
        format!("{:?}", CStr::from_ptr(p_message_id))
    };

    let p_msg_str = if p_message.is_null() {
        "-".to_string()
    } else {
        format!("{:?}", CStr::from_ptr(p_message))
    };

    let mut message_string =
        format!("{} - {:?} - {}", p_msg_id_str, msg_type, p_msg_str,);

    dbg!();
    if !cmd_buf_labels.is_empty() {
        message_string.push_str("\n  Command buffers: ");
        for cmd_buf in cmd_buf_labels {
            if !cmd_buf.p_label_name.is_null() {
                message_string.push_str(&format!(
                    "{:?}",
                    CStr::from_ptr(cmd_buf.p_label_name)
                ));
            }
        }

        message_string.push_str("\n");
    }

    dbg!();
    if !objects.is_empty() {
        message_string.push_str("\n  Objects: ");
        for obj in objects {
            if obj.p_object_name.is_null() {
                message_string.push_str(&format!(
                    "   {:?} - no name\n",
                    obj.object_handle,
                ));
            } else {
                message_string.push_str(&format!(
                    "   {:?} - {:?}\n",
                    obj.object_handle,
                    CStr::from_ptr(obj.p_object_name),
                ));
            }
        }
    }

    dbg!();

    match msg_severity {
        MsgSeverity::VERBOSE => {
            debug!("{}", message_string);
            // debug!(
            //     "{:?} - {:?} - {:?}",
            //     CStr::from_ptr(p_message_id),
            //     msg_type,
            //     CStr::from_ptr(p_message)
            // );
        }
        MsgSeverity::INFO => {
            info!("{}", message_string);
            //     "{:?} - {:?} - {:?}",
            //     CStr::from_ptr(p_message_id),
            //     msg_type,
            //     CStr::from_ptr(p_message)
            // );
        }
        MsgSeverity::WARNING => {
            warn!("{}", message_string);
            //     "{:?} - {:?} - {:?}",
            //     CStr::from_ptr(p_message_id),
            //     msg_type,
            //     CStr::from_ptr(p_message)
            // );
        }
        MsgSeverity::ERROR => {
            error!("{}", message_string);
            //     "{:?} - {:?} - {:?}",
            //     CStr::from_ptr(p_message_id),
            //     msg_type,
            //     CStr::from_ptr(p_message)
            // );
        }
        _ => {
            error!("{}", message_string);
            //     "{:?} - {:?} - {:?}",
            //     CStr::from_ptr(p_message_id),
            //     msg_type,
            //     CStr::from_ptr(p_message)
            // );
        }
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

/// Setup the DebugUtils messenger if validation layers are enabled.
pub fn setup_debug_utils(
    entry: &Entry,
    instance: &Instance,
) -> Option<(DebugUtils, vk::DebugUtilsMessengerEXT)> {
    if !ENABLE_VALIDATION_LAYERS {
        return None;
    }

    let severity = {
        use vk::DebugUtilsMessageSeverityFlagsEXT as Severity;
        // TODO use the flexi_logger configuration here
        Severity::all()
    };

    let types = {
        use vk::DebugUtilsMessageTypeFlagsEXT as Type;
        // TODO maybe some customization here too
        Type::all()
    };

    let create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
        .message_severity(severity)
        .message_type(types)
        .pfn_user_callback(Some(vulkan_debug_utils_callback))
        .build();

    let debug_utils = DebugUtils::new(entry, instance);

    // TODO this should probably return Result, but i need to handle
    // the return at the top of this function first
    let messenger = unsafe {
        debug_utils
            .create_debug_utils_messenger(&create_info, None)
            .ok()
    }?;

    Some((debug_utils, messenger))
}
