use vulkano::{
    device::Queue,
    format::R8G8B8A8Unorm,
    image::{Dimensions, ImmutableImage, MipmapsCount},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};

use rgb::*;

use anyhow::Result;

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crossbeam::atomic::AtomicCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
pub enum ThemeId {
    Primary,
    Secondary,
}

/// A theme definition that can be transformed into theme data usable by the GPU
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeDef {
    pub background: RGB<f32>,
    pub node_colors: Vec<RGB<f32>>,
}

/// A theme represented as a clear value-compatible background color,
/// and an immutable image that can be indexed by node ID in the
/// fragment shader
#[derive(Debug)]
pub struct Theme {
    background: [f32; 4],
    node_colors: Arc<ImmutableImage<R8G8B8A8Unorm>>,

    color_hash: u64,

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

    pub fn color_hash(&self) -> u64 {
        self.color_hash
    }

    pub fn bg_luma(&self) -> f32 {
        let [r, g, b, _] = self.background;
        let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        luminance
    }

    pub fn is_dark(&self) -> bool {
        self.bg_luma() < 0.5
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

        let mut colors_u8 = Vec::with_capacity(theme_def.node_colors.len() * 4);

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        for &color in theme_def.node_colors.iter() {
            let r = (255.0 * color.r).floor();
            let g = (255.0 * color.g).floor();
            let b = (255.0 * color.b).floor();
            let a = 255u8;

            (r as u8).hash(&mut hasher);
            (g as u8).hash(&mut hasher);
            (b as u8).hash(&mut hasher);

            colors_u8.push(r as u8);
            colors_u8.push(g as u8);
            colors_u8.push(b as u8);
            colors_u8.push(a);
        }

        let color_hash = hasher.finish();

        let (node_colors, future) = ImmutableImage::from_iter(
            colors_u8.into_iter(),
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
                color_hash,
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

const RGB_NODES: [(f32, f32, f32); 6] = [
    (1.0, 0.0, 0.0),
    (1.0, 0.0, 0.0),
    (0.0, 1.0, 0.0),
    (0.0, 1.0, 0.0),
    (0.0, 0.0, 1.0),
    (0.0, 0.0, 1.0),
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
    queue: Arc<Queue>,
    active: ThemeId,

    primary: Theme,
    secondary: Theme,

    primary_def: ThemeDef,
    secondary_def: ThemeDef,

    custom: FxHashMap<u32, Theme>,

    sampler: Arc<Sampler>,

    /// if this is Some(future), the future must be joined before the
    /// active theme is used in the renderer
    future: Option<Box<dyn GpuFuture>>,
}

impl Themes {
    pub fn new_from_primary_and_secondary(
        queue: Arc<Queue>,
        primary: &ThemeDef,
        secondary: &ThemeDef,
    ) -> Result<Themes> {
        let active = ThemeId::Primary;

        let primary_def = primary.clone();
        let secondary_def = secondary.clone();

        let (primary, primary_fut) = Theme::from_theme_def(&queue, primary)?;
        let (secondary, secondary_fut) =
            Theme::from_theme_def(&queue, secondary)?;

        let custom: FxHashMap<u32, Theme> = FxHashMap::default();

        let future = Some(primary_fut.join(secondary_fut).boxed());

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

        Ok(Themes {
            queue,
            active,

            primary,
            secondary,

            primary_def,
            secondary_def,

            custom,

            sampler,

            future,
        })
    }

    pub fn sampler(&self) -> &Arc<Sampler> {
        &self.sampler
    }

    pub fn primary(&self) -> &Theme {
        &self.primary
    }

    pub fn secondary(&self) -> &Theme {
        &self.secondary
    }

    pub fn get_theme_def(&self, theme_id: ThemeId) -> &ThemeDef {
        match theme_id {
            ThemeId::Primary => &self.primary_def,
            ThemeId::Secondary => &self.secondary_def,
        }
    }

    pub fn replace_theme_def(
        &mut self,
        theme_id: ThemeId,
        theme_def: ThemeDef,
    ) -> Result<()> {
        let (theme, future) = Theme::from_theme_def(&self.queue, &theme_def)?;

        match theme_id {
            ThemeId::Primary => {
                self.primary_def = theme_def;
                self.primary = theme;
            }
            ThemeId::Secondary => {
                self.secondary_def = theme_def;
                self.secondary = theme;
            }
        }

        self.future = Some(future);

        Ok(())
    }

    pub fn set_theme(&mut self, theme_id: ThemeId) -> ThemeId {
        let new_theme = match theme_id {
            ThemeId::Primary => ThemeId::Primary,
            ThemeId::Secondary => ThemeId::Secondary,
        };

        if new_theme != self.active {
            self.active = new_theme;
        }

        new_theme
    }

    pub fn toggle_theme(&mut self) -> ThemeId {
        let new_theme = match self.active {
            ThemeId::Primary => ThemeId::Secondary,
            ThemeId::Secondary => ThemeId::Primary,
        };

        self.active = new_theme;

        new_theme
    }

    /// Take the future signifying all theme texture uploads, and tag
    /// all themes as being uploaded. The future *must* be synced
    /// before any texture theme is used!
    #[must_use = "taking the Themes future assumes that the future will be joined before the theme is used"]
    pub fn take_future(&mut self) -> Option<Box<dyn GpuFuture>> {
        self.primary.is_uploaded.store(true);
        self.secondary.is_uploaded.store(true);

        self.custom
            .values_mut()
            .for_each(|t| t.is_uploaded.store(true));

        std::mem::take(&mut self.future)
    }

    pub fn themes_to_upload(&self) -> Vec<(ThemeId, &Theme)> {
        let mut res = Vec::new();

        if !self.primary.is_uploaded.load() {
            res.push((ThemeId::Primary, &self.primary));
        }

        if !self.secondary.is_uploaded.load() {
            res.push((ThemeId::Secondary, &self.secondary));
        }

        res
    }

    /// Returns the active theme if it's ready to use
    pub fn active_theme(&self) -> Option<(ThemeId, &Theme)> {
        let (id, theme) = match self.active {
            i @ ThemeId::Primary => (i, &self.primary),
            i @ ThemeId::Secondary => (i, &self.secondary),
            // ThemeId::Custom(id) => {
            //     self.custom.get(&id).map(|t| (ThemeId::Custom(id), t))?
            // }
        };

        theme.is_uploaded.load().then(|| (id, theme))
    }

    pub fn active_theme_ignore_cache(&self) -> (ThemeId, &Theme) {
        match self.active {
            i @ ThemeId::Primary => (i, &self.primary),
            i @ ThemeId::Secondary => (i, &self.secondary),
        }
    }
}
