use crate::{
    geometry::{Point, Rect},
    universe::FlatLayout,
    view::{ScreenDims, View},
};

use handlegraph::{handle::Edge, handlegraph::*};

use handlegraph::packedgraph::PackedGraph;

use ash::version::{DeviceV1_0, InstanceV1_0};
use ash::{vk, Device};

use anyhow::Result;

use nalgebra_glm as glm;

use std::ffi::CString;

use super::create_shader_module;
use super::Vertex;

use super::nodes::NodePushConstants;
use crate::vulkan::render_pass::Framebuffers;
use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

// use crate::vulkan::compute::ComputePipeline;

pub struct EdgeRenderer {
    pub(crate) descriptor_pool: vk::DescriptorPool,
    pub(crate) descriptor_set_layout: vk::DescriptorSetLayout,
    pub(crate) descriptor_set: vk::DescriptorSet,

    ubo: EdgesUBOBuffer,

    pub(crate) pipeline_layout: vk::PipelineLayout,
    pub(crate) pipeline: vk::Pipeline,

    pub(crate) device: Device,
    pub(crate) edge_index_buffer: EdgeIndices,

    wide_lines: bool,
}

impl EdgeRenderer {
    fn layout_binding() -> vk::DescriptorSetLayoutBinding {
        use vk::ShaderStageFlags as Stages;

        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(
                Stages::VERTEX
                    | Stages::TESSELLATION_CONTROL
                    | Stages::TESSELLATION_EVALUATION
                    | Stages::FRAGMENT,
            )
            .build()
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let binding = Self::layout_binding();
        let bindings = [binding];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout =
            unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        Ok(layout)
    }

    fn create_isoline_pipeline(
        device: &Device,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        layouts: &[vk::DescriptorSetLayout],
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        let vert_src = crate::load_shader!("edges/edges.vert.spv");
        let tesc_src = crate::load_shader!("edges/edges.tesc.spv");
        let tese_src = crate::load_shader!("edges/edges.tese.spv");
        let frag_src = crate::load_shader!("edges/edges.frag.spv");

        Self::create_pipeline(
            device,
            msaa_samples,
            render_pass,
            layouts,
            &vert_src,
            &tesc_src,
            &tese_src,
            &frag_src,
        )
    }

    fn create_quad_pipeline(
        device: &Device,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        layouts: &[vk::DescriptorSetLayout],
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        let vert_src = crate::load_shader!("edges/quads.vert.spv");
        let tesc_src = crate::load_shader!("edges/quads.tesc.spv");
        let tese_src = crate::load_shader!("edges/quads.tese.spv");
        let frag_src = crate::load_shader!("edges/edges.frag.spv");

        Self::create_pipeline(
            device,
            msaa_samples,
            render_pass,
            layouts,
            &vert_src,
            &tesc_src,
            &tese_src,
            &frag_src,
        )
    }

    fn create_pipeline(
        device: &Device,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        layouts: &[vk::DescriptorSetLayout],
        vert_src: &[u32],
        tesc_src: &[u32],
        tese_src: &[u32],
        frag_src: &[u32],
    ) -> (vk::Pipeline, vk::PipelineLayout) {
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
            [DS::VIEWPORT, DS::SCISSOR, DS::LINE_WIDTH]
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

            // let layouts = [];

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
        layout: &FlatLayout,
    ) -> Result<Self> {
        let vk_context = app.vk_context();
        let device = app.vk_context().device();

        let msaa_samples = app.msaa_samples;
        let render_pass = app.render_passes.edges;

        let ubo = EdgesUBOBuffer::new(app)?;

        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let image_count = 1;

        let descriptor_pool = {
            let pool_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: image_count,
            };

            let pool_sizes = [pool_size];

            let pool_info = vk::DescriptorPoolCreateInfo::builder()
                .pool_sizes(&pool_sizes)
                .max_sets(image_count)
                .build();

            unsafe { device.create_descriptor_pool(&pool_info, None) }
        }?;

