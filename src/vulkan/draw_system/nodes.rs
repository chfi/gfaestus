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

use super::Vertex;
use super::{create_shader_module, read_shader_from_file};

pub struct NodeThemePipeline {
    descriptor_pool: vk::DescriptorPool,

    descriptor_set_layout: vk::DescriptorSetLayout,

    sampler: vk::Sampler,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    device: Device,
}

impl NodeThemePipeline {
    fn theme_layout_binding() -> vk::DescriptorSetLayoutBinding {
        use vk::ShaderStageFlags as Stages;

        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(Stages::FRAGMENT)
            .build()
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let binding = Self::theme_layout_binding();
        let bindings = [binding];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout =
            unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        Ok(layout)
    }

    fn create_pipeline(
        device: &Device,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        descriptor_set_layout: vk::DescriptorSetLayout,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        create_pipeline(
            device,
            msaa_samples,
            render_pass,
            descriptor_set_layout,
            "shaders/nodes_themed.frag.spv",
        )
    }
}

pub struct NodeOverlayPipeline {
    descriptor_pool: vk::DescriptorPool,

    descriptor_set_layout: vk::DescriptorSetLayout,

    sampler: vk::Sampler,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    device: Device,
}

impl NodeOverlayPipeline {
    fn overlay_layout_binding() -> vk::DescriptorSetLayoutBinding {
        use vk::ShaderStageFlags as Stages;

        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_TEXEL_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::FRAGMENT)
            .build()
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let binding = Self::overlay_layout_binding();
        let bindings = [binding];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout =
            unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        Ok(layout)
    }

    fn create_pipeline(
        device: &Device,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        descriptor_set_layout: vk::DescriptorSetLayout,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        create_pipeline(
            device,
            msaa_samples,
            render_pass,
            descriptor_set_layout,
            "shaders/nodes_overlay.frag.spv",
        )
    }
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

pub struct NodePushConstants {
    view_transform: glm::Mat4,
    node_width: f32,
    scale: f32,
    viewport_dims: [f32; 2],
    texture_period: u32,
}

impl NodePushConstants {
    #[inline]
    pub fn new(
        offset: [f32; 2],
        viewport_dims: [f32; 2],
        view: crate::view::View,
        node_width: f32,
        texture_period: u32,
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
            texture_period,
        }
    }

    #[inline]
    pub fn bytes(&self) -> [u8; 84] {
        use crate::view;

        let mut bytes = [0u8; 84];

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

        let u_bytes = self.texture_period.to_ne_bytes();
        let mut offset = 80;
        for i in 0..4 {
            bytes[offset] = u_bytes[i];
            offset += 1;
        }

        bytes
    }
}

fn create_pipeline(
    device: &Device,
    msaa_samples: vk::SampleCountFlags,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
    frag_shader_path: &str,
) -> (vk::Pipeline, vk::PipelineLayout) {
    let vert_src =
        read_shader_from_file("shaders/nodes_simple.vert.spv").unwrap();
    let geom_src =
        read_shader_from_file("shaders/nodes_simple.geom.spv").unwrap();
    let frag_src = read_shader_from_file(frag_shader_path).unwrap();

    let vert_module = create_shader_module(device, &vert_src);
    let geom_module = create_shader_module(device, &geom_src);
    let frag_module = create_shader_module(device, &frag_src);

    let entry_point = CString::new("main").unwrap();

    let vert_state_info = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::VERTEX)
        .module(vert_module)
        .name(&entry_point)
        .build();

    let geom_state_info = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::GEOMETRY)
        .module(geom_module)
        .name(&entry_point)
        .build();

    let frag_state_info = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(frag_module)
        .name(&entry_point)
        .build();

    let shader_state_infos =
        [vert_state_info, geom_state_info, frag_state_info];

    let vert_binding_descs = [Vertex::get_binding_desc()];
    let vert_attr_descs = Vertex::get_attribute_descs();
    let vert_input_info = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(&vert_binding_descs)
        .vertex_attribute_descriptions(&vert_attr_descs)
        .build();

    let input_assembly_info =
        vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::LINE_LIST)
            .primitive_restart_enable(false)
            .build();

    let viewport_info = vk::PipelineViewportStateCreateInfo::builder()
        .viewport_count(1)
        .scissor_count(1)
        .build();

    let dynamic_states = {
        use vk::DynamicState as DS;
        [DS::VIEWPORT, DS::SCISSOR]
    };

    let dynamic_state_info = vk::PipelineDynamicStateCreateInfo::builder()
        .dynamic_states(&dynamic_states)
        .build();

    let rasterizer_info = vk::PipelineRasterizationStateCreateInfo::builder()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false)
        .depth_bias_constant_factor(0.0)
        .depth_bias_clamp(0.0)
        .depth_bias_slope_factor(0.0)
        .build();

    // let depth_stencil_info = todo!();

    let multisampling_info = vk::PipelineMultisampleStateCreateInfo::builder()
        .sample_shading_enable(false)
        .rasterization_samples(msaa_samples)
        .min_sample_shading(1.0)
        .alpha_to_coverage_enable(false)
        .alpha_to_one_enable(false)
        .build();

    let color_blend_attachment =
        vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::all())
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ZERO)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build();
    let color_blend_attachments = [color_blend_attachment];

    let color_blending_info = vk::PipelineColorBlendStateCreateInfo::builder()
        .logic_op_enable(false)
        .logic_op(vk::LogicOp::COPY)
        .attachments(&color_blend_attachments)
        .blend_constants([0.0, 0.0, 0.0, 0.0])
        .build();

    let layout = {
        use vk::ShaderStageFlags as Flags;

        let layouts = [descriptor_set_layout];

        let pc_range = vk::PushConstantRange::builder()
            .stage_flags(Flags::VERTEX | Flags::GEOMETRY | Flags::FRAGMENT)
            .offset(0)
            .size(80)
            .build();

        let pc_ranges = [pc_range];

        let layout_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&layouts)
            .push_constant_ranges(&pc_ranges)
            .build();

        unsafe { device.create_pipeline_layout(&layout_info, None).unwrap() }
    };

    let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
        .stages(&shader_state_infos)
        .vertex_input_state(&vert_input_info)
        .input_assembly_state(&input_assembly_info)
        .viewport_state(&viewport_info)
        .dynamic_state(&dynamic_state_info)
        .rasterization_state(&rasterizer_info)
        .multisample_state(&multisampling_info)
        .color_blend_state(&color_blending_info)
        .layout(layout)
        .render_pass(render_pass)
        .subpass(0)
        .build();

    let pipeline_infos = [pipeline_info];

    let pipeline = unsafe {
        device
            .create_graphics_pipelines(
                vk::PipelineCache::null(),
                &pipeline_infos,
                None,
            )
            .unwrap()[0]
    };

    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(geom_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    (pipeline, layout)
}
