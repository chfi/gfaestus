use ash::version::DeviceV1_0;
use ash::vk::PipelineShaderStageCreateInfo;
use ash::{vk, Device};
use handlegraph::handle::NodeId;
use rustc_hash::FxHashSet;

use std::{ffi::CString, ops::RangeInclusive};

use nalgebra_glm as glm;

use anyhow::*;

use crate::view::View;
use crate::vulkan::GfaestusVk;
use crate::{
    geometry::Point, overlays::OverlayKind, vulkan::texture::GradientTexture,
};

use crate::vulkan::render_pass::Framebuffers;

use super::super::{create_shader_module, Vertex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineKind {
    OverlayRgb,
    OverlayU, // instead of "value"
              // OverlayUv, // might add later
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodePipelineConfig {
    pub tessellation: bool,

    pub kind: PipelineKind,
    // frag_shader:
    // geometry: bool,
}

impl NodePipelineConfig {
    fn shader_modules(
        &self,
        device: &Device,
    ) -> Result<Vec<PipelineShaderStageCreateInfo>> {
        // TODO pick which vertex shader to use; different for no-tess version
        let vert_src = crate::load_shader!("nodes/base.vert.spv");

        let tesc_src = crate::load_shader!("nodes/base.tesc.spv");
        let tese_src = crate::load_shader!("nodes/base.tese.spv");

        let frag_src = match self.kind {
            PipelineKind::OverlayRgb => {
                crate::load_shader!("nodes/overlay_rgb.frag.spv")
            }

            PipelineKind::OverlayU => {
                crate::load_shader!("nodes/overlay_value.frag.spv")
            }
        };

        let vert_module = create_shader_module(device, &vert_src);
        let tesc_module = create_shader_module(device, &tesc_src);
        let tese_module = create_shader_module(device, &tese_src);
        let frag_module = create_shader_module(device, &frag_src);

        let entry_point = CString::new("main").unwrap();

        let vert_state_info = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(&entry_point)
            .build();

        let tesc_state_info = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::TESSELLATION_CONTROL)
            .module(tesc_module)
            .name(&entry_point)
            .build();

        let tese_state_info = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::TESSELLATION_EVALUATION)
            .module(tese_module)
            .name(&entry_point)
            .build();

        let frag_state_info = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(&entry_point)
            .build();

        let shader_state_infos = vec![
            vert_state_info,
            tesc_state_info,
            tese_state_info,
            frag_state_info,
        ];

        Ok(shader_state_infos)
    }
}

/*
fn create_node_pipeline(device: &Device,
                        msaa_samples: vk::SampleCountFlags,
                        render_pass: vk::RenderPass,
                        layouts: &[vk::DescriptorSetLayout],
                        frag_shader: &[u32],
*/

pub(super) fn create_tess_pipeline(
    device: &Device,
    msaa_samples: vk::SampleCountFlags,
    render_pass: vk::RenderPass,
    layouts: &[vk::DescriptorSetLayout],
    frag_shader: &[u8],
) -> (vk::Pipeline, vk::PipelineLayout) {
    let vert_src = crate::load_shader!("nodes/base.vert.spv");
    let tesc_src = crate::load_shader!("nodes/base.tesc.spv");
    let tese_src = crate::load_shader!("nodes/base.tese.spv");
    let frag_src = {
        let mut cursor = std::io::Cursor::new(frag_shader);
        ash::util::read_spv(&mut cursor).unwrap()
    };

    let vert_module = create_shader_module(device, &vert_src);
    let tesc_module = create_shader_module(device, &tesc_src);
    let tese_module = create_shader_module(device, &tese_src);
    let frag_module = create_shader_module(device, &frag_src);

    let entry_point = CString::new("main").unwrap();

    let vert_state_info = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::VERTEX)
        .module(vert_module)
        .name(&entry_point)
        .build();

    let tesc_state_info = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::TESSELLATION_CONTROL)
        .module(tesc_module)
        .name(&entry_point)
        .build();

    let tese_state_info = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::TESSELLATION_EVALUATION)
        .module(tese_module)
        .name(&entry_point)
        .build();

    let frag_state_info = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(frag_module)
        .name(&entry_point)
        .build();

    let shader_state_infos = [
        vert_state_info,
        tesc_state_info,
        tese_state_info,
        frag_state_info,
    ];

    let vert_binding_descs = [Vertex::get_binding_desc()];
    let vert_attr_descs = Vertex::get_attribute_descs();
    let vert_input_info = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(&vert_binding_descs)
        .vertex_attribute_descriptions(&vert_attr_descs)
        .build();

    let input_assembly_info =
        vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::PATCH_LIST)
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

    let tessellation_state_info =
        vk::PipelineTessellationStateCreateInfo::builder()
            .patch_control_points(2)
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

    let multisampling_info = vk::PipelineMultisampleStateCreateInfo::builder()
        .sample_shading_enable(false)
        .rasterization_samples(msaa_samples)
        .min_sample_shading(1.0)
        .alpha_to_coverage_enable(true)
        .alpha_to_one_enable(false)
        .build();

    let color_blend_attachment =
        vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::all())
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build();

    let id_color_blend_attachment =
        vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::R)
            .blend_enable(false)
            .build();

    let mask_color_blend_attachment =
        vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::all())
            .blend_enable(false)
            .build();

    let color_blend_attachments = [
        color_blend_attachment,
        id_color_blend_attachment,
        mask_color_blend_attachment,
    ];

    let color_blending_info = vk::PipelineColorBlendStateCreateInfo::builder()
        .logic_op_enable(false)
        .logic_op(vk::LogicOp::COPY)
        .attachments(&color_blend_attachments)
        .blend_constants([0.0, 0.0, 0.0, 0.0])
        .build();

    let layout = {
        use vk::ShaderStageFlags as Flags;

        let pc_range = vk::PushConstantRange::builder()
            .stage_flags(
                Flags::VERTEX
                    | Flags::TESSELLATION_CONTROL
                    | Flags::TESSELLATION_EVALUATION
                    | Flags::FRAGMENT,
            )
            .offset(0)
            .size(84)
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
        .tessellation_state(&tessellation_state_info)
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
        device.destroy_shader_module(tesc_module, None);
        device.destroy_shader_module(tese_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    (pipeline, layout)
}

pub(super) fn create_sampler(device: &Device) -> Result<vk::Sampler> {
    let sampler_info = vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::NEAREST)
        .min_filter(vk::Filter::NEAREST)
        .address_mode_u(vk::SamplerAddressMode::REPEAT)
        .address_mode_v(vk::SamplerAddressMode::REPEAT)
        .address_mode_w(vk::SamplerAddressMode::REPEAT)
        .anisotropy_enable(false)
        .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
        .unnormalized_coordinates(false)
        .compare_enable(false)
        .compare_op(vk::CompareOp::ALWAYS)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .mip_lod_bias(0.0)
        .min_lod(0.0)
        .max_lod(1.0)
        .build();

    let sampler = unsafe { device.create_sampler(&sampler_info, None) }?;

    Ok(sampler)
}
