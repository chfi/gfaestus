use ash::version::DeviceV1_0;
use ash::{vk, Device};
use rustc_hash::FxHashMap;

use std::ffi::CString;

use anyhow::Result;

use crate::vulkan::render_pass::Framebuffers;
use crate::vulkan::texture::{Gradients, Texture};
use crate::vulkan::GfaestusVk;

use super::create_shader_module;

pub struct GuiPipeline {
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    texture_sets: Vec<vk::DescriptorSet>,

    sampler: vk::Sampler,
    egui_texture_set: vk::DescriptorSet,
    egui_texture: Texture,
    egui_texture_version: u64,

    texture_set_map: FxHashMap<u64, vk::DescriptorSet>,

    pub vertices: GuiVertices,

    tex_2d_pipeline_layout: vk::PipelineLayout,
    tex_2d_pipeline: vk::Pipeline,

    tex_rgba_pipeline_layout: vk::PipelineLayout,
    tex_rgba_pipeline: vk::Pipeline,
    device: Device,
}

impl GuiPipeline {
    pub fn new(
        app: &super::super::GfaestusVk,
        render_pass: vk::RenderPass,
    ) -> Result<Self> {
        let device = app.vk_context().device();

        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let max_texture_count = 64;

        let descriptor_pool = {
            let sampler_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: max_texture_count,
            };

            // let value_size = vk::DescriptorPoolSize {
            //     ty: vk::DescriptorType::Com
            //     descriptor_count: image_count,
            // };

            let pool_sizes = [sampler_size];

            let pool_info = vk::DescriptorPoolCreateInfo::builder()
                .pool_sizes(&pool_sizes)
                .max_sets(max_texture_count)
                .build();

            unsafe { device.create_descriptor_pool(&pool_info, None) }
        }?;

