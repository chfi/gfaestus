use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;

pub mod gui;
pub mod nodes;
pub mod selection;

#[derive(Clone, Copy)]
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

#[macro_export]
macro_rules! load_shader {
    ($path:literal) => {{
        let buf = crate::include_shader!($path);
        let mut cursor = std::io::Cursor::new(buf);
        ash::util::read_spv(&mut cursor).unwrap()
    }};
}

pub(crate) fn read_shader_from_file<P>(path: P) -> Result<Vec<u32>>
where
    P: AsRef<std::path::Path>,
{
    use std::{fs::File, io::Read};

    let mut buf = Vec::new();
    let mut file = File::open(path)?;
    file.read_to_end(&mut buf)?;

    let mut cursor = std::io::Cursor::new(buf);

    let spv = ash::util::read_spv(&mut cursor)?;
    Ok(spv)
}

pub(crate) fn create_shader_module(
    device: &Device,
    code: &[u32],
) -> vk::ShaderModule {
    let create_info = vk::ShaderModuleCreateInfo::builder().code(code).build();
    unsafe { device.create_shader_module(&create_info, None).unwrap() }
}
