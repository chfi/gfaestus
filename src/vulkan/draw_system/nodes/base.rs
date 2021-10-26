use ash::version::DeviceV1_0;
use ash::vk::PipelineShaderStageCreateInfo;
use ash::{vk, Device};

use std::ffi::CString;

use anyhow::*;

use super::super::{create_shader_module, Vertex};
use crate::vulkan::context::NodeRendererType;
use crate::vulkan::GfaestusVk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineKind {
    OverlayRgb,
    // instead of "value"
    OverlayU,
    // might add later:
    // OverlayUv,
}

pub struct NodePipelineConfig {
    pub kind: PipelineKind,
}

impl NodePipelineConfig {
    fn stage_create_info(
        &self,
        renderer_type: NodeRendererType,
        device: &Device,
        entry_point: &std::ffi::CStr,
    ) -> Result<Vec<PipelineShaderStageCreateInfo>> {
        let vert_src = match renderer_type {
            NodeRendererType::VertexOnly => {
                crate::load_shader!("nodes/quad.vert.spv")
            }
            NodeRendererType::TessellationQuads => {
                crate::load_shader!("nodes/base.vert.spv")
            }
        };

        let frag_src = match self.kind {
            PipelineKind::OverlayRgb => {
                crate::load_shader!("nodes/overlay_rgb.frag.spv")
            }

            PipelineKind::OverlayU => {
                crate::load_shader!("nodes/overlay_value.frag.spv")
            }
        };

        let vert_module = create_shader_module(device, &vert_src);
        let frag_module = create_shader_module(device, &frag_src);

        let vert_state_info = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point)
            .build();

        let frag_state_info = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point)
            .build();

        if matches!(renderer_type, NodeRendererType::TessellationQuads) {
            let tesc_src = crate::load_shader!("nodes/base.tesc.spv");
            let tese_src = crate::load_shader!("nodes/base.tese.spv");

            let tesc_module = create_shader_module(device, &tesc_src);
            let tese_module = create_shader_module(device, &tese_src);

            let tesc_state_info = vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::TESSELLATION_CONTROL)
                .module(tesc_module)
                .name(entry_point)
                .build();

            let tese_state_info = vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::TESSELLATION_EVALUATION)
                .module(tese_module)
                .name(entry_point)
                .build();

            let shader_state_infos = vec![
                vert_state_info,
                tesc_state_info,
                tese_state_info,
                frag_state_info,
            ];
            Ok(shader_state_infos)
        } else {
            let shader_state_infos = vec![vert_state_info, frag_state_info];
            Ok(shader_state_infos)
        }
    }
}

pub(crate) fn create_node_pipeline(
    app: &GfaestusVk,
    renderer_type: NodeRendererType,
    pipeline_config: NodePipelineConfig,
    layouts: &[vk::DescriptorSetLayout],
) -> Result<(vk::Pipeline, vk::PipelineLayout)> {
    let msaa_samples = app.msaa_samples;
    let render_pass = app.render_passes.nodes;

    let device = app.vk_context().device();

    let entry_point = CString::new("main").unwrap();

    let shader_stages_create_infos = pipeline_config.stage_create_info(
        renderer_type,
        device,
        &entry_point,
    )?;

    let vert_binding_descs = [Vertex::get_binding_desc()];
    let vert_attr_descs = Vertex::get_attribute_descs();
    let vert_input_info = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(&vert_binding_descs)
        .vertex_attribute_descriptions(&vert_attr_descs)
        .build();

    let input_assembly_info = {
        let topology =
            if matches!(renderer_type, NodeRendererType::TessellationQuads) {
                vk::PrimitiveTopology::PATCH_LIST
            } else {
                vk::PrimitiveTopology::TRIANGLE_LIST
            };
        vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(topology)
            .primitive_restart_enable(false)
            .build()
    };

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

        let stage_flags =
            if matches!(renderer_type, NodeRendererType::TessellationQuads) {
                Flags::VERTEX
                    | Flags::TESSELLATION_CONTROL
                    | Flags::TESSELLATION_EVALUATION
                    | Flags::FRAGMENT
            } else {
                Flags::VERTEX | Flags::FRAGMENT
            };

        let pc_range = vk::PushConstantRange::builder()
            .stage_flags(stage_flags)
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

    let mut pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
        .stages(&shader_stages_create_infos)
        .vertex_input_state(&vert_input_info)
        .input_assembly_state(&input_assembly_info)
        .viewport_state(&viewport_info)
        .dynamic_state(&dynamic_state_info)
        .rasterization_state(&rasterizer_info)
        .multisample_state(&multisampling_info)
        .color_blend_state(&color_blending_info)
        .layout(layout)
        .render_pass(render_pass)
        .subpass(0);

    // only used if tessellation active, but define it anyway
    let tessellation_state_info =
        vk::PipelineTessellationStateCreateInfo::builder()
            .patch_control_points(2)
            .build();

    if matches!(renderer_type, NodeRendererType::TessellationQuads) {
        pipeline_info =
            pipeline_info.tessellation_state(&tessellation_state_info);
    }

    let pipeline_info = pipeline_info.build();

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
        for stage in shader_stages_create_infos {
            device.destroy_shader_module(stage.module, None);
        }
    }

    Ok((pipeline, layout))
}
