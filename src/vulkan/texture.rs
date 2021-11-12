use ash::{version::DeviceV1_0, vk, Device};

use anyhow::Result;

pub mod color_schemes;

pub use color_schemes::*;

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

    pub fn allocate(
        app: &super::GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        width: usize,
        height: usize,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Result<Self> {
        use vk::ImageLayout as Layout;
        use vk::ImageUsageFlags as ImgUsage;

        let vk_context = app.vk_context();
        let device = vk_context.device();

        let extent = vk::Extent3D {
            width: width as u32,
            height: height as u32,
            depth: 1,
        };
        let img_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            .tiling(vk::ImageTiling::OPTIMAL)
            // .tiling(vk::ImageTiling::LINEAR)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(usage)
            //     ImgUsage::TRANSFER_SRC
            //         | ImgUsage::TRANSFER_DST
            //         | ImgUsage::STORAGE
            //         | vk::ImageUsageFlags::SAMPLED,
            // )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty())
            .build();

        log::debug!("Creating image {:?}", img_info);
        let image = unsafe { device.create_image(&img_info, None) }?;
        let mem_reqs = unsafe { device.get_image_memory_requirements(image) };
        let mem_type_ix = super::find_memory_type(
            mem_reqs,
            vk_context.get_mem_properties(),
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        );

        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(mem_reqs.size)
            .memory_type_index(mem_type_ix)
            .build();

        log::debug!("Allocating {} bytes of memory for image", mem_reqs.size);
        let memory = unsafe {
            let mem = device.allocate_memory(&alloc_info, None)?;
            device.bind_image_memory(image, mem, 0)?;
            mem
        };

        log::debug!("Transitioning image to SHADER_READ_ONLY_OPTIMAL");
        super::GfaestusVk::transition_image(
            device,
            command_pool,
            transition_queue,
            image,
            Layout::UNDEFINED,
            Layout::SHADER_READ_ONLY_OPTIMAL,
        )?;

        let view = {
            let create_info = vk::ImageViewCreateInfo::builder()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();

            unsafe { device.create_image_view(&create_info, None) }
        }?;
        // log::debug!("Created image view {:?}", view);
        log::debug!("Created image view");

        let texture = Self::new(image, memory, view, None);
        log::debug!("Image created: {:?}", image);

        Ok(texture)
    }

    pub fn copy_from_slice(
        &self,
        app: &super::GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        width: usize,
        height: usize,
        data: &[u8],
    ) -> Result<()> {
        let usage = vk::BufferUsageFlags::TRANSFER_SRC;
        let memory_usage = vk_mem::MemoryUsage::GpuOnly;

        let (staging_buf, staging_alloc, _) =
            app.create_buffer_with_data(usage, memory_usage, false, &data)?;

        self.copy_from_buffer(
            app,
            command_pool,
            transition_queue,
            staging_buf,
            width,
            height,
        )?;

        app.allocator.destroy_buffer(staging_buf, &staging_alloc)?;

        Ok(())
    }

    pub fn copy_from_buffer(
        &self,
        app: &super::GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        buffer: vk::Buffer,
        width: usize,
        height: usize,
    ) -> Result<()> {
        use vk::ImageLayout as Layout;

        let extent = vk::Extent2D {
            width: width as u32,
            height: height as u32,
        };

        let device = app.vk_context().device();

        log::debug!("Copying buffer into texture");

        super::GfaestusVk::transition_image(
            device,
            command_pool,
            transition_queue,
            self.image,
            Layout::UNDEFINED,
            Layout::TRANSFER_DST_OPTIMAL,
        )?;

        super::GfaestusVk::copy_buffer_to_image(
            device,
            command_pool,
            transition_queue,
            buffer,
            self.image,
            extent,
        )?;

        super::GfaestusVk::transition_image(
            device,
            command_pool,
            transition_queue,
            self.image,
            Layout::TRANSFER_DST_OPTIMAL,
            Layout::SHADER_READ_ONLY_OPTIMAL,
        )?;

        Ok(())
    }

    pub fn from_pixel_bytes(
        app: &super::GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        width: usize,
        height: usize,
        pixels: &[u8],
    ) -> Result<Self> {
        use vk::BufferUsageFlags as BufUsage;
        use vk::ImageLayout as Layout;
        use vk::ImageUsageFlags as ImgUsage;
        use vk::MemoryPropertyFlags as MemProps;

        let vk_context = app.vk_context();
        let device = vk_context.device();

        let format = vk::Format::R8_UNORM;

        let image_size =
            (pixels.len() * std::mem::size_of::<u8>()) as vk::DeviceSize;

        log::debug!(
            "Creating {}x{} R8_UNORM texture from pixel slice",
            width,
            height
        );

        let (buffer, buf_mem, buf_size) = app.create_buffer(
            image_size,
            BufUsage::TRANSFER_SRC,
            MemProps::HOST_VISIBLE | MemProps::HOST_COHERENT,
        )?;

        log::debug!("Created staging buffer");

        unsafe {
            let ptr = device.map_memory(
                buf_mem,
                0,
                image_size,
                vk::MemoryMapFlags::empty(),
            )?;

            let mut align = ash::util::Align::new(
                ptr,
                std::mem::align_of::<u8>() as _,
                buf_size,
            );
            align.copy_from_slice(&pixels);
            device.unmap_memory(buf_mem);
        }

        log::debug!("Copied pixels into staging buffer");
        let extent = vk::Extent3D {
            width: width as u32,
            height: height as u32,
            depth: 1,
        };

        let img_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            .tiling(vk::ImageTiling::OPTIMAL)
            // .tiling(vk::ImageTiling::LINEAR)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(
                ImgUsage::TRANSFER_SRC
                    | ImgUsage::TRANSFER_DST
                    | vk::ImageUsageFlags::SAMPLED,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty())
            .build();

        log::debug!("Creating image {:?}", img_info);
        let image = unsafe { device.create_image(&img_info, None) }?;
        let mem_reqs = unsafe { device.get_image_memory_requirements(image) };
        let mem_type_ix = super::find_memory_type(
            mem_reqs,
            vk_context.get_mem_properties(),
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        );

        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(mem_reqs.size)
            .memory_type_index(mem_type_ix)
            .build();

        let memory = unsafe {
            let mem = device.allocate_memory(&alloc_info, None)?;
            device.bind_image_memory(image, mem, 0)?;
            mem
        };

        {
            super::GfaestusVk::transition_image(
                device,
                command_pool,
                transition_queue,
                image,
                Layout::UNDEFINED,
                Layout::TRANSFER_DST_OPTIMAL,
            )?;

            super::GfaestusVk::copy_buffer_to_image(
                device,
                command_pool,
                transition_queue,
                buffer,
                image,
                vk::Extent2D {
                    width: extent.width,
                    height: extent.height,
                },
            )?;

            super::GfaestusVk::transition_image(
                device,
                command_pool,
                transition_queue,
                image,
                Layout::TRANSFER_DST_OPTIMAL,
                Layout::SHADER_READ_ONLY_OPTIMAL,
            )?;
        }

        log::debug!("Filled image from staging buffer");
        let view = {
            let create_info = vk::ImageViewCreateInfo::builder()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();

            unsafe { device.create_image_view(&create_info, None) }
        }?;

        unsafe {
            device.destroy_buffer(buffer, None);
            device.free_memory(buf_mem, None);
        }

        Ok(Self::new(image, memory, view, None))
    }

    pub fn null() -> Self {
        Texture {
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
            view: vk::ImageView::null(),
            sampler: None,
        }
    }

    pub fn is_null(&self) -> bool {
        self.image == vk::Image::null()
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

    pub fn create_attachment_image(
        vk_context: &super::context::VkContext,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        usage: vk::ImageUsageFlags,
        layout: vk::ImageLayout,
        extent: vk::Extent2D,
        format: vk::Format,
        sampler: Option<vk::Sampler>,
    ) -> Result<Self> {
        use vk::ImageLayout as Layout;

        log::trace!("creating attachment image");

        let (img, mem) = super::GfaestusVk::create_image(
            vk_context,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            extent,
            vk::SampleCountFlags::TYPE_1,
            format,
            vk::ImageTiling::OPTIMAL,
            usage,
        )?;

        super::GfaestusVk::transition_image(
            vk_context.device(),
            command_pool,
            transition_queue,
            img,
            Layout::UNDEFINED,
            layout,
        )?;

        let view = super::GfaestusVk::create_image_view(
            vk_context.device(),
            img,
            1,
            format,
            vk::ImageAspectFlags::COLOR,
        )?;

        Ok(Self::new(img, mem, view, sampler))
    }

    pub fn create_transient_color(
        vk_context: &super::context::VkContext,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        swapchain_props: super::SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<Self> {
        let format = swapchain_props.format.format;

        log::trace!("creating transient color image");

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

#[derive(Clone, Copy)]
pub struct Texture1D {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
}

impl Texture1D {
    pub fn new(
        image: vk::Image,
        memory: vk::DeviceMemory,
        view: vk::ImageView,
    ) -> Self {
        Texture1D {
            image,
            memory,
            view,
        }
    }

    pub fn destroy(&mut self, device: &Device) {
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }

    pub fn create_from_colors(
        app: &super::GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        colors: &[rgb::RGB<f32>],
    ) -> Result<Self> {
        use vk::BufferUsageFlags as BufUsage;
        use vk::ImageLayout as Layout;
        use vk::ImageUsageFlags as ImgUsage;
        use vk::MemoryPropertyFlags as MemProps;

        let vk_context = app.vk_context();
        let device = vk_context.device();

        let format = vk::Format::R8G8B8A8_UNORM;

        let image_size =
            (colors.len() * 4 * std::mem::size_of::<u8>()) as vk::DeviceSize;

        let (buffer, buf_mem, buf_size) = app.create_buffer(
            image_size,
            BufUsage::TRANSFER_SRC,
            MemProps::HOST_VISIBLE | MemProps::HOST_COHERENT,
        )?;

        let mut pixels: Vec<u8> = Vec::with_capacity(colors.len() * 4);

        for &color in colors {
            let r = (color.r * 255.0).floor() as u8;
            let g = (color.g * 255.0).floor() as u8;
            let b = (color.b * 255.0).floor() as u8;
            let a = 255u8;

            pixels.push(r);
            pixels.push(g);
            pixels.push(b);
            pixels.push(a);
        }

        unsafe {
            let ptr = device.map_memory(
                buf_mem,
                0,
                image_size,
                vk::MemoryMapFlags::empty(),
            )?;

            let mut align = ash::util::Align::new(
                ptr,
                std::mem::align_of::<u8>() as _,
                buf_size,
            );
            align.copy_from_slice(&pixels);
            device.unmap_memory(buf_mem);
        }

        let extent = vk::Extent3D {
            width: colors.len() as u32,
            height: 1,
            depth: 1,
        };

        let img_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_1D)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            // .tiling(vk::ImageTiling::LINEAR)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(
                ImgUsage::TRANSFER_SRC
                    | ImgUsage::TRANSFER_DST
                    | ImgUsage::SAMPLED,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty())
            .build();

        let image = unsafe { device.create_image(&img_info, None) }?;
        let mem_reqs = unsafe { device.get_image_memory_requirements(image) };
        let mem_type_ix = super::find_memory_type(
            mem_reqs,
            vk_context.get_mem_properties(),
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        );

        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(mem_reqs.size)
            .memory_type_index(mem_type_ix)
            .build();

        let memory = unsafe {
            let mem = device.allocate_memory(&alloc_info, None)?;
            device.bind_image_memory(image, mem, 0)?;
            mem
        };

        {
            super::GfaestusVk::transition_image(
                device,
                command_pool,
                transition_queue,
                image,
                Layout::UNDEFINED,
                Layout::TRANSFER_DST_OPTIMAL,
            )?;

            super::GfaestusVk::copy_buffer_to_image(
                device,
                command_pool,
                transition_queue,
                buffer,
                image,
                vk::Extent2D {
                    width: extent.width,
                    height: 1,
                },
            )?;

            super::GfaestusVk::transition_image(
                device,
                command_pool,
                transition_queue,
                image,
                Layout::TRANSFER_DST_OPTIMAL,
                Layout::SHADER_READ_ONLY_OPTIMAL,
            )?;
        }

        let view = {
            let create_info = vk::ImageViewCreateInfo::builder()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_1D)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();

            unsafe { device.create_image_view(&create_info, None) }
        }?;

        unsafe {
            device.destroy_buffer(buffer, None);
            device.free_memory(buf_mem, None);
        }

        Ok(Self::new(image, memory, view))
    }
}
