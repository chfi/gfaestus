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
use crate::vulkan::texture::Texture;
use crate::vulkan::SwapchainProperties;

use super::{create_shader_module, read_shader_from_file};

pub struct GuiPipeline {
    descriptor_pool: vk::DescriptorPool,

    descriptor_set_layout: vk::DescriptorSetLayout,

    sampler: vk::Sampler,
    texture: Texture,
    texture_version: u64,

    vertices: GuiVertices,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    device: Device,
}

impl GuiPipeline {
    pub fn new(
        app: &super::super::GfaestusVk,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
    ) -> Result<Self> {
        let device = app.vk_context().device();

        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let (pipeline, pipeline_layout) = Self::create_pipeline(
            device,
            msaa_samples,
            render_pass,
            desc_set_layout,
        );

        let sampler = {
            let sampler_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .anisotropy_enable(false)
                // .max_anisotropy(16.0)
                .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
                .unnormalized_coordinates(false)
                .compare_enable(false)
                .compare_op(vk::CompareOp::ALWAYS)
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .mip_lod_bias(0.0)
                .min_lod(0.0)
                .max_lod(1.0)
                .build();

            unsafe { device.create_sampler(&sampler_info, None) }
        }?;

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

        let vertices = GuiVertices::new(device);

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,

            sampler,
            texture: Texture::null(),
            texture_version: 0,

            vertices,

            pipeline_layout,
            pipeline,

            device: device.clone(),
        })
    }

    pub fn texture_version(&self) -> u64 {
        self.texture_version
    }

    pub fn texture_is_null(&self) -> bool {
        self.texture.is_null()
    }

    pub fn upload_texture(
        &mut self,
        app: &super::super::GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        texture: egui::Texture,
    ) -> Result<()> {
        if !self.texture_is_null() {
            self.texture.destroy(&app.vk_context.device());
        }

        let width = texture.width;
        let height = texture.height;
        let pixels = &texture.pixels;

        let version = texture.version;

        let texture = Texture::from_pixel_bytes(
            app,
            command_pool,
            transition_queue,
            width,
            height,
            pixels,
        )?;

        self.texture = texture;
        self.texture_version = version;

        Ok(())
    }

    fn layout_binding() -> vk::DescriptorSetLayoutBinding {
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
        let binding = Self::layout_binding();
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
        let vert_src = read_shader_from_file("shaders/gui.frag.spv").unwrap();
        let frag_src = read_shader_from_file("shaders/gui.vert.spv").unwrap();

        let vert_module = create_shader_module(device, &vert_src);
        let frag_module = create_shader_module(device, &frag_src);

        let entry_point = CString::new("main").unwrap();

        let vert_state_info = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(&entry_point)
            .build();

        let frag_state_info = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(&entry_point)
            .build();

        let shader_state_infos = [vert_state_info, frag_state_info];

        let vert_binding_descs = [GuiVertex::get_binding_desc()];
        let vert_attr_descs = GuiVertex::get_attribute_descs();
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
            let layouts = [descriptor_set_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
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
            device.destroy_shader_module(frag_module, None);
        }

        (pipeline, layout)
    }
}

pub struct GuiVertices {
    capacity: usize,

    vertex_buffer: vk::Buffer,
    vertex_memory: vk::DeviceMemory,

    index_buffer: vk::Buffer,
    index_memory: vk::DeviceMemory,

    ranges: Vec<(u32, u32)>,
    clips: Vec<egui::Rect>,

    device: Device,
}

impl GuiVertices {
    pub fn new(device: &Device) -> Self {
        let vertex_buffer = vk::Buffer::null();
        let vertex_memory = vk::DeviceMemory::null();

        let index_buffer = vk::Buffer::null();
        let index_memory = vk::DeviceMemory::null();

        let ranges = Vec::new();
        let clips = Vec::new();

        let device = device.clone();

        Self {
            capacity: 0,

            vertex_buffer,
            vertex_memory,

            index_buffer,
            index_memory,

            ranges,
            clips,

            device,
        }
    }

    pub fn has_vertices(&self) -> bool {
        !self.ranges.is_empty()
    }

    pub fn upload_meshes(
        &mut self,
        app: &super::super::GfaestusVk,
        meshes: &[egui::ClippedMesh],
    ) -> Result<()> {
        // let (clips, meshes): (Vec<_>, Vec<_>) = meshes
        //     .iter()
        //     .map(|egui::ClippedMesh(rect, mesh)| (*rect, mesh))
        //     .unzip();

        // let req_capacity: usize =
        //     meshes.iter().map(|mesh| mesh.indices.len()).sum();

        if self.vertex_buffer != vk::Buffer::null() {
            self.destroy();
        }

        let mut vertices: Vec<GuiVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        let mut ranges: Vec<(u32, u32)> = Vec::new();
        let mut clips: Vec<egui::Rect> = Vec::new();

        let mut offset = 0u32;

        for egui::ClippedMesh(clip, mesh) in meshes.iter() {
            let len = indices.len() as u32;

            indices.extend(mesh.indices.iter().copied());
            vertices.extend(mesh.vertices.iter().map(|vx| GuiVertex {
                position: [vx.pos.x, vx.pos.y],
                uv: [vx.uv.x, vx.uv.y],
                color: vx.color.to_array(),
            }));

            clips.push(*clip);

            ranges.push((offset, offset + len));
            offset += len;
        }

        let (vx_buf, vx_mem) = app
            .create_device_local_buffer_with_data::<u32, _>(
                vk::BufferUsageFlags::VERTEX_BUFFER,
                &vertices,
            )?;

        let (ix_buf, ix_mem) = app
            .create_device_local_buffer_with_data::<u32, _>(
                vk::BufferUsageFlags::INDEX_BUFFER,
                &indices,
            )?;

        self.vertex_buffer = vx_buf;
        self.vertex_memory = vx_mem;

        self.index_buffer = ix_buf;
        self.index_memory = ix_mem;

        self.ranges.clone_from(&ranges);
        self.clips.clone_from(&clips);

        Ok(())
    }

    pub fn destroy(&mut self) {
        if self.has_vertices() {
            unsafe {
                self.device.destroy_buffer(self.vertex_buffer, None);
                self.device.free_memory(self.vertex_memory, None);

                self.device.destroy_buffer(self.index_buffer, None);
                self.device.free_memory(self.index_memory, None);
            }

            self.vertex_buffer = vk::Buffer::null();
            self.vertex_memory = vk::DeviceMemory::null();

            self.index_buffer = vk::Buffer::null();
            self.index_memory = vk::DeviceMemory::null();

            self.ranges.clear();
            self.clips.clear();
        }
    }
}

#[derive(Clone, Copy)]
pub struct GuiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

impl GuiVertex {
    fn get_binding_desc() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(std::mem::size_of::<GuiVertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    fn get_attribute_descs() -> [vk::VertexInputAttributeDescription; 3] {
        let pos_desc = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build();

        let uv_desc = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(12)
            .build();

        let color_desc = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(2)
            .format(vk::Format::R8G8B8A8_UINT)
            .offset(24)
            .build();

        [pos_desc, uv_desc, color_desc]
    }
}
