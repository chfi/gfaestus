use std::collections::HashMap;

use ash::{version::DeviceV1_0, vk, Device};

use anyhow::Result;

use colorous::Gradient;
use rustc_hash::FxHashMap;

use crate::vulkan::GfaestusVk;

use super::Texture;
use super::Texture1D;

pub struct Gradients_ {
    // gradient_offsets: FxHashMap<egui::TextureId, usize>,
    gradient_offsets: FxHashMap<GradientName, usize>,
    pub texture: Texture,
}

impl Gradients_ {
    pub fn initialize(
        app: &GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        width: usize,
    ) -> Result<Self> {
        assert!(
            width.is_power_of_two(),
            "GradientTexture width has to be a power of two"
        );
        let gradient_count = Self::GRADIENT_NAMES.len();

        let height = 64usize;
        let size = width * height;
        assert!(height.is_power_of_two() && height >= gradient_count);

        // let mut gradients: HashMap<egui::TextureId, GradientTexture> =

        let mut gradient_offsets: FxHashMap<GradientName, usize> =
            FxHashMap::default();

        let format = vk::Format::R8G8B8A8_UNORM;

        // TODO fix the usage flags
        let texture = Texture::allocate(
            app,
            command_pool,
            transition_queue,
            width,
            height,
            format,
            vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED,
        )?;

        let buf_size = size * std::mem::size_of::<[u8; 4]>();

        let mut pixels: Vec<u8> = Vec::with_capacity(buf_size);

        for (gradient_id, name) in Self::GRADIENT_NAMES.iter().enumerate() {
            let gradient = name.gradient();

            for i in 0..width {
                let (r, g, b) = gradient.eval_rational(i, width).as_tuple();

                pixels.push(r);
                pixels.push(g);
                pixels.push(b);
                pixels.push(255);
            }

            let offset = pixels.len();

            gradient_offsets.insert(*name, offset);
        }

        for _ in 0..(buf_size - pixels.len()) {
            pixels.push(0);
        }

        texture.copy_from_slice(
            app,
            command_pool,
            transition_queue,
            width,
            height,
            &pixels,
        )?;

        Ok(Self {
            gradient_offsets,
            texture,
        })
    }

    pub const GRADIENT_NAMES: [GradientName; 38] = {
        use GradientName::*;
        [
            Blues,
            BlueGreen,
            BluePurple,
            BrownGreen,
            Cividis,
            Cool,
            CubeHelix,
            Greens,
            GreenBlue,
            Greys,
            Inferno,
            Magma,
            Oranges,
            OrangeRed,
            PinkGreen,
            Plasma,
            Purples,
            PurpleBlue,
            PurpleBlueGreen,
            PurpleGreen,
            PurpleOrange,
            PurpleRed,
            Rainbow,
            Reds,
            RedBlue,
            RedGray,
            RedPurple,
            RedYellowBlue,
            RedYellowGreen,
            Sinebow,
            Spectral,
            Turbo,
            Viridis,
            Warm,
            YellowGreen,
            YellowGreenBlue,
            YellowOrangeBrown,
            YellowOrangeRed,
        ]
    };
}

pub struct Gradients {
    gradients: HashMap<egui::TextureId, GradientTexture>,
}

impl Gradients {
    pub fn gradient(&self, name: GradientName) -> Option<&GradientTexture> {
        let key = name.texture_id();
        self.gradients.get(&key)
    }

    pub fn gradient_from_id(
        &self,
        texture_id: egui::TextureId,
    ) -> Option<&GradientTexture> {
        self.gradients.get(&texture_id)
    }

    pub const GRADIENT_NAMES: [GradientName; 38] = {
        use GradientName::*;
        [
            Blues,
            BlueGreen,
            BluePurple,
            BrownGreen,
            Cividis,
            Cool,
            CubeHelix,
            Greens,
            GreenBlue,
            Greys,
            Inferno,
            Magma,
            Oranges,
            OrangeRed,
            PinkGreen,
            Plasma,
            Purples,
            PurpleBlue,
            PurpleBlueGreen,
            PurpleGreen,
            PurpleOrange,
            PurpleRed,
            Rainbow,
            Reds,
            RedBlue,
            RedGray,
            RedPurple,
            RedYellowBlue,
            RedYellowGreen,
            Sinebow,
            Spectral,
            Turbo,
            Viridis,
            Warm,
            YellowGreen,
            YellowGreenBlue,
            YellowOrangeBrown,
            YellowOrangeRed,
        ]
    };

