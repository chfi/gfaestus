use vulkano::{
    device::Queue,
    format::{Format, R8G8B8A8Unorm},
    framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract},
    image::{
        AttachmentImage, Dimensions, ImageAccess, ImageUsage, ImageViewAccess,
        ImmutableImage, MipmapsCount,
    },
    instance::PhysicalDevice,
};

use vulkano::sampler::{
    Filter, MipmapMode, Sampler, SamplerAddressMode,
    UnnormalizedSamplerAddressMode,
};

use vulkano::sync::GpuFuture;

use rgb::*;

use anyhow::Result;

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crossbeam::atomic::AtomicCell;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Hash)]
pub enum ThemeId {
    Light,
    Dark,
    Custom(u32),
}

/// A theme definition that can be transformed into theme data usable by the GPU
pub struct ThemeDef {
    background: RGB<f32>,
    node_colors: Vec<RGB<f32>>,
}

/// A theme represented as a clear value-compatible background color,
/// and an immutable image that can be indexed by node ID in the
/// fragment shader
pub struct Theme {
    background: [f32; 4],
    node_colors: Arc<ImmutableImage<R8G8B8A8Unorm>>,
    color_period: u32,

    is_uploaded: AtomicCell<bool>,
}

impl Theme {
    pub fn clear(&self) -> vulkano::format::ClearValue {
        vulkano::format::ClearValue::Float(self.background)
    }

    pub fn texture(&self) -> &Arc<ImmutableImage<R8G8B8A8Unorm>> {
        &self.node_colors
    }

    pub fn width(&self) -> u32 {
        self.color_period
    }

    fn from_theme_def(
        queue: &Arc<Queue>,
        theme_def: &ThemeDef,
    ) -> Result<(Theme, Box<dyn GpuFuture>)> {
        let background = {
            let bg = theme_def.background;
            [bg.r, bg.g, bg.b, 1.0]
        };

        let color_period = theme_def.node_colors.len() as u32;

        let (node_colors, future) = ImmutableImage::from_iter(
            theme_def.node_colors.iter().map(|&nc| {
                [
                    (255.0 * nc.r).floor() as u8,
                    (255.0 * nc.g).floor() as u8,
                    (255.0 * nc.b).floor() as u8,
                    255,
                ]
            }),
            // .map(|&nc| [nc.r, nc.g, nc.b, 255]),
            Dimensions::Dim1d {
                width: color_period,
            },
            MipmapsCount::One,
            R8G8B8A8Unorm,
            queue.clone(),
        )?;

        let is_uploaded = AtomicCell::new(false);

        Ok((
            Theme {
                background,
                node_colors,
                color_period,
                is_uploaded,
            },
            future.boxed(),
        ))
    }
}

const RAINBOW: [(f32, f32, f32); 7] = [
    (1.0, 0.0, 0.0),
    (1.0, 0.65, 0.0),
    (1.0, 1.0, 0.0),
    (0.0, 0.5, 0.0),
    (0.0, 0.0, 1.0),
    (0.3, 0.0, 0.51),
    (0.93, 0.51, 0.93),
];

pub fn light_default() -> ThemeDef {
    let background = RGB::new(1.0, 1.0, 1.0);

    // use rainbow theme for node colors in both light and dark themes for now
    let node_colors =
        RAINBOW.iter().copied().map(RGB::from).collect::<Vec<_>>();

    ThemeDef {
        background,
        node_colors,
    }
}

pub fn dark_default() -> ThemeDef {
    let background = RGB::new(0.0, 0.0, 0.05);

    // use rainbow theme for node colors in both light and dark themes for now
    let node_colors =
        RAINBOW.iter().copied().map(RGB::from).collect::<Vec<_>>();

    ThemeDef {
        background,
        node_colors,
    }
}

/// The running app's theme state, including the active & all uploaded
/// themes. Tracks the theme texture's GPU upload state (to an extent),
/// and whether draw systems using the active texture needs to recreate
/// its descriptor sets due to new texture uploads.
pub struct Themes {
    active: ThemeId,

    light: Theme,
    dark: Theme,
    custom: FxHashMap<u32, Theme>,

    sampler: Arc<Sampler>,

    queue: Arc<Queue>,

    /// if this is Some(future), the future must be joined before the
    /// active theme is used in the renderer
    future: Option<Box<dyn GpuFuture>>,

    need_rebuild: AtomicCell<bool>,

    descriptor_set_count: AtomicCell<usize>,
    need_new_descriptor_set: AtomicCell<bool>,
}

impl Themes {
    pub fn new_from_light_and_dark(
        queue: Arc<Queue>,
        light: &ThemeDef,
        dark: &ThemeDef,
    ) -> Result<Themes> {
        let active = ThemeId::Light;

        let (light, light_fut) = Theme::from_theme_def(&queue, light)?;
        let (dark, dark_fut) = Theme::from_theme_def(&queue, dark)?;

        let custom: FxHashMap<u32, Theme> = FxHashMap::default();

        let future = Some(light_fut.join(dark_fut).boxed());

        // NB the theme's period will have to be provided to the
        // shader if the sampler is normalized or not, unless we make
        // all theme textures |nodes| wide

        let sampler = Sampler::new(
            queue.device().clone(),
            Filter::Nearest,
            Filter::Nearest,
            MipmapMode::Linear,
            SamplerAddressMode::Repeat,
            SamplerAddressMode::Repeat,
            SamplerAddressMode::Repeat,
            0.0,
            1.0,
            0.0,
            1.0,
        )?;

        // let sampler = Sampler::unnormalized(queue.device().clone(),
        //                                     Filter::Nearest,
        //                                     UnnormalizedSamplerAddressMode::ClampToEdge,
        //                                     UnnormalizedSamplerAddressMode::ClampToEdge)?;

        let need_rebuild = AtomicCell::new(false);
        let descriptor_set_count = AtomicCell::new(0);
        let need_new_descriptor_set = AtomicCell::new(true);

        Ok(Themes {
            active,

            light,
            dark,
            custom,

            sampler,

            queue,

            future,

            need_rebuild,

            descriptor_set_count,
            need_new_descriptor_set,
        })
    }

    /// Take the future signifying all theme texture uploads, and tag
    /// all themes as being uploaded. The future *must* be synced
    /// before any texture theme is used!
    #[must_use = "taking the Themes future assumes that the future will be joined before the theme is used"]
    pub fn take_future(&mut self) -> Option<Box<dyn GpuFuture>> {
        self.light.is_uploaded.store(true);
        self.dark.is_uploaded.store(true);

        self.custom
            .values_mut()
            .for_each(|t| t.is_uploaded.store(true));

        std::mem::take(&mut self.future)
    }

    /// Returns the active theme if it's ready to use
    pub fn active_theme(&self) -> Option<&Theme> {
        let theme = match self.active {
            ThemeId::Light => &self.light,
            ThemeId::Dark => &self.dark,
            ThemeId::Custom(id) => self.custom.get(&id)?,
        };

        theme.is_uploaded.take().then(|| theme)
    }
}
