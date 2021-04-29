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

pub struct SnarlOverlay {
    overlay: NodeOverlay,

    colors: Vec<rgb::RGB<f32>>,

    default_color: rgb::RGB<f32>,

    snarls: Vec<(u32, u32)>,
}

impl SnarlOverlay {
    pub fn new(app: &GfaestusVk, node_count: usize) -> Result<Self> {
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

        let mut overlay = NodeOverlay::new_empty("snarl_overlay", app, node_count)?;

        let node_colors = (0..node_count)
            .into_iter()
            .map(|x| (NodeId::from((x + 1) as usize), default_color));

        overlay.update_overlay(app.vk_context().device(), node_colors)?;

        Ok(Self {
            overlay,

            colors,
            default_color,

            snarls,
        })
    }

    pub fn add_snarl(&mut self, device: &Device, snarl: (NodeId, NodeId)) -> Result<()> {
        let next_ix = self.snarls.len();

        let color = self.colors[next_ix % self.colors.len()];

        let new_colors = vec![(snarl.0, color), (snarl.1, color)];

        self.overlay.update_overlay(device, new_colors)
    }

    pub fn into_overlay(self) -> NodeOverlay {
        self.overlay
    }
}
