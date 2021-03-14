use ash::{
    extensions::{
        ext::DebugReport,
        khr::{Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
};
use ash::{vk, Device, Entry, Instance};

use nalgebra_glm as glm;

use anyhow::Result;

use super::SwapchainProperties;

pub struct GfaestusCmdBuf {
    command_buffer: vk::CommandBuffer,
}

impl GfaestusCmdBuf {
    pub fn frame(
        device: &Device,
        pool: vk::CommandPool,
        render_pass: vk::RenderPass,
        framebuffer: &vk::Framebuffer,
        swapchain_props: SwapchainProperties,
        // vertex_buffer: vk::Buffer,
        // pipeline_layout: vk::PipelineLayout,
        // descriptor_sets: &[vk::DescriptorSet],
        // graphics_pipeline: vk::Pipeline,
    ) -> Result<vk::CommandBuffer> {
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1)
            .build();

        let buffers = unsafe { device.allocate_command_buffers(&alloc_info) }?;

        for (i, buf) in buffers.iter().enumerate() {
            let buf = *buf;
            // let framebuffer = framebuffers[i];

            {
                let cmd_buf_begin_info = vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                    .build();

                unsafe {
                    device.begin_command_buffer(buf, &cmd_buf_begin_info)
                }?;
            }

            {
                let clear_values = [
                    vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    },
                    vk::ClearValue {
                        depth_stencil: vk::ClearDepthStencilValue {
                            depth: 1.0,
                            stencil: 0,
                        },
                    },
                ];

                let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
                    .render_pass(render_pass)
                    .framebuffer(*framebuffer)
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: swapchain_props.extent,
                    })
                    .clear_values(&clear_values)
                    .build();

                unsafe {
                    device.cmd_begin_render_pass(
                        buf,
                        &render_pass_begin_info,
                        vk::SubpassContents::INLINE,
                    )
                };
            }

            // TODO bind pipeline
            // TODO bind buffers
            // TODO draw

            unsafe { device.cmd_end_render_pass(buf) };

            unsafe { device.end_command_buffer(buf) }?;
        }

        Ok(buffers)
    }
}

pub struct NodeDrawAsh {
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    vertex_buffer: vk::Buffer,
    vertex_buffer_memory: vk::DeviceMemory,

    // uniform_buffer: vk::Buffer,
    // uniform_buffer_memory: vk::DeviceMemory,
    descriptor_set: vk::DescriptorSet,
}

// pub struct NodesUBO {
//     matrix: glm::Mat4,
// }

impl NodeDrawAsh {
    pub fn new(
        desc_pool: vk::DescriptorPool,
        render_pass: vk::RenderPass,
    ) -> Result<Self> {
        unimplemented!();
    }

    fn descriptor_set_layout(device: &Device) -> vk::DescriptorSetLayout {
        // let ubo_binding = Unif

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&[])
            .build();

        unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .unwrap()
        }
    }
}
