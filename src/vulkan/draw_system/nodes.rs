use ash::version::DeviceV1_0;
use ash::{vk, Device};
use handlegraph::handle::NodeId;
use rustc_hash::FxHashSet;

use std::{ffi::CString, ops::RangeInclusive};

use nalgebra_glm as glm;

use anyhow::Result;

use crate::view::View;
use crate::vulkan::GfaestusVk;
use crate::vulkan::SwapchainProperties;
use crate::{
    geometry::Point, overlays::OverlayKind, vulkan::texture::GradientTexture,
};

use crate::vulkan::render_pass::Framebuffers;

use super::create_shader_module;
use super::Vertex;

pub mod overlay;
pub mod theme;

pub use overlay::*;
pub use theme::*;

pub struct NodePipelines {
    pub theme_pipeline: NodeThemePipeline,
    pub overlay_pipeline: NodeOverlayPipeline,

    pub overlay_pipelines: OverlayPipelines,

    selection_descriptors: SelectionDescriptors,

    pub vertices: NodeVertices,
}

impl NodePipelines {
    pub fn new(
        app: &GfaestusVk,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        selection_buffer: vk::Buffer,
    ) -> Result<Self> {
        let vk_context = app.vk_context();
        let device = vk_context.device();

        let vertices = NodeVertices::new(device);

        let selection_descriptors = SelectionDescriptors::new(
            app,
            swapchain_props,
            selection_buffer,
            1,
        )?;

        let theme_pipeline = NodeThemePipeline::new(
            app,
            msaa_samples,
            render_pass,
            selection_descriptors.layout,
        )?;
        let overlay_pipeline = NodeOverlayPipeline::new(
            device,
            msaa_samples,
            render_pass,
            selection_descriptors.layout,
        )?;

        let overlay_pipelines = OverlayPipelines::new(
            device,
            msaa_samples,
            render_pass,
            selection_descriptors.layout,
        )?;

        Ok(Self {
            theme_pipeline,
            overlay_pipeline,
            overlay_pipelines,
            vertices,
            selection_descriptors,
        })
    }

    pub fn device(&self) -> &Device {
        &self.theme_pipeline.device
    }

    pub fn has_overlay_new(&self) -> bool {
        self.overlay_pipelines.overlay_set_id.is_some()
    }

    pub fn has_overlay(&self) -> bool {
        self.overlay_pipeline.overlay_set_id.is_some()
    }

