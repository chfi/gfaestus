use ash::{
    extensions::{
        ext::DebugReport,
        khr::{Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
};
use ash::{vk, Device, Entry, Instance};

use std::ffi::CString;

use std::sync::{Arc, Weak};

use nalgebra_glm as glm;

use anyhow::Result;

use super::SwapchainProperties;

use crate::geometry::Point;
use crate::view::View;

pub mod gui;
pub mod nodes;
pub mod selection;

use nodes::*;

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

fn read_shader_from_file<P>(path: P) -> Result<Vec<u32>>
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

fn create_shader_module(device: &Device, code: &[u32]) -> vk::ShaderModule {
    let create_info = vk::ShaderModuleCreateInfo::builder().code(code).build();
    unsafe { device.create_shader_module(&create_info, None).unwrap() }
}
