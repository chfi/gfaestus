use ash::vk;

use anyhow::*;

use crate::vulkan::{
    context::NodeRendererType, draw_system::Vertex, GfaestusVk,
};

pub struct NodeVertices {
    pub(crate) vertex_count: usize,

    pub(crate) vertex_buffer: vk::Buffer,

    allocation: vk_mem::Allocation,
    allocation_info: Option<vk_mem::AllocationInfo>,

    renderer_type: NodeRendererType,
}

impl NodeVertices {
    pub fn new(renderer_type: NodeRendererType) -> Self {
        let vertex_count = 0;
        let vertex_buffer = vk::Buffer::null();

        let allocation = vk_mem::Allocation::null();
        let allocation_info = None;

        Self {
            vertex_count,
            vertex_buffer,
            allocation,
            allocation_info,

            renderer_type,
        }
    }

    pub fn buffer(&self) -> vk::Buffer {
        self.vertex_buffer
    }

    pub fn has_vertices(&self) -> bool {
        self.allocation_info.is_some()
    }

    pub fn destroy(&mut self, app: &GfaestusVk) -> Result<()> {
        if self.has_vertices() {
            app.allocator
                .destroy_buffer(self.vertex_buffer, &self.allocation)?;

            self.vertex_buffer = vk::Buffer::null();
            self.allocation = vk_mem::Allocation::null();
            self.allocation_info = None;

            self.vertex_count = 0;
        }

        Ok(())
    }

    /// `line` as in the vertex input to the node tessellation stage is
    /// one line, or one pair of points, per node
    ///
    /// the input is one pair of vertices per node
    fn upload_line_vertices(
        &mut self,
        app: &GfaestusVk,
        vertices: &[Vertex],
    ) -> Result<()> {
        assert!(self.renderer_type == NodeRendererType::TessellationQuads);

        if self.has_vertices() {
            self.destroy(app)?;
        }

        let usage = vk::BufferUsageFlags::VERTEX_BUFFER
            | vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_SRC;
        let memory_usage = vk_mem::MemoryUsage::GpuOnly;

        let (buffer, allocation, allocation_info) =
            app.create_buffer_with_data(usage, memory_usage, false, &vertices)?;

        app.set_debug_object_name(buffer, "Node Vertex Buffer (Lines)")?;

        self.vertex_count = vertices.len();

        self.vertex_buffer = buffer;
        self.allocation = allocation;
        self.allocation_info = Some(allocation_info);

        Ok(())
    }

    /// `quad` as in the vertex input to the node pipeline that doesn't
    /// use tessellation is one quad (2 triangles) per node
    ///
    /// the input is the same as `upload_line_vertices`, but the
    /// function repeats vertices to produce two triangles per node --
    /// so the uploaded vertices are at the same points as in the line
    /// case
    fn upload_quad_vertices(
        &mut self,
        app: &GfaestusVk,
        vertices: &[Vertex],
    ) -> Result<()> {
        assert!(self.renderer_type == NodeRendererType::VertexOnly);

        if self.has_vertices() {
            self.destroy(app)?;
        }

        let usage = vk::BufferUsageFlags::VERTEX_BUFFER
            | vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_SRC;
        let memory_usage = vk_mem::MemoryUsage::GpuOnly;

        let mut quad_vertices: Vec<Vertex> =
            Vec::with_capacity(vertices.len() * 3);

        // NB: first triangle, if p0 is the left side, goes "bottom
        // left, top right, top left"; second is "bottom left, bottom
        // right, top right"
        for chunk in vertices.chunks_exact(2) {
            if let &[p0, p1] = chunk {
                quad_vertices.push(p0);
                quad_vertices.push(p1);
                quad_vertices.push(p0);

                quad_vertices.push(p0);
                quad_vertices.push(p1);
                quad_vertices.push(p1);
            }
        }

        let vertices = quad_vertices;

        let (buffer, allocation, allocation_info) =
            app.create_buffer_with_data(usage, memory_usage, false, &vertices)?;

        app.set_debug_object_name(buffer, "Node Vertex Buffer (Quads)")?;

        self.vertex_count = vertices.len();

        self.vertex_buffer = buffer;
        self.allocation = allocation;
        self.allocation_info = Some(allocation_info);

        Ok(())
    }

    pub fn upload_vertices(
        &mut self,
        app: &GfaestusVk,
        vertices: &[Vertex],
    ) -> Result<()> {
        match self.renderer_type {
            NodeRendererType::VertexOnly => {
                self.upload_quad_vertices(app, vertices)
            }
            NodeRendererType::TessellationQuads => {
                self.upload_line_vertices(app, vertices)
            }
        }
    }

    pub fn download_vertices(
        &self,
        app: &GfaestusVk,
        node_count: usize,
        target: &mut Vec<crate::universe::Node>,
    ) -> Result<()> {
        target.clear();
        let cap = target.capacity();
        if cap < node_count {
            target.reserve(node_count - cap);
        }

        let alloc_info = self.allocation_info.as_ref().unwrap();

        let staging_buffer_info = vk::BufferCreateInfo::builder()
            .size(alloc_info.get_size() as u64)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .build();

        let staging_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuToCpu,
            flags: vk_mem::AllocationCreateFlags::MAPPED,
            ..Default::default()
        };

        let (staging_buf, staging_alloc, staging_alloc_info) = app
            .allocator
            .create_buffer(&staging_buffer_info, &staging_create_info)?;

        app.set_debug_object_name(
            staging_buf,
            "Node Position Download Staging Buffer",
        )?;

        GfaestusVk::copy_buffer(
            app.vk_context().device(),
            app.transient_command_pool,
            app.graphics_queue,
            self.buffer(),
            staging_buf,
            staging_alloc_info.get_size() as u64,
        );

        unsafe {
            let mapped_ptr = staging_alloc_info.get_mapped_data();

            let val_ptr = mapped_ptr as *const crate::universe::Node;

            let sel_slice = std::slice::from_raw_parts(val_ptr, node_count);

            match self.renderer_type {
                NodeRendererType::TessellationQuads => {
                    // if it uses just two vertices per node, copy the entire slice
                    target.extend_from_slice(sel_slice);
                }
                NodeRendererType::VertexOnly => {
                    // if it uses six vertices per node, only read every
                    // third pair of vertices (per the vx order in
                    // upload_vertices)

                    // TODO this probably works; but untested (could be
                    // parallelized too)
                    for n in 0..node_count {
                        let ix = n * 3;
                        target.push(sel_slice[ix]);
                    }
                }
            }
        }

        app.allocator.destroy_buffer(staging_buf, &staging_alloc)?;

        target.shrink_to_fit();

        Ok(())
    }
}
