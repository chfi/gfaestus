use ash::version::DeviceV1_0;
use ash::{vk, Device};
use handlegraph::handle::NodeId;
use rustc_hash::FxHashSet;

use std::ops::RangeInclusive;

use nalgebra_glm as glm;

use anyhow::*;

use crate::view::View;
use crate::vulkan::context::NodeRendererType;
use crate::vulkan::GfaestusVk;
use crate::{geometry::Point, vulkan::texture::GradientTexture};

use crate::vulkan::render_pass::Framebuffers;

pub mod base;
pub mod overlay;
pub mod vertices;

pub use base::*;
pub use overlay::*;
pub use vertices::*;

pub struct NodePipelines {
    pub pipelines: OverlayPipelines,

    selection_descriptors: SelectionDescriptors,

    pub vertices: NodeVertices,

    device: Device,

    renderer_type: NodeRendererType,
}

impl NodePipelines {
    pub fn new(app: &GfaestusVk, selection_buffer: vk::Buffer) -> Result<Self> {
        let vk_context = app.vk_context();
        let device = vk_context.device();

        let renderer_type = vk_context.renderer_config.nodes;

        log::warn!("node_renderer_type: {:?}", renderer_type);

        let vertices = NodeVertices::new(renderer_type);

        let selection_descriptors =
            SelectionDescriptors::new(app, selection_buffer, 1)?;

        let pipelines = OverlayPipelines::new(
            app,
            renderer_type,
            selection_descriptors.layout,
        )?;

        Ok(Self {
            pipelines,
            vertices,
            selection_descriptors,

            device: device.clone(),

            renderer_type,
        })
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn has_overlay(&self) -> bool {
        self.pipelines.overlay_set_id.is_some()
    }

    pub fn draw(
        &mut self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        viewport_dims: [f32; 2],
        node_width: f32,
        view: View,
        offset: Point,
        background_color: rgb::RGB<f32>,
        overlay_id: usize,
        color_scheme: &GradientTexture,
    ) -> Result<()> {
        self.pipelines.write_overlay(overlay_id, color_scheme)?;

        let overlay = self.pipelines.overlays.get(&overlay_id).unwrap();

        let device = &self.pipelines.device;

        let clear_values = {
            let bg = background_color;
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

        self.pipelines.bind_pipeline(device, cmd_buf, overlay.kind);

        let vx_bufs = [self.vertices.vertex_buffer];
        let offsets = [0];

        unsafe {
            device.cmd_bind_vertex_buffers(cmd_buf, 0, &vx_bufs, &offsets);
        }

        self.pipelines.bind_descriptor_sets(
            device,
            cmd_buf,
            overlay_id,
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

        let layout = self.pipelines.pipeline_layout_kind(overlay.kind);

        unsafe {
            use vk::ShaderStageFlags as Flags;

            let mut stages = Flags::VERTEX | Flags::FRAGMENT;

            if self.renderer_type == NodeRendererType::TessellationQuads {
                stages |= Flags::TESSELLATION_CONTROL
                    | Flags::TESSELLATION_EVALUATION;
            }

            device.cmd_push_constants(cmd_buf, layout, stages, 0, &pc_bytes)
        };

        unsafe {
            device.cmd_draw(cmd_buf, self.vertices.vertex_count as u32, 1, 0, 0)
        };

        // End render pass
        unsafe { device.cmd_end_render_pass(cmd_buf) };

        Ok(())
    }

    pub fn destroy(&mut self, app: &super::super::GfaestusVk) {
        let device = &self.device;

        unsafe {
            device.destroy_descriptor_set_layout(
                self.selection_descriptors.layout,
                None,
            );
            device
                .destroy_descriptor_pool(self.selection_descriptors.pool, None);
        }

        self.vertices.destroy(app).unwrap();
        self.pipelines.destroy(&app.allocator).unwrap();
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

    elem_size: u32,
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

    pub fn new(
        app: &GfaestusVk,
        width: u32,
        height: u32,
        id_format: vk::Format,
    ) -> Result<Self> {
        use std::mem;
        let elem_size = match id_format {
            vk::Format::R32_UINT => Ok(mem::size_of::<u32>()),
            vk::Format::R32G32_UINT => Ok(mem::size_of::<[u32; 2]>()),
            vk::Format::R32G32B32_UINT => Ok(mem::size_of::<[u32; 3]>()),
            vk::Format::R32G32B32A32_UINT => Ok(mem::size_of::<[u32; 4]>()),
            _ => Err(anyhow!("Incompatible ID format")),
        }?;

        let img_size = (width * height * elem_size as u32) as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_COHERENT
            | vk::MemoryPropertyFlags::HOST_CACHED;

        let (buffer, memory, size) =
            app.create_buffer(img_size, usage, mem_props)?;

        app.set_debug_object_name(buffer, "Node ID Buffer")?;

        Ok(Self {
            buffer,
            memory,
            size,
            width,
            height,

            elem_size: elem_size as u32,
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

        let img_size = (width * height * self.elem_size) as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_COHERENT;

        let (buffer, memory, size) =
            app.create_buffer(img_size, usage, mem_props)?;

        app.set_debug_object_name(buffer, "Node ID Buffer")?;

        self.buffer = buffer;
        self.memory = memory;
        self.size = size;
        self.width = width;
        self.height = height;

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
