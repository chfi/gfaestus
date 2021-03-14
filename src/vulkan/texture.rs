use ash::{version::DeviceV1_0, vk, Device};

use anyhow::Result;

#[derive(Clone, Copy)]
pub struct Texture {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
    pub sampler: Option<vk::Sampler>,
}

impl Texture {
    pub fn new(
        image: vk::Image,
        memory: vk::DeviceMemory,
        view: vk::ImageView,
        sampler: Option<vk::Sampler>,
    ) -> Self {
        Texture {
            image,
            memory,
            view,
            sampler,
        }
    }

    pub fn destroy(&mut self, device: &Device) {
        unsafe {
            if let Some(sampler) = self.sampler.take() {
                device.destroy_sampler(sampler, None);
            }
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }

    pub fn create_transient_color(
        vk_context: &super::context::VkContext,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        swapchain_props: super::SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<Self> {
        let format = swapchain_props.format.format;

        use vk::ImageLayout as Layout;
        use vk::ImageUsageFlags as Usage;

        let (img, mem) = super::GfaestusVk::create_image(
            vk_context,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            swapchain_props.extent,
            msaa_samples,
            format,
            vk::ImageTiling::OPTIMAL,
            Usage::TRANSIENT_ATTACHMENT | Usage::COLOR_ATTACHMENT,
        )?;

        super::GfaestusVk::transition_image(
            vk_context.device(),
            command_pool,
            transition_queue,
            img,
            format,
            Layout::UNDEFINED,
            Layout::COLOR_ATTACHMENT_OPTIMAL,
        )?;

        let view = super::GfaestusVk::create_image_view(
            vk_context.device(),
            img,
            1,
            format,
            vk::ImageAspectFlags::COLOR,
        )?;

        Ok(Self::new(img, mem, view, None))
    }
}
