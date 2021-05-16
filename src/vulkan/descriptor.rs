use ash::{
    extensions::{
        ext::DebugReport,
        khr::{Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk::SurfaceKHR,
};
use ash::{vk, Device, Entry};

use anyhow::Result;

pub trait Descriptor {
    const DESC_TYPE: vk::DescriptorType;
}

pub trait DescriptorSet {
    fn create_descriptor_pool(device: &Device) -> Result<vk::DescriptorPool>;

    fn create_descriptor_set(
        device: &Device,
        pool: vk::DescriptorPool,
    ) -> Result<vk::DescriptorSet>;
}
