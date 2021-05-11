use ash::version::DeviceV1_0;
use ash::{vk, Device};

use std::ffi::CString;

use nalgebra_glm as glm;

use anyhow::Result;

use handlegraph::handle::NodeId;

use crate::geometry::Point;
use crate::view::View;
use crate::vulkan::GfaestusVk;

use crate::vulkan::draw_system::nodes::NodeOverlay;