        let egui_texture_sets = {
            let layouts = vec![desc_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        let (tex_2d_pipeline, tex_2d_pipeline_layout) = Self::create_pipeline(
            device,
            render_pass,
            desc_set_layout,
            crate::load_shader!("gui/gui_2d.frag.spv"),
        );

        let (tex_rgba_pipeline, tex_rgba_pipeline_layout) =
            Self::create_pipeline(
                device,
                render_pass,
                desc_set_layout,
                crate::load_shader!("gui/gui_rgba.frag.spv"),
            );

        let sampler = {
            let sampler_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                // .mag_filter(vk::Filter::LINEAR)
                // .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .anisotropy_enable(false)
                .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
                .unnormalized_coordinates(false)
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .mip_lod_bias(0.0)
                .min_lod(0.0)
                .max_lod(1.0)
                .build();

            unsafe { device.create_sampler(&sampler_info, None) }
        }?;

        let egui_texture = Texture::null();

        let vertices = GuiVertices::new(device);

        let texture_set_map = FxHashMap::default();

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,
            texture_sets: Vec::new(),

            sampler,
            egui_texture_set: egui_texture_sets[0],
            egui_texture,
            egui_texture_version: 0,

            texture_set_map,

            vertices,

            tex_2d_pipeline_layout,
            tex_2d_pipeline,

            tex_rgba_pipeline_layout,
            tex_rgba_pipeline,
            device: device.clone(),
        })
    }

    pub fn draw(
        &self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        viewport_dims: [f32; 2],
    ) -> Result<()> {
        let device = &self.device;

        let clear_values = [];

        let extent = vk::Extent2D {
            width: viewport_dims[0] as u32,
            height: viewport_dims[1] as u32,
        };

        let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
            .render_pass(render_pass)
            .framebuffer(framebuffers.gui)
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

        let vx_bufs = [self.vertices.vertex_buffer];

        let pc_bytes = {
            let push_constants = GuiPushConstants::new(viewport_dims);

            push_constants.bytes()
        };

        for (ix, &(start, ix_count)) in self.vertices.ranges.iter().enumerate()
        {
            if ix_count == 0 {
                continue;
            }

            let vx_offset = self.vertices.vertex_offsets[ix];

            let clip = self.vertices.clips[ix];
            let offset = vk::Offset2D {
                x: clip.min.x as i32,
                y: clip.min.y as i32,
            };
            let extent = vk::Extent2D {
                width: (clip.max.x - clip.min.x) as u32,
                height: (clip.max.y - clip.min.y) as u32,
            };

            let scissor = vk::Rect2D { offset, extent };
            let scissors = [scissor];

            let texture_id = self.vertices.texture_ids[ix];

            unsafe {
                device.cmd_set_scissor(cmd_buf, 0, &scissors);

                let offsets = [0];
                device.cmd_bind_vertex_buffers(cmd_buf, 0, &vx_bufs, &offsets);

                device.cmd_bind_index_buffer(
                    cmd_buf,
                    self.vertices.index_buffer,
                    (start * 4) as vk::DeviceSize,
                    vk::IndexType::UINT32,
                );

                match texture_id {
                    egui::TextureId::Egui => {
                        device.cmd_bind_pipeline(
                            cmd_buf,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.tex_2d_pipeline,
                        );

                        let desc_sets = [self.egui_texture_set];

                        device.cmd_bind_descriptor_sets(
                            cmd_buf,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.tex_2d_pipeline_layout,
                            0,
                            &desc_sets,
                            &[],
                        );

                        use vk::ShaderStageFlags as Flags;
                        device.cmd_push_constants(
                            cmd_buf,
                            self.tex_2d_pipeline_layout,
                            Flags::VERTEX,
                            0,
                            &pc_bytes,
                        );

                        device.cmd_draw_indexed(
                            cmd_buf,
                            ix_count,
                            1,
                            0,
                            vx_offset as i32,
                            0,
                        )
                    }
                    egui::TextureId::User(texture_id) => {
                        device.cmd_bind_pipeline(
                            cmd_buf,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.tex_rgba_pipeline,
                        );

                        let desc_set = self
                            .texture_set_map
                            .get(&texture_id)
                            .expect("GUI tried to use missing texture");
                        let desc_sets = [*desc_set];

                        device.cmd_bind_descriptor_sets(
                            cmd_buf,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.tex_rgba_pipeline_layout,
                            0,
                            &desc_sets,
                            &[],
                        );

                        use vk::ShaderStageFlags as Flags;
                        device.cmd_push_constants(
                            cmd_buf,
                            self.tex_rgba_pipeline_layout,
                            Flags::VERTEX,
                            0,
                            &pc_bytes,
                        );

                        device.cmd_draw_indexed(
                            cmd_buf,
                            ix_count,
                            1,
                            0,
                            vx_offset as i32,
                            0,
                        )
                    }
                }
            };
        }

        unsafe { device.cmd_end_render_pass(cmd_buf) };

        Ok(())
    }

    pub fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        let device = &self.device;

        unsafe {
            device.destroy_descriptor_set_layout(
                self.descriptor_set_layout,
                None,
            );

            device.destroy_sampler(self.sampler, None);

            device.destroy_pipeline(self.tex_2d_pipeline, None);
            device.destroy_pipeline_layout(self.tex_2d_pipeline_layout, None);

            device.destroy_pipeline(self.tex_rgba_pipeline, None);
            device.destroy_pipeline_layout(self.tex_rgba_pipeline_layout, None);

            self.vertices.destroy(allocator);

            if !self.egui_texture.is_null() {
                self.egui_texture.destroy(device);
            }
        }
    }

    pub fn egui_texture_version(&self) -> u64 {
        self.egui_texture_version
    }

    pub fn egui_texture_is_null(&self) -> bool {
        self.egui_texture.is_null()
    }

    pub fn upload_egui_texture(
        &mut self,
        app: &super::super::GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        texture: &egui::Texture,
    ) -> Result<()> {
        if !self.egui_texture_is_null() {
            self.egui_texture.destroy(&app.vk_context.device());
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

        self.egui_texture = texture;
        self.egui_texture_version = version;

        let desc_write = self.egui_descriptor_write();
        let desc_writes = [desc_write];

        let device = app.vk_context().device();

        unsafe { device.update_descriptor_sets(&desc_writes, &[]) }

        Ok(())
    }

    fn egui_descriptor_write(&self) -> vk::WriteDescriptorSet {
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(self.egui_texture.view)
            .sampler(self.sampler)
            .build();
        let image_infos = [image_info];

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.egui_texture_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_infos)
            .build();

        sampler_descriptor_write
    }

    pub fn add_texture(
        &mut self,
        app: &GfaestusVk,
        texture: Texture,
    ) -> Result<egui::TextureId> {
        let device = app.vk_context().device();

        let id = self.texture_sets.len() as u64;
        let tex_id = egui::TextureId::User(id);

        let texture_sets = {
            let layouts = vec![self.descriptor_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(self.descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.view)
            .sampler(self.sampler)
            .build();
        let image_infos = [image_info];

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(texture_sets[0])
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_infos)
            .build();

        let writes = [sampler_descriptor_write];
        unsafe { device.update_descriptor_sets(&writes, &[]) }

        self.texture_sets.push(texture_sets[0]);
        self.texture_set_map.insert(id, texture_sets[0]);

        Ok(tex_id)
    }

    fn gradient_descriptor_write(
        &self,
        texture_id: egui::TextureId,
        gradients: &Gradients,
    ) -> vk::WriteDescriptorSet {
        let texture = gradients.gradient_from_id(texture_id).unwrap();

        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.texture.view)
            .sampler(self.sampler)
            .build();
        let image_infos = [image_info];

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(vk::DescriptorSet::null())
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_infos)
            .build();

        sampler_descriptor_write
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
        render_pass: vk::RenderPass,
        descriptor_set_layout: vk::DescriptorSetLayout,
        frag_src: Vec<u32>,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        let vert_src = crate::load_shader!("gui/gui.vert.spv");

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
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
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

        let multisampling_info =
            vk::PipelineMultisampleStateCreateInfo::builder()
                .sample_shading_enable(false)
                .rasterization_samples(vk::SampleCountFlags::TYPE_1)
                .min_sample_shading(1.0)
                .alpha_to_coverage_enable(false)
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
            let layouts = [descriptor_set_layout];

            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(vk::ShaderStageFlags::VERTEX)
                .offset(0)
                .size(8)
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
            device.destroy_shader_module(frag_module, None);
        }

        (pipeline, layout)
    }
}

