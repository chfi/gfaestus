use crate::{
    geometry::{Point, Rect},
    view::{ScreenDims, View},
    vulkan::tiles::ScreenTiles,
};

use handlegraph::{
    handle::{Direction, Edge, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use handlegraph::{
    packedgraph::{paths::StepPtr, PackedGraph},
    path_position::PathPositionMap,
};

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;

use nalgebra_glm as glm;

use std::{ffi::CString, ops::RangeInclusive};

use super::create_shader_module;
use super::Vertex;

use crate::app::node_flags::SelectionBuffer;

use super::nodes::NodePushConstants;
use crate::vulkan::render_pass::Framebuffers;
use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

pub struct EdgeRenderer2 {
    /*
    pub(crate) descriptor_pool: vk::DescriptorPool,
    pub(crate) descriptor_set_layout: vk::DescriptorSetLayout,
    pub(crate) descriptor_set: vk::DescriptorSet,
    */
    pub(crate) pipeline_layout: vk::PipelineLayout,
    pub(crate) pipeline: vk::Pipeline,

    pub(crate) device: Device,
    pub(crate) edge_index_buffer: EdgeIndices,
}

impl EdgeRenderer2 {
    fn layout_binding() -> vk::DescriptorSetLayoutBinding {
        unimplemented!();
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let bindings = [];

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
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        let vert_src = crate::load_shader!("edges/edges.vert.spv");
        let tesc_src = crate::load_shader!("edges/edges.tesc.spv");
        let tese_src = crate::load_shader!("edges/edges.tese.spv");
        let frag_src = crate::load_shader!("edges/edges.frag.spv");

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

        let rasterizer_info =
            vk::PipelineRasterizationStateCreateInfo::builder()
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

        let multisampling_info =
            vk::PipelineMultisampleStateCreateInfo::builder()
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

        let color_blend_attachments = [color_blend_attachment];

        let color_blending_info =
            vk::PipelineColorBlendStateCreateInfo::builder()
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

            let layouts = [];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe {
                device.create_pipeline_layout(&layout_info, None).unwrap()
            }
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

    pub fn new(
        app: &GfaestusVk,
        graph: &PackedGraph,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
    ) -> Result<Self> {
        let device = app.vk_context().device();
        dbg!();

        /*
        let desc_set_layout = Self::create_descriptor_set_layout(device)?;
        */

        dbg!();
        let (pipeline, pipeline_layout) =
            Self::create_pipeline(device, msaa_samples, render_pass);

        dbg!();
        let edge_index_buffer = EdgeIndices::new(app, graph)?;

        Ok(Self {
            pipeline_layout,
            pipeline,

            edge_index_buffer,
            device: device.clone(),
        })
    }

    pub fn destroy(&mut self) {
        unsafe {
            // self.device.destroy_descriptor_set_layout(
            //     self.descriptor_set_layout,
            //     None,
            // );

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);

            // self.device
            //     .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }

    pub fn draw(
        &self,
        cmd_buf: vk::CommandBuffer,
        vertices: &NodeVertices,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        viewport_dims: [f32; 2],
        node_width: f32,
        view: View,
        offset: Point,
    ) -> Result<()> {
        let device = &self.device;

        let extent = vk::Extent2D {
            width: viewport_dims[0] as u32,
            height: viewport_dims[1] as u32,
        };

        let clear_values = [];

        let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(render_pass)
            .framebuffer(framebuffers.edges)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            })
            .clear_values(&clear_values)
            .build();

        unsafe {
            device.cmd_begin_render_pass(
                cmd_buf,
                &render_pass_begin_info,
                vk::SubpassContents::INLINE,
            )
        };

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            )
        };

        let vx_bufs = [vertices.vertex_buffer];

        let offsets = [0];
        unsafe {
            device.cmd_bind_vertex_buffers(cmd_buf, 0, &vx_bufs, &offsets);

            device.cmd_bind_index_buffer(
                cmd_buf,
                self.edge_index_buffer.buffer,
                0,
                vk::IndexType::UINT32,
            );
        };

        let push_constants = NodePushConstants::new(
            [offset.x, offset.y],
            viewport_dims,
            view,
            node_width,
            7,
        );

        let pc_bytes = push_constants.bytes();

        unsafe {
            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.pipeline_layout,
                Flags::VERTEX
                    | Flags::TESSELLATION_CONTROL
                    | Flags::TESSELLATION_EVALUATION
                    | Flags::FRAGMENT,
                0,
                &pc_bytes,
            )
        };

        unsafe {
            device.cmd_draw_indexed(
                cmd_buf,
                (self.edge_index_buffer.edge_count * 2) as u32,
                1,
                0,
                0,
                0,
            )
        };

        // End render pass
        unsafe { device.cmd_end_render_pass(cmd_buf) };

        Ok(())
    }
}

pub struct EdgeIndices {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    // size: vk::DeviceSize,
    edge_count: usize,
}

impl EdgeIndices {
    fn new(app: &GfaestusVk, graph: &PackedGraph) -> Result<Self> {
        let edge_count = graph.edge_count();

        let mut edges: Vec<u32> = Vec::with_capacity(edge_count * 2);

        for Edge(left, right) in graph.edges() {
            let left_l = left.forward().flip().0 - 1;
            let left_r = left.forward().0 - 1;

            let right_l = right.forward().flip().0 - 1;
            let right_r = right.forward().0 - 1;

            let (left_ix, right_ix) =
                match (left.is_reverse(), right.is_reverse()) {
                    (false, false) => (left_r, right_l),
                    (true, false) => (left_l, right_l),
                    (false, true) => (left_r, right_r),
                    (true, true) => (left_l, right_r),
                };

            edges.push(left_ix as u32);
            edges.push(right_ix as u32);
        }

        println!("added {} edges", edges.len());

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::INDEX_BUFFER;

        let (buffer, memory) =
            app.create_device_local_buffer_with_data::<u32, _>(usage, &edges)?;

        Ok(Self {
            buffer,
            memory,
            edge_count,
        })
    }
}
