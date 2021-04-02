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
use crate::vulkan::texture::Texture1D;
use crate::vulkan::GfaestusVk;
use crate::vulkan::SwapchainProperties;

use super::Vertex;
use super::{create_shader_module, read_shader_from_file};

pub struct NodeThemePipeline {
    descriptor_pool: vk::DescriptorPool,

    descriptor_set_layout: vk::DescriptorSetLayout,

    sampler: vk::Sampler,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    themes: Vec<NodeThemeData>,

    device: Device,
}

pub struct NodeIdBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
    width: u32,
    height: u32,
}

impl NodeIdBuffer {
    pub fn new(app: &GfaestusVk, width: u32, height: u32) -> Result<Self> {
        let img_size = (width * height) as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_COHERENT;

        let (buffer, memory, size) =
            app.create_buffer(img_size, usage, mem_props)?;

        Ok(Self {
            buffer,
            memory,
            size,
            width,
            height,
        })
    }

    pub fn destroy(&mut self, device: &Device) {
        unsafe {
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }

        self.buffer = vk::Buffer::null();
        self.memory = vk::DeviceMemory::null();
        self.size = 0 as vk::DeviceSize;
        self.width = 0;
        self.height = 0;
    }

    pub fn recreate(
        &mut self,
        app: &GfaestusVk,
        width: u32,
        height: u32,
    ) -> Result<()> {
        if self.width * self.height == width * height {
            return Ok(());
        }

        self.destroy(app.vk_context().device());

        let img_size = (width * height) as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_COHERENT;

        let (buffer, memory, size) =
            app.create_buffer(img_size, usage, mem_props)?;

        self.buffer = buffer;
        self.memory = memory;
        self.size = size;
        self.width = width;
        self.height = height;

        Ok(())
    }
}

pub struct NodeThemeData {
    // device: Device,
    descriptor_set: vk::DescriptorSet,

    texture: Texture1D,

    background_color: rgb::RGB<f32>,
}

const RAINBOW: [(f32, f32, f32); 7] = [
    (1.0, 0.0, 0.0),
    (1.0, 0.65, 0.0),
    (1.0, 1.0, 0.0),
    (0.0, 0.5, 0.0),
    (0.0, 0.0, 1.0),
    (0.3, 0.0, 0.51),
    (0.93, 0.51, 0.93),
];

impl NodeThemeData {
    pub fn test_theme(
        app: &super::super::GfaestusVk,
        descriptor_pool: vk::DescriptorPool,
        descriptor_set_layout: vk::DescriptorSetLayout,
        sampler: vk::Sampler,
    ) -> Result<Self> {
        let colors = RAINBOW
            .iter()
            .copied()
            .map(rgb::RGB::from)
            .collect::<Vec<_>>();

        let texture = Texture1D::create_from_colors(
            app,
            app.transient_command_pool,
            app.graphics_queue,
            &colors,
        )?;

        let device = app.vk_context().device();

        let descriptor_sets = {
            let layouts = vec![descriptor_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        for set in descriptor_sets.iter() {
            let image_info = vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(texture.view)
                .sampler(sampler)
                .build();
            let image_infos = [image_info];

            let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
                .dst_set(*set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_infos)
                .build();

            let descriptor_writes = [sampler_descriptor_write];

            unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) }
        }

        Ok(Self {
            descriptor_set: descriptor_sets[0],
            texture,
            background_color: rgb::RGB::new(0.05, 0.05, 0.25),
        })
    }

    pub fn destroy(&mut self, device: &Device) {
        self.texture.destroy(device);
    }
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

    fn new(
        app: &super::super::GfaestusVk,
        // device: &Device,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        // image_count: usize,
    ) -> Result<Self> {
        let device = app.vk_context().device();

        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let (pipeline, pipeline_layout) = Self::create_pipeline(
            device,
            msaa_samples,
            render_pass,
            desc_set_layout,
        );

        let sampler = create_sampler(device)?;

        let image_count = 1;

        let descriptor_pool = {
            let sampler_pool_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: image_count,
            };

            let pool_sizes = [sampler_pool_size];

            let pool_info = vk::DescriptorPoolCreateInfo::builder()
                .pool_sizes(&pool_sizes)
                .max_sets(image_count)
                .build();

            unsafe { device.create_descriptor_pool(&pool_info, None) }
        }?;

        let theme = NodeThemeData::test_theme(
            app,
            descriptor_pool,
            desc_set_layout,
            sampler,
        )?;

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,

            sampler,

            pipeline_layout,
            pipeline,

            themes: vec![theme],

            device: device.clone(),
        })
    }

    pub fn destroy(&mut self) {
        unsafe {
            for theme in self.themes.iter_mut() {
                theme.destroy(&self.device);
            }
            self.themes.clear();

            self.device.destroy_descriptor_set_layout(
                self.descriptor_set_layout,
                None,
            );
            self.device.destroy_sampler(self.sampler, None);

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);

            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
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

    fn new(
        device: &Device,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        // image_count: usize,
    ) -> Result<Self> {
        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let (pipeline, pipeline_layout) = Self::create_pipeline(
            device,
            msaa_samples,
            render_pass,
            desc_set_layout,
        );

        let sampler = create_sampler(device)?;

        let image_count = 1;

        let descriptor_pool = {
            let sampler_pool_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: image_count,
            };

            let pool_sizes = [sampler_pool_size];

            let pool_info = vk::DescriptorPoolCreateInfo::builder()
                .pool_sizes(&pool_sizes)
                .max_sets(image_count)
                .build();

            unsafe { device.create_descriptor_pool(&pool_info, None) }
        }?;

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,

            sampler,

            pipeline_layout,
            pipeline,

            device: device.clone(),
        })
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.device.destroy_descriptor_set_layout(
                self.descriptor_set_layout,
                None,
            );
            self.device.destroy_sampler(self.sampler, None);

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);

            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