        let descriptor_sets = {
            let layouts = vec![desc_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        for set in descriptor_sets.iter() {
            let buf_info = vk::DescriptorBufferInfo::builder()
                .buffer(ubo.buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)
                .build();

            let buf_infos = [buf_info];

            let descriptor_write = vk::WriteDescriptorSet::builder()
                .dst_set(*set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(&buf_infos)
                .build();

            let descriptor_writes = [descriptor_write];

            unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) }
        }

        let layouts = [desc_set_layout];

        let renderer_config = vk_context.renderer_config;
        let wide_lines = renderer_config.supported_features.wide_lines;

        let (pipeline, pipeline_layout) = match renderer_config.edges {
            crate::vulkan::context::EdgeRendererType::TessellationIsolines => {
                Self::create_isoline_pipeline(
                    device,
                    msaa_samples,
                    render_pass,
                    &layouts,
                )
            }
            crate::vulkan::context::EdgeRendererType::TessellationQuads => {
                Self::create_quad_pipeline(
                    device,
                    msaa_samples,
                    render_pass,
                    &layouts,
                )
            }
            crate::vulkan::context::EdgeRendererType::Disabled => {
                anyhow::bail!("Tried to create a Disabled edge renderer!");
            }
        };

        let edge_index_buffer =
            EdgeIndices::new_with_components(app, graph, layout)?;

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,
            descriptor_set: descriptor_sets[0],

            ubo,

            pipeline_layout,
            pipeline,

            edge_index_buffer,
            device: device.clone(),

            wide_lines,
        })
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.device.destroy_descriptor_set_layout(
                self.descriptor_set_layout,
                None,
            );

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);

            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }

    pub fn write_ubo(&mut self, ubo: &EdgesUBO) -> Result<()> {
        self.ubo.ubo = *ubo;
        self.ubo.write_ubo()
    }

    pub fn draw(
        &self,
        cmd_buf: vk::CommandBuffer,
        edge_width: f32,
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

        unsafe {
            if self.wide_lines {
                device.cmd_set_line_width(cmd_buf, edge_width);
            } else {
                device.cmd_set_line_width(cmd_buf, 1.0);
            }
        }

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

        let desc_sets = [self.descriptor_set];

        let offsets = [0];
        unsafe {
            device.cmd_bind_vertex_buffers(cmd_buf, 0, &vx_bufs, &offsets);

            device.cmd_bind_index_buffer(
                cmd_buf,
                self.edge_index_buffer.buffer,
                0,
                vk::IndexType::UINT32,
            );

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &desc_sets[0..=0],
                &null,
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

#[allow(dead_code)]
pub struct EdgeIndices {
    pub buffer: vk::Buffer,

    allocation: vk_mem::Allocation,
    allocation_info: vk_mem::AllocationInfo,

    edge_count: usize,
}

impl EdgeIndices {
    fn new_with_components(
        app: &GfaestusVk,
        graph: &PackedGraph,
        layout: &FlatLayout,
    ) -> Result<Self> {
        let mut edge_count = 0;
        let mut edges: Vec<u32> = Vec::with_capacity(graph.edge_count() * 2);

        for Edge(left, right) in graph.edges() {
            let left_comp = layout.node_component(left.id());
            let right_comp = layout.node_component(right.id());

            if left_comp != right_comp {
                continue;
            }

            let left_l = (left.id().0 - 1) * 2;
            let left_r = left_l + 1;

            let right_l = (right.id().0 - 1) * 2;
            let right_r = right_l + 1;

            let (left_ix, right_ix) =
                match (left.is_reverse(), right.is_reverse()) {
                    (false, false) => (left_r, right_l),
                    (true, false) => (left_l, right_l),
                    (false, true) => (left_r, right_r),
                    (true, true) => (left_l, right_r),
                };

            edges.push(left_ix as u32);
            edges.push(right_ix as u32);
            edge_count += 1;
        }

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::INDEX_BUFFER;

        let memory_usage = vk_mem::MemoryUsage::GpuOnly;

        let (buffer, allocation, allocation_info) = app
            // .create_buffer_with_data::<u32, _>(
            .create_buffer_with_data(usage, memory_usage, false, &edges)?;

        app.set_debug_object_name(buffer, "Edge Indices Buffer")?;

        Ok(Self {
            buffer,
            allocation,
            allocation_info,

            edge_count,
        })
    }
}

pub struct PreprocessPushConstants {
    edge_count: usize,
    visible_area: Rect,
    viewport_size: ScreenDims,
}

impl PreprocessPushConstants {
    pub const PC_RANGE: u32 = (std::mem::size_of::<u32>() * 7) as u32;

    #[inline]
    pub fn new<Dims: Into<ScreenDims>>(
        edge_count: usize,
        viewport_dims: Dims,
        view: crate::view::View,
    ) -> Self {
        let viewport_size = viewport_dims.into();

        let visible_area = {
            let map = view.screen_to_world_map(viewport_size);

            let top_left = glm::vec4(0.0, 0.0, 0.0, 1.0);
            let bottom_right =
                glm::vec4(viewport_size.width, viewport_size.height, 0.0, 1.0);

            let tl = map * top_left;
            let br = map * bottom_right;

            let min = Point::new(tl[0], tl[1]);
            let max = Point::new(br[0], br[1]);
            Rect::new(min, max)
        };

        Self {
            edge_count,
            viewport_size,
            visible_area,
        }
    }

    #[inline]
    pub fn bytes(&self) -> [u8; Self::PC_RANGE as usize] {
        let mut bytes = [0u8; 7 * 4];

        let mut offset = 0;

        let ec_bytes = self.edge_count.to_ne_bytes();
        for i in 0..4 {
            bytes[offset] = ec_bytes[i];
            offset += 1;
        }

        {
            let mut add_float = |f: f32| {
                let f_bytes = f.to_ne_bytes();
                for i in 0..4 {
                    bytes[offset] = f_bytes[i];
                    offset += 1;
                }
            };

            add_float(self.visible_area.min().x);
            add_float(self.visible_area.min().y);
            add_float(self.visible_area.max().x);
            add_float(self.visible_area.max().y);

            add_float(self.viewport_size.width);
            add_float(self.viewport_size.height);
        }

        bytes
    }
}

pub struct EdgesUBOBuffer {
    ubo: EdgesUBO,

    buffer: vk::Buffer,
    allocation: vk_mem::Allocation,
    allocation_info: vk_mem::AllocationInfo,
}

impl EdgesUBOBuffer {
    pub fn new(app: &GfaestusVk) -> Result<Self> {
        let ubo = EdgesUBO::default();

        let data = ubo.bytes();

        let usage = vk::BufferUsageFlags::UNIFORM_BUFFER;

        // let memory_usage = vk_mem::MemoryUsage::CpuToGpu;
        let memory_usage = vk_mem::MemoryUsage::CpuOnly;

        let (buffer, allocation, allocation_info) = app
            // .create_buffer_with_data::<f32, _>(
            .create_buffer_with_data(usage, memory_usage, true, &data)?;

        app.set_debug_object_name(buffer, "Edges UBO")?;

        let result = Self {
            ubo,

            buffer,
            allocation,
            allocation_info,
        };

        result.write_ubo()?;

        Ok(result)
    }

    pub fn write_ubo(&self) -> Result<()> {
        let tls = &self.ubo.tess_levels;

        #[rustfmt::skip]
        let data = EdgesUBOData {
            edge_color: [
                self.ubo.edge_color.r,
                self.ubo.edge_color.g,
                self.ubo.edge_color.b,
                1.0,
            ],

            edge_width: self.ubo.edge_width,

            tess_levels: [0.0, 0.0, 0.0, tls[0],
                          0.0, 0.0, 0.0, tls[1],
                          0.0, 0.0, 0.0, tls[2],
                          0.0, 0.0, 0.0, tls[3],
                          0.0, 0.0, 0.0, tls[4]],

            curve_offset: self.ubo.curve_offset,
        };

        let ubos = [data];

        let mapped_ptr = self.allocation_info.get_mapped_data();

        unsafe {
            let mapped_ptr = mapped_ptr as *mut std::ffi::c_void;

            let mut align = ash::util::Align::new(
                mapped_ptr,
                std::mem::align_of::<f32>() as _,
                std::mem::size_of_val(&ubos) as u64,
            );

            align.copy_from_slice(&ubos);
        }

        Ok(())
    }

    pub fn destroy(&self, app: &GfaestusVk) -> Result<()> {
        app.allocator
            .destroy_buffer(self.buffer, &self.allocation)?;
        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct EdgesUBOData {
    edge_color: [f32; 4],
    edge_width: f32,

    tess_levels: [f32; 20],

    curve_offset: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct EdgesUBO {
    pub edge_color: rgb::RGB<f32>,
    pub edge_width: f32,

    pub tess_levels: [f32; 5],

    pub curve_offset: f32,
}

impl std::default::Default for EdgesUBO {
    fn default() -> Self {
        Self {
            edge_color: rgb::RGB::new(0.1, 0.1, 0.1),
            edge_width: 1.7,

            tess_levels: [2.0, 3.0, 5.0, 8.0, 16.0],

            curve_offset: 0.2,
        }
    }
}

impl EdgesUBO {
    pub fn bytes(&self) -> [u8; 116] {
        let mut bytes = [0u8; 116];

        let mut offset = 0;

        let mut add_float = |f: f32| {
            let f_bytes = f.to_ne_bytes();
            for i in 0..4 {
                bytes[offset] = f_bytes[i];
                offset += 1;
            }
        };
        add_float(self.edge_color.r);
        add_float(self.edge_color.g);
        add_float(self.edge_color.b);
        add_float(1.0); // vec3s are kinda nasty wrt alignments

        add_float(self.edge_width);

        for &tl in &self.tess_levels {
            add_float(tl);
            add_float(tl);
            add_float(tl);
            add_float(tl);
        }

        add_float(self.curve_offset);

        bytes
    }
}