    pub fn initialize(
        app: &GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        width: usize,
    ) -> Result<Self> {
        let mut gradients: HashMap<egui::TextureId, GradientTexture> =
            HashMap::new();

        for name in std::array::IntoIter::new(Self::GRADIENT_NAMES) {
            let gradient = name.gradient();

            let texture = GradientTexture::new(
                app,
                command_pool,
                transition_queue,
                gradient,
                width,
            )?;

            let key = name.texture_id();

            gradients.insert(key, texture);
        }

        Ok(Self { gradients })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GradientName {
    Blues,
    BlueGreen,
    BluePurple,
    BrownGreen,
    Cividis,
    Cool,
    CubeHelix,
    Greens,
    GreenBlue,
    Greys,
    Inferno,
    Magma,
    Oranges,
    OrangeRed,
    PinkGreen,
    Plasma,
    Purples,
    PurpleBlue,
    PurpleBlueGreen,
    PurpleGreen,
    PurpleOrange,
    PurpleRed,
    Rainbow,
    Reds,
    RedBlue,
    RedGray,
    RedPurple,
    RedYellowBlue,
    RedYellowGreen,
    Sinebow,
    Spectral,
    Turbo,
    Viridis,
    Warm,
    YellowGreen,
    YellowGreenBlue,
    YellowOrangeBrown,
    YellowOrangeRed,
}

impl std::string::ToString for GradientName {
    fn to_string(&self) -> String {
        match self {
            GradientName::Blues => "Blues".to_string(),
            GradientName::BlueGreen => "BlueGreen".to_string(),
            GradientName::BluePurple => "BluePurple".to_string(),
            GradientName::BrownGreen => "BrownGreen".to_string(),
            GradientName::Cividis => "Cividis".to_string(),
            GradientName::Cool => "Cool".to_string(),
            GradientName::CubeHelix => "CubeHelix".to_string(),
            GradientName::Greens => "Greens".to_string(),
            GradientName::GreenBlue => "GreenBlue".to_string(),
            GradientName::Greys => "Greys".to_string(),
            GradientName::Inferno => "Inferno".to_string(),
            GradientName::Magma => "Magma".to_string(),
            GradientName::Oranges => "Oranges".to_string(),
            GradientName::OrangeRed => "OrangeRed".to_string(),
            GradientName::PinkGreen => "PinkGreen".to_string(),
            GradientName::Plasma => "Plasma".to_string(),
            GradientName::Purples => "Purples".to_string(),
            GradientName::PurpleBlue => "PurpleBlue".to_string(),
            GradientName::PurpleBlueGreen => "PurpleBlueGreen".to_string(),
            GradientName::PurpleGreen => "PurpleGreen".to_string(),
            GradientName::PurpleOrange => "PurpleOrange".to_string(),
            GradientName::PurpleRed => "PurpleRed".to_string(),
            GradientName::Rainbow => "Rainbow".to_string(),
            GradientName::Reds => "Reds".to_string(),
            GradientName::RedBlue => "RedBlue".to_string(),
            GradientName::RedGray => "RedGray".to_string(),
            GradientName::RedPurple => "RedPurple".to_string(),
            GradientName::RedYellowBlue => "RedYellowBlue".to_string(),
            GradientName::RedYellowGreen => "RedYellowGreen".to_string(),
            GradientName::Sinebow => "Sinebow".to_string(),
            GradientName::Spectral => "Spectral".to_string(),
            GradientName::Turbo => "Turbo".to_string(),
            GradientName::Viridis => "Viridis".to_string(),
            GradientName::Warm => "Warm".to_string(),
            GradientName::YellowGreen => "YellowGreen".to_string(),
            GradientName::YellowGreenBlue => "YellowGreenBlue".to_string(),
            GradientName::YellowOrangeBrown => "YellowOrangeBrown".to_string(),
            GradientName::YellowOrangeRed => "YellowOrangeRed".to_string(),
        }
    }
}

impl GradientName {
    pub fn gradient(&self) -> Gradient {
        use colorous::*;
        match self {
            GradientName::Blues => BLUES,
            GradientName::BlueGreen => BLUE_GREEN,
            GradientName::BluePurple => BLUE_PURPLE,
            GradientName::BrownGreen => BROWN_GREEN,
            GradientName::Cividis => CIVIDIS,
            GradientName::Cool => COOL,
            GradientName::CubeHelix => CUBEHELIX,
            GradientName::Greens => GREENS,
            GradientName::GreenBlue => GREEN_BLUE,
            GradientName::Greys => GREYS,
            GradientName::Inferno => INFERNO,
            GradientName::Magma => MAGMA,
            GradientName::Oranges => ORANGES,
            GradientName::OrangeRed => ORANGE_RED,
            GradientName::PinkGreen => PINK_GREEN,
            GradientName::Plasma => PLASMA,
            GradientName::Purples => PURPLES,
            GradientName::PurpleBlue => PURPLE_BLUE,
            GradientName::PurpleBlueGreen => PURPLE_BLUE_GREEN,
            GradientName::PurpleGreen => PURPLE_GREEN,
            GradientName::PurpleOrange => PURPLE_ORANGE,
            GradientName::PurpleRed => PURPLE_RED,
            GradientName::Rainbow => RAINBOW,
            GradientName::Reds => REDS,
            GradientName::RedBlue => RED_BLUE,
            GradientName::RedGray => RED_GREY,
            GradientName::RedPurple => RED_PURPLE,
            GradientName::RedYellowBlue => RED_YELLOW_BLUE,
            GradientName::RedYellowGreen => RED_YELLOW_GREEN,
            GradientName::Sinebow => SINEBOW,
            GradientName::Spectral => SPECTRAL,
            GradientName::Turbo => TURBO,
            GradientName::Viridis => VIRIDIS,
            GradientName::Warm => WARM,
            GradientName::YellowGreen => YELLOW_GREEN,
            GradientName::YellowGreenBlue => YELLOW_GREEN_BLUE,
            GradientName::YellowOrangeBrown => YELLOW_ORANGE_BROWN,
            GradientName::YellowOrangeRed => YELLOW_ORANGE_RED,
        }
    }

    pub fn texture_id(&self) -> egui::TextureId {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::default();

        self.hash(&mut hasher);

        let hash = hasher.finish();

        egui::TextureId::User(hash)
    }
}

pub struct GradientTexture {
    pub texture: Texture1D,
}

impl GradientTexture {
    pub fn new(
        app: &GfaestusVk,
        command_pool: vk::CommandPool,
        transition_queue: vk::Queue,
        gradient: Gradient,
        width: usize,
    ) -> Result<Self> {
        assert!(
            width.is_power_of_two(),
            "GradientTexture width has to be a power of two"
        );

        let mut colors: Vec<rgb::RGB<f32>> = Vec::with_capacity(width);

        for i in 0..width {
            let (r, g, b) = gradient.eval_rational(i, width).as_tuple();

            let r = (r as f32) / 255.0;
            let g = (g as f32) / 255.0;
            let b = (b as f32) / 255.0;

            let rgb_color = rgb::RGB::new(r, g, b);

            colors.push(rgb_color);
        }

        let texture = Texture1D::create_from_colors(
            app,
            command_pool,
            transition_queue,
            &colors,
        )?;

        Ok(Self { texture })
    }

    pub fn create_sampler(device: &Device) -> Result<vk::Sampler> {
        let sampler = {
            let sampler_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .anisotropy_enable(false)
                .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
                .unnormalized_coordinates(false)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .mip_lod_bias(0.0)
                .min_lod(0.0)
                .max_lod(1.0)
                .build();

            unsafe { device.create_sampler(&sampler_info, None) }
        }?;

        Ok(sampler)
    }
}
