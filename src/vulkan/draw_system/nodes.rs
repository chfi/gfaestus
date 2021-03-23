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

use crate::geometry::Point;
use crate::view::View;
use crate::vulkan::SwapchainProperties;

pub struct NodeThemePipeline {
    descriptor_pool: vk::DescriptorPool,

    descriptor_set_layout: vk::DescriptorSetLayout,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    device: Device,
}

pub struct NodeOverlayPipeline {
    descriptor_pool: vk::DescriptorPool,

    descriptor_set_layout: vk::DescriptorSetLayout,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    device: Device,
}

pub struct NodePushConstants {
    view_transform: glm::Mat4,
    node_width: f32,
    scale: f32,
    viewport_dims: [f32; 2],
}

pub struct NodeVertices {
    vertex_count: usize,
    vertex_buffer: vk::Buffer,
    vertex_memory: vk::DeviceMemory,

    device: Device,
}

pub struct NodePipelines {
    theme_pipeline: NodeThemePipeline,
    overlay_pipeline: NodeOverlayPipeline,

    vertices: NodeVertices,
}

impl NodePushConstants {
    #[inline]
    pub fn new(
        offset: [f32; 2],
        viewport_dims: [f32; 2],
        view: crate::view::View,
        node_width: f32,
    ) -> Self {
        use crate::view;

        let model_mat = glm::mat4(
            1.0, 0.0, 0.0, offset[0], 0.0, 1.0, 0.0, offset[1], 0.0, 0.0, 1.0,
            0.0, 0.0, 0.0, 0.0, 1.0,
        );

        let view_mat = view.to_scaled_matrix();

        let width = viewport_dims[0];
        let height = viewport_dims[1];

        let viewport_mat = view::viewport_scale(width, height);

        let matrix = viewport_mat * view_mat * model_mat;

        Self {
            view_transform: matrix,
            node_width,
            viewport_dims,
            scale: view.scale,
        }
    }

    #[inline]
    pub fn bytes(&self) -> [u8; 80] {
        use crate::view;

        let mut bytes = [0u8; 80];

        let view_transform_array = view::mat4_to_array(&self.view_transform);

        {
            let mut offset = 0;

            let mut add_float = |f: f32| {
                let f_bytes = f.to_ne_bytes();
                for i in 0..4 {
                    bytes[offset] = f_bytes[i];
                    offset += 1;
                }
            };

            for i in 0..4 {
                let row = view_transform_array[i];
                for j in 0..4 {
                    let val = row[j];
                    add_float(val);
                }
            }

            add_float(self.node_width);
            add_float(self.scale);

            add_float(self.viewport_dims[0]);
            add_float(self.viewport_dims[1]);
        }

        bytes
    }
}