pub struct NodeVertices {
    vertex_count: usize,
    vertex_buffer: vk::Buffer,
    vertex_memory: vk::DeviceMemory,

    device: Device,
}

impl NodeVertices {
    pub fn new(device: &Device) -> Self {
        let vertex_count = 0;
        let vertex_buffer = vk::Buffer::null();
        let vertex_memory = vk::DeviceMemory::null();

        let device = device.clone();

        Self {
            vertex_count,
            vertex_buffer,
            vertex_memory,
            device,
        }
    }

    pub fn has_vertices(&self) -> bool {
        self.vertex_count != 0
    }

    pub fn destroy(&mut self) {
        if self.has_vertices() {
            unsafe {
                self.device.destroy_buffer(self.vertex_buffer, None);
                self.device.free_memory(self.vertex_memory, None);
            }

            self.vertex_buffer = vk::Buffer::null();
            self.vertex_memory = vk::DeviceMemory::null();

            self.vertex_count = 0;
        }
    }

    pub fn upload_vertices(
        &mut self,
        app: &super::super::GfaestusVk,
        vertices: &[Vertex],
    ) -> Result<()> {
        if self.has_vertices() {
            self.destroy();
        }

        let (buf, mem) = app.create_vertex_buffer(vertices)?;

        self.vertex_count = vertices.len();

        self.vertex_buffer = buf;
        self.vertex_memory = mem;

        Ok(())
    }
}

pub struct NodePipelines {
    theme_pipeline: NodeThemePipeline,
    overlay_pipeline: NodeOverlayPipeline,

    pub vertices: NodeVertices,
}

impl NodePipelines {
    pub fn new(
        app: &super::super::GfaestusVk,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
    ) -> Result<Self> {
        let vk_context = app.vk_context();
        let device = vk_context.device();

        let vertices = NodeVertices::new(device);

        let theme_pipeline =
            NodeThemePipeline::new(app, msaa_samples, render_pass)?;
        let overlay_pipeline =
            NodeOverlayPipeline::new(device, msaa_samples, render_pass)?;

        Ok(Self {
            theme_pipeline,
            overlay_pipeline,
            vertices,
        })
    }

    pub fn draw_themed(
        &self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffer: vk::Framebuffer,
        _framebuffer_dc: vk::Framebuffer,
        viewport_dims: [f32; 2],
        node_width: f32,
        view: View,
        offset: Point,
        theme_id: usize,
    ) -> Result<()> {
        let device = &self.theme_pipeline.device;

        let theme = &self.theme_pipeline.themes[0];

        let clear_values = {
            let bg = theme.background_color;
            [
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [bg.r, bg.g, bg.b, 1.0],
                    },
                },
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        uint32: [0, 0, 0, 0],
                    },
                },
            ]
        };

        let extent = vk::Extent2D {
            width: viewport_dims[0] as u32,
            height: viewport_dims[1] as u32,
        };

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
                self.theme_pipeline.pipeline,
            )
        };

        let vx_bufs = [self.vertices.vertex_buffer];
        let desc_sets = [self.theme_pipeline.themes[0].descriptor_set];
        let offsets = [0];
        unsafe {
            device.cmd_bind_vertex_buffers(cmd_buf, 0, &vx_bufs, &offsets);

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::GRAPHICS,
                self.theme_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=0],
                &null,
            );
        };

        // let uniforms = [self.theme_pipeline

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
                self.theme_pipeline.pipeline_layout,
                Flags::VERTEX | Flags::GEOMETRY | Flags::FRAGMENT,
                0,
                &pc_bytes,
            )
        };

        unsafe {
            device.cmd_draw(cmd_buf, self.vertices.vertex_count as u32, 1, 0, 0)
        };

        // End render pass
        unsafe { device.cmd_end_render_pass(cmd_buf) };

        Ok(())
    }

    pub fn destroy(&mut self) {
        self.vertices.destroy();
        self.theme_pipeline.destroy();
        self.overlay_pipeline.destroy();
    }
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
            // .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            // .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            // .color_blend_op(vk::BlendOp::ADD)
            // .src_alpha_blend_factor(vk::BlendFactor::SRC_ALPHA)
            // .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            // .alpha_blend_op(vk::BlendOp::ADD)
            .build();

    let color_blend_attachments =
        [color_blend_attachment, id_color_blend_attachment];

    let color_blending_info = vk::PipelineColorBlendStateCreateInfo::builder()
        .logic_op_enable(false)
        .logic_op(vk::LogicOp::NO_OP)
        // .logic_op(vk::LogicOp::COPY)
        .attachments(&color_blend_attachments)
        .blend_constants([0.0, 0.0, 0.0, 0.0])
        .build();

    let layout = {
        use vk::ShaderStageFlags as Flags;

        let layouts = [descriptor_set_layout];

        let pc_range = vk::PushConstantRange::builder()
            .stage_flags(Flags::VERTEX | Flags::GEOMETRY | Flags::FRAGMENT)
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

fn create_sampler(device: &Device) -> Result<vk::Sampler> {
    let sampler_info = vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::NEAREST)
        .min_filter(vk::Filter::NEAREST)
        .address_mode_u(vk::SamplerAddressMode::REPEAT)
        .address_mode_v(vk::SamplerAddressMode::REPEAT)
        .address_mode_w(vk::SamplerAddressMode::REPEAT)
        .anisotropy_enable(false)
        // .max_anisotropy(16.0)
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
