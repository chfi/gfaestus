use ash::{
    extensions::{
        ext::DebugReport,
        khr::{Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
};
use ash::{vk, Device, Entry, Instance};

use nalgebra_glm as glm;

use anyhow::Result;

pub struct NodeDrawAsh {
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    vertex_buffer: vk::Buffer,
    vertex_buffer_memory: vk::DeviceMemory,

    // uniform_buffer: vk::Buffer,
    // uniform_buffer_memory: vk::DeviceMemory,
    descriptor_set: vk::DescriptorSet,
}

// pub struct NodesUBO {
//     matrix: glm::Mat4,
// }

impl NodeDrawAsh {
    pub fn new(
        desc_pool: vk::DescriptorPool,
        render_pass: vk::RenderPass,
    ) -> Result<Self> {
        unimplemented!();
    }

    fn descriptor_set_layout(device: &Device) -> vk::DescriptorSetLayout {
        // let ubo_binding = Unif

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&[])
            .build();

        unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .unwrap()
        }
    }
}
