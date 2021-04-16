use ash::version::DeviceV1_0;
use ash::{vk, Device};

use std::ffi::CString;

use nalgebra_glm as glm;

use anyhow::Result;

use crate::geometry::Point;
use crate::view::View;
use crate::vulkan::GfaestusVk;

use crate::vulkan::draw_system::nodes::NodeOverlay;

pub struct SnarlOverlay {
    overlay: NodeOverlay,

    colors: Vec<rgb::RGB<f32>>,

    default_color: rgb::RGB<f32>,

    snarls: Vec<(u32, u32)>,
}

impl SnarlOverlay {
    pub fn new(
        app: &GfaestusVk,
        pool: vk::DescriptorPool,
        layout: vk::DescriptorSetLayout,
        node_count: usize,
    ) -> Result<Self> {
        let default_color = rgb::RGB::new(0.3, 0.3, 0.3);

        let colors = vec![
            rgb::RGB::new(1.0, 0.0, 0.0),
            rgb::RGB::new(1.0, 0.65, 0.0),
            rgb::RGB::new(1.0, 1.0, 0.0),
            rgb::RGB::new(0.0, 0.5, 0.0),
            rgb::RGB::new(0.0, 0.0, 1.0),
            rgb::RGB::new(0.3, 0.0, 0.51),
            rgb::RGB::new(0.93, 0.51, 0.93),
        ];

        let snarls: Vec<(u32, u32)> = Vec::new();

        let overlay = NodeOverlay::new_empty(app, pool, layout, node_count)?;

        Ok(Self {
            overlay,

            colors,
            default_color,

            snarls,
        })
    }
}