    pub fn draw_themed(
        &self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        viewport_dims: [f32; 2],
        node_width: f32,
        view: View,
        offset: Point,
    ) -> Result<()> {
        let device = &self.theme_pipeline.device;

        let clear_values = {
            let bg = self.theme_pipeline.active_background_color();
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
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
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
            .framebuffer(framebuffers.nodes)
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
        let desc_sets = [
            self.theme_pipeline.theme_set,
            self.selection_descriptors.descriptor_set,
        ];

        let offsets = [0];
        unsafe {
            device.cmd_bind_vertex_buffers(cmd_buf, 0, &vx_bufs, &offsets);

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::GRAPHICS,
                self.theme_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=1],
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

    pub fn draw_overlay_new(
        &mut self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        viewport_dims: [f32; 2],
        node_width: f32,
        view: View,
        offset: Point,
        overlay: (usize, OverlayKind),
        color_scheme: &GradientTexture,
    ) -> Result<()> {
        self.overlay_pipelines
            .write_overlay(overlay, color_scheme)?;

        let device = &self.overlay_pipeline.device;

        let clear_values = {
            let bg = self.theme_pipeline.active_background_color();
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
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
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
            .framebuffer(framebuffers.nodes)
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

        self.overlay_pipelines
            .bind_pipeline(device, cmd_buf, overlay.1);

        let vx_bufs = [self.vertices.vertex_buffer];
        let offsets = [0];

        unsafe {
            device.cmd_bind_vertex_buffers(cmd_buf, 0, &vx_bufs, &offsets);
        }

        self.overlay_pipelines.bind_descriptor_sets(
            device,
            cmd_buf,
            overlay,
            self.selection_descriptors.descriptor_set,
        )?;

        let push_constants = NodePushConstants::new(
            [offset.x, offset.y],
            viewport_dims,
            view,
            node_width,
            7,
        );

        let pc_bytes = push_constants.bytes();

        let layout = self.overlay_pipelines.pipeline_layout_kind(overlay.1);

        unsafe {
            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                layout,
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

    pub fn draw_overlay(
        &self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        viewport_dims: [f32; 2],
        node_width: f32,
        view: View,
        offset: Point,
    ) -> Result<()> {
        let device = &self.overlay_pipeline.device;

        let clear_values = {
            let bg = self.theme_pipeline.active_background_color();
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
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
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
            .framebuffer(framebuffers.nodes)
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
                self.overlay_pipeline.pipeline,
            )
        };

        let vx_bufs = [self.vertices.vertex_buffer];
        let desc_sets = [
            self.overlay_pipeline.overlay_set,
            self.selection_descriptors.descriptor_set,
        ];

        let offsets = [0];
        unsafe {
            device.cmd_bind_vertex_buffers(cmd_buf, 0, &vx_bufs, &offsets);

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::GRAPHICS,
                self.overlay_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=1],
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
                self.overlay_pipeline.pipeline_layout,
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
        let device = &self.theme_pipeline.device;

        unsafe {
            device.destroy_descriptor_set_layout(
                self.selection_descriptors.layout,
                None,
            );
            device
                .destroy_descriptor_pool(self.selection_descriptors.pool, None);
        }

        self.vertices.destroy();
        self.theme_pipeline.destroy();
        self.overlay_pipeline.destroy();
    }
}

pub struct SelectionDescriptors {
    pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
    // TODO should be one per swapchain image
    descriptor_set: vk::DescriptorSet,
    // should not be owned by this, but MainView
    // buffer: vk::Buffer,
}

impl SelectionDescriptors {
    fn new(
        app: &GfaestusVk,
        swapchain_props: SwapchainProperties,
        buffer: vk::Buffer,
        image_count: u32,
        // msaa_samples: vk::SampleCountFlags,
    ) -> Result<Self> {
        let vk_context = app.vk_context();
        let device = vk_context.device();

        let layout = Self::create_descriptor_set_layout(device)?;

        let descriptor_pool = {
            let pool_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
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
            let layouts = vec![layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        for set in descriptor_sets.iter() {
            let buf_info = vk::DescriptorBufferInfo::builder()
                .buffer(buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)
                .build();

            let buf_infos = [buf_info];

            let descriptor_write = vk::WriteDescriptorSet::builder()
                .dst_set(*set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&buf_infos)
                .build();

            let descriptor_writes = [descriptor_write];

            unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) }
        }

        Ok(Self {
            pool: descriptor_pool,
            layout,
            // TODO should be one per swapchain image
            descriptor_set: descriptor_sets[0],
            // should not be owned by this, but MainView
            // buffer,
        })
    }

    fn layout_binding() -> vk::DescriptorSetLayoutBinding {
        use vk::ShaderStageFlags as Stages;

        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
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
}

pub struct NodeIdBuffer {
    pub buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
    pub width: u32,
    pub height: u32,
}

impl NodeIdBuffer {
    pub fn read_rect(
        &self,
        device: &Device,
        x_range: RangeInclusive<u32>,
        y_range: RangeInclusive<u32>,
    ) -> FxHashSet<NodeId> {
        let min_x = (*x_range.start()).max(0);
        let max_x = (*x_range.end()).min(self.width - 1);

        let min_y = (*y_range.start()).max(0);
        let max_y = (*y_range.end()).min(self.height - 1);

        let mut values: FxHashSet<NodeId> = FxHashSet::default();

        let rows = min_y..=max_y;
        let row_width = (max_x - min_x) as usize;

        unsafe {
            let data_ptr = device
                .map_memory(
                    self.memory,
                    0,
                    self.size,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap();

            for y in rows {
                let row_start = ((y * self.width) + min_x) as usize;
                let val_ptr = (data_ptr as *const u32).add(row_start);

                let slice = std::slice::from_raw_parts(val_ptr, row_width);

                values.extend(slice.iter().filter_map(|&id| {
                    if id == 0 {
                        None
                    } else {
                        Some(NodeId::from(id as u64))
                    }
                }));
            }

            device.unmap_memory(self.memory);
        }

        values
    }

    pub fn read(&self, device: &Device, x: u32, y: u32) -> Option<u32> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let value = unsafe {
            let data_ptr = device
                .map_memory(
                    self.memory,
                    0,
                    self.size,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap();

            let x_offset = |x: u32, o: i32| -> u32 {
                let x = x as i32;
                (x + o).clamp(0, (self.width - 1) as i32) as u32
            };

            let y_offset = |y: u32, o: i32| -> u32 {
                let y = y as i32;
                (y + o).clamp(0, (self.height - 1) as i32) as u32
            };

            let to_ix =
                |x: u32, y: u32| -> usize { (y * self.width + x) as usize };

            let index = (y * self.width + x) as usize;

            let ix_l = to_ix(x_offset(x, -1), y);
            let ix_r = to_ix(x_offset(x, 1), y);

            let ix_u = to_ix(x, y_offset(y, -1));
            let ix_d = to_ix(x, y_offset(y, 1));

            let indices = [index, ix_l, ix_r, ix_u, ix_d];

            let mut value = 0;

            for &ix in indices.iter() {
                let val_ptr = (data_ptr as *const u32).add(ix);
                value = val_ptr.read();

                if value != 0 {
                    break;
                }
            }

            device.unmap_memory(self.memory);

            value
        };

        if value == 0 {
            None
        } else {
            Some(value)
        }
    }

    pub fn new(app: &GfaestusVk, width: u32, height: u32) -> Result<Self> {
        let img_size = (width * height * (std::mem::size_of::<u32>() as u32))
            as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_COHERENT
            | vk::MemoryPropertyFlags::HOST_CACHED;

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

        let img_size = (width * height * (std::mem::size_of::<u32>() as u32))
            as vk::DeviceSize;

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

pub struct NodeVertices {
    pub(crate) vertex_count: usize,
    pub(crate) vertex_buffer: vk::Buffer,
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

    pub fn buffer(&self) -> vk::Buffer {
        self.vertex_buffer
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

    pub fn download_vertices(
        &self,
        device: &Device,
        node_count: usize,
        target: &mut Vec<crate::universe::Node>,
    ) -> Result<()> {
        target.clear();
        let cap = target.capacity();
        if cap < node_count {
            target.reserve(node_count - cap);
        }

        unsafe {
            let data_ptr = device.map_memory(
                self.vertex_memory,
                0,
                vk::WHOLE_SIZE,
                vk::MemoryMapFlags::empty(),
            )?;

            let val_ptr = data_ptr as *const crate::universe::Node;

            let sel_slice = std::slice::from_raw_parts(val_ptr, node_count);

            target.extend_from_slice(sel_slice);

            device.unmap_memory(self.vertex_memory);
        }

        target.shrink_to_fit();

        Ok(())
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
    layouts: &[vk::DescriptorSetLayout],
    frag_shader: &[u8],
) -> (vk::Pipeline, vk::PipelineLayout) {
    let vert_src = crate::load_shader!("nodes/base.vert.spv");
    let geom_src = crate::load_shader!("nodes/base.geom.spv");
    let frag_src = {
        let mut cursor = std::io::Cursor::new(frag_shader);
        ash::util::read_spv(&mut cursor).unwrap()
    };

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
