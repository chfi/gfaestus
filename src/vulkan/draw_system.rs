use ash::version::DeviceV1_0;
use ash::{vk, Device};

use bytemuck::{Pod, Zeroable};

pub mod edges;
pub mod gui;
pub mod nodes;
pub mod post;
pub mod selection;

#[derive(Clone, Copy, Zeroable, Pod)]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 2],
}

impl Vertex {
    fn get_binding_desc() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    fn get_attribute_descs() -> [vk::VertexInputAttributeDescription; 1] {
        let pos_desc = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build();

        [pos_desc]
    }
}

pub(crate) fn create_shader_module(
    device: &Device,
    code: &[u32],
) -> vk::ShaderModule {
    let create_info = vk::ShaderModuleCreateInfo::builder().code(code).build();
    unsafe { device.create_shader_module(&create_info, None).unwrap() }
}
