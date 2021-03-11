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