pub struct GuiVertices {
    vertex_buffer: vk::Buffer,
    vertex_alloc: vk_mem::Allocation,
    vertex_alloc_info: Option<vk_mem::AllocationInfo>,

    index_buffer: vk::Buffer,
    index_alloc: vk_mem::Allocation,
    index_alloc_info: Option<vk_mem::AllocationInfo>,

    ranges: Vec<(u32, u32)>,
    vertex_offsets: Vec<u32>,
    clips: Vec<egui::Rect>,

    texture_ids: Vec<egui::TextureId>,

    device: Device,
}

impl GuiVertices {
    pub fn new(device: &Device) -> Self {
        let vertex_buffer = vk::Buffer::null();
        let vertex_alloc = vk_mem::Allocation::null();
        let vertex_alloc_info = None;

        let index_buffer = vk::Buffer::null();
        let index_alloc = vk_mem::Allocation::null();
        let index_alloc_info = None;

        let ranges = Vec::new();
        let vertex_offsets = Vec::new();
        let clips = Vec::new();

        let texture_ids = Vec::new();

        let device = device.clone();

        Self {
            vertex_buffer,
            vertex_alloc,
            vertex_alloc_info,

            index_buffer,
            index_alloc,
            index_alloc_info,

            ranges,
            vertex_offsets,
            clips,

            texture_ids,

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
        self.destroy(&app.allocator);

        let mut vertices: Vec<GuiVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        let mut ranges: Vec<(u32, u32)> = Vec::new();
        let mut vertex_offsets: Vec<u32> = Vec::new();
        let mut clips: Vec<egui::Rect> = Vec::new();

        let mut texture_ids: Vec<egui::TextureId> = Vec::new();

        let mut offset = 0u32;
        let mut vertex_offset = 0u32;

        for egui::ClippedMesh(clip, mesh) in meshes.iter() {
            let len = mesh.indices.len() as u32;
            let vx_len = mesh.vertices.len() as u32;

            indices.extend(mesh.indices.iter().copied());
            vertices.extend(mesh.vertices.iter().map(|vx| {
                let (r, g, b, a) = vx.color.to_tuple();
                GuiVertex {
                    position: [vx.pos.x, vx.pos.y],
                    uv: [vx.uv.x, vx.uv.y],
                    color: [
                        (r as f32) / 255.0,
                        (g as f32) / 255.0,
                        (b as f32) / 255.0,
                        (a as f32) / 255.0,
                    ],
                }
            }));

            clips.push(*clip);

            ranges.push((offset, len));
            vertex_offsets.push(vertex_offset);
            texture_ids.push(mesh.texture_id);

            offset += len;
            vertex_offset += vx_len;
        }

        let (vx_buf, vx_alloc, vx_alloc_info) = app
            // .create_buffer_with_data::<u32, _>(
            .create_buffer_with_data(
                vk::BufferUsageFlags::VERTEX_BUFFER,
                vk_mem::MemoryUsage::GpuOnly,
                false,
                &vertices,
            )?;

        let (ix_buf, ix_alloc, ix_alloc_info) = app
            // .create_buffer_with_data::<u32, _>(
            .create_buffer_with_data(
                vk::BufferUsageFlags::INDEX_BUFFER,
                vk_mem::MemoryUsage::GpuOnly,
                false,
                &indices,
            )?;

        app.set_debug_object_name(vx_buf, "GUI Vertex Buffer")?;
        app.set_debug_object_name(ix_buf, "GUI Index Buffer")?;

        self.vertex_buffer = vx_buf;
        self.vertex_alloc = vx_alloc;
        self.vertex_alloc_info = Some(vx_alloc_info);

        self.index_buffer = ix_buf;
        self.index_alloc = ix_alloc;
        self.index_alloc_info = Some(ix_alloc_info);

        self.ranges.clone_from(&ranges);
        self.vertex_offsets.clone_from(&vertex_offsets);
        self.clips.clone_from(&clips);

        self.texture_ids.clone_from(&texture_ids);

        Ok(())
    }

    pub fn destroy(&mut self, allocator: &vk_mem::Allocator) {
        unsafe {
            self.device.destroy_buffer(self.vertex_buffer, None);
            self.device.destroy_buffer(self.index_buffer, None);
        }
        allocator.free_memory(&self.vertex_alloc).unwrap();
        allocator.free_memory(&self.index_alloc).unwrap();

        self.vertex_buffer = vk::Buffer::null();
        self.vertex_alloc = vk_mem::Allocation::null();
        self.vertex_alloc_info = None;

        self.index_buffer = vk::Buffer::null();
        self.index_alloc = vk_mem::Allocation::null();
        self.index_alloc_info = None;

        self.ranges.clear();
        self.vertex_offsets.clear();
        self.clips.clear();
    }
}

use bytemuck::{Pod, Zeroable};

#[derive(Clone, Copy, Zeroable, Pod)]
#[repr(C)]
pub struct GuiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
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
            .offset(8)
            .build();

        let color_desc = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(2)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(16)
            .build();

        [pos_desc, uv_desc, color_desc]
    }
}

pub struct GuiPushConstants {
    width: f32,
    height: f32,
}

impl GuiPushConstants {
    #[inline]
    pub fn new(viewport_dims: [f32; 2]) -> Self {
        let width = viewport_dims[0];
        let height = viewport_dims[1];

        Self { width, height }
    }

    #[inline]
    pub fn bytes(&self) -> [u8; 8] {
        let mut bytes = [0u8; 8];

        {
            let mut offset = 0;

            let mut add_float = |f: f32| {
                let f_bytes = f.to_ne_bytes();
                for i in 0..4 {
                    bytes[offset] = f_bytes[i];
                    offset += 1;
                }
            };

            add_float(self.width);
            add_float(self.height);
        }

        bytes
    }
}
