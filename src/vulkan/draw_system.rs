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

pub struct NodeDrawAsh {
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    has_vertices: bool,

    vertex_count: usize,

    vertex_buffer: vk::Buffer,
    vertex_buffer_memory: vk::DeviceMemory,
    // uniform_buffer: vk::Buffer,
    // uniform_buffer_memory: vk::DeviceMemory,

    // descriptor_set: vk::DescriptorSet,
    device: Device,
}

pub struct NodePC {
    view_transform: glm::Mat4,
    node_width: f32,
    scale: f32,
    viewport_dims: [f32; 2],
}

impl NodePC {
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

        // let view_data = view::mat4_to_array(&matrix);

        Self {
            view_transform: matrix,
            node_width,
            viewport_dims,
            scale: view.scale,
        }
    }

    pub fn bytes(&self) -> [u8; 80] {
        use crate::view;

        let mut bytes = [0u8; 80];

        // let view_transform_
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

impl NodeDrawAsh {
    pub fn destroy(&self) {
        let device = &self.device;

        unsafe {
            println!("NodeDrawAsh - desc set layout");
            device.destroy_descriptor_set_layout(
                self.descriptor_set_layout,
                None,
            );
            println!("NodeDrawAsh - pipeline layout");
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            println!("NodeDrawAsh - pipeline");
            device.destroy_pipeline(self.pipeline, None);

            if self.has_vertices {
                println!("NodeDrawAsh - vertex buffer");
                device.destroy_buffer(self.vertex_buffer, None);
                println!("NodeDrawAsh - vertex memory");
                device.free_memory(self.vertex_buffer_memory, None);
            }
        }
    }
}

// pub struct NodesUBO {
//     matrix: glm::Mat4,
// }

impl NodeDrawAsh {
    pub fn new(
        vk_context: &super::VkContext,
        // desc_pool: &vk::DescriptorPool,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,

        render_pass: vk::RenderPass,
    ) -> Result<Self> {
        let device = vk_context.device();

        let descriptor_set_layout = Self::create_descriptor_set_layout(device);

        let (pipeline, pipeline_layout) = Self::create_pipeline(
            device,
            swapchain_props,
            msaa_samples,
            render_pass,
            descriptor_set_layout,
        );

        let vertex_buffer = vk::Buffer::null();
        let vertex_buffer_memory = vk::DeviceMemory::null();

        let device = vk_context.device().clone();

        Ok(Self {
            device,

            render_pass,
            descriptor_set_layout,

            pipeline_layout,
            pipeline,

            vertex_count: 0,

            has_vertices: false,

            vertex_buffer,
            vertex_buffer_memory,
        })
    }

    pub fn draw(
        &self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffer: vk::Framebuffer,
        extent: vk::Extent2D,
        view: View,
        offset: Point,
        viewport_dims: [f32; 2],
        node_width: f32,
    ) -> Result<()> {
        let device = &self.device;

        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
        }];

        let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(render_pass)
            .framebuffer(framebuffer)
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

        let vx_bufs = [self.vertex_buffer];
        let offsets = [0];
        unsafe {
            device.cmd_bind_vertex_buffers(cmd_buf, 0, &vx_bufs, &offsets)
        };

        let push_constants =
            NodePC::new([offset.x, offset.y], viewport_dims, view, node_width);

        let pc_bytes = push_constants.bytes();

        unsafe {
            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.pipeline_layout,
                Flags::VERTEX | Flags::GEOMETRY | Flags::FRAGMENT,
                0,
                &pc_bytes,
            )
        };

        unsafe { device.cmd_draw(cmd_buf, self.vertex_count as u32, 1, 0, 0) };

        // End render pass
        unsafe { device.cmd_end_render_pass(cmd_buf) };

        Ok(())
    }

    pub fn upload_vertices(
        &mut self,
        app: &super::GfaestusVk,
        vertices: &[Vertex],
    ) -> Result<()> {
        if self.has_vertices {
            panic!("replacing node vertices not supported yet");
        }

        let (buf, mem) = app.create_vertex_buffer(vertices)?;

        self.vertex_count = vertices.len();

        self.vertex_buffer = buf;
        self.vertex_buffer_memory = mem;

        self.has_vertices = true;

        Ok(())
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> vk::DescriptorSetLayout {
        let ubo_binding = NodeUniform::get_descriptor_set_layout_binding();
        let bindings = [ubo_binding];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .unwrap()
        }
    }

    fn create_pipeline(
        device: &Device,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        descriptor_set_layout: vk::DescriptorSetLayout,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        let vert_src =
            read_shader_from_file("shaders/nodes_simple.vert.spv").unwrap();
        let geom_src =
            read_shader_from_file("shaders/nodes_simple.geom.spv").unwrap();
        let frag_src =
            read_shader_from_file("shaders/nodes_simple.frag.spv").unwrap();

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

        // let depth_stencil_info = todo!();

        let multisampling_info =
            vk::PipelineMultisampleStateCreateInfo::builder()
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

        let color_blending_info =
            vk::PipelineColorBlendStateCreateInfo::builder()
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

            unsafe {
                device.create_pipeline_layout(&layout_info, None).unwrap()
            }
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

    // fn descriptor_set_layout(device: &Device) -> vk::DescriptorSetLayout {
    //     // let ubo_binding = Unif

    //     let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
    //         .bindings(&[])
    //         .build();

    //     unsafe {
    //         device
    //             .create_descriptor_set_layout(&layout_info, None)
    //             .unwrap()
    //     }
    // }
}

pub struct NodeUniform {
    view_transform: glm::Mat4,
}

impl NodeUniform {
    fn get_descriptor_set_layout_binding() -> vk::DescriptorSetLayoutBinding {
        use vk::ShaderStageFlags as Stages;

        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::VERTEX | Stages::FRAGMENT)
            .build()
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
