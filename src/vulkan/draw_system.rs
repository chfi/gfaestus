use ash::{
    extensions::{
        ext::DebugReport,
        khr::{Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
};
use ash::{vk, Device, Entry, Instance};

use std::ffi::CString;

use nalgebra_glm as glm;

use anyhow::Result;

use super::SwapchainProperties;

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

        Ok(buffers[0])
    }
}

pub struct NodeDrawAsh {
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,

    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,

    vertex_buffer: vk::Buffer,
    vertex_buffer_memory: vk::DeviceMemory,

    uniform_buffer: vk::Buffer,
    uniform_buffer_memory: vk::DeviceMemory,

    descriptor_set: vk::DescriptorSet,
}

// pub struct NodesUBO {
//     matrix: glm::Mat4,
// }

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

impl NodeDrawAsh {
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
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        let vert_src =
            read_shader_from_file("shaders/nodes_simple.vert.spv").unwrap();
        let frag_src =
            read_shader_from_file("shaders/nodes_simple.vert.spv").unwrap();

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
    }

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
