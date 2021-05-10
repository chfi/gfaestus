use rgb::*;

use anyhow::Result;

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crossbeam::atomic::AtomicCell;

use crate::vulkan::{
    context::VkContext,
    draw_system::{
        gui::{GuiPipeline, GuiVertex, GuiVertices},
        nodes::NodeThemePipeline,
    },
    texture::*,
    GfaestusVk, SwapchainProperties,
};

#[derive(Debug, Clone)]
pub enum AppThemeMsg {
    TogglePreviousTheme,
    UseTheme {
        theme_id: usize,
    },
    SetThemeDef {
        theme_id: usize,
        theme_def: ThemeDef,
    },
}

pub struct AppThemes {
    next_theme_id: usize,

    theme_definitions: FxHashMap<usize, ThemeDef>,

    pub active_theme: usize,
    pub previous_theme: usize,

    uploaded_to_gpu: bool,
}

impl AppThemes {
    pub fn default_themes() -> Self {
        let light = light_default();
        let light_ix = 0;

        let dark = dark_default();
        let dark_ix = 1;

        let next_theme_id = 2;

        let theme_definitions: FxHashMap<usize, ThemeDef> =
            std::array::IntoIter::new([(light_ix, light), (dark_ix, dark)]).collect();

        let active_theme = light_ix;
        let previous_theme = dark_ix;

        Self {
            next_theme_id,

            theme_definitions,

            active_theme,
            previous_theme,

            uploaded_to_gpu: false,
        }
    }

    pub fn upload_to_gpu(
        &mut self,
        app: &GfaestusVk,
        theme_pipeline: &mut NodeThemePipeline,
    ) -> Result<()> {
        if self.uploaded_to_gpu {
            return Ok(());
        }

        for (&theme_id, theme_def) in self.theme_definitions.iter() {
            theme_pipeline.upload_theme_data(app, theme_id, theme_def)?;
        }

        self.uploaded_to_gpu = true;

        Ok(())
    }

    pub fn active_theme_luma(&self) -> f32 {
        let theme = self
            .theme_definitions
            .get(&self.active_theme)
            .expect("Active theme lacks theme definition");
        let bg = theme.background;

        (0.2126 * bg.r) + (0.7152 * bg.g) + (0.0722 * bg.b)
    }

    pub fn is_active_theme_dark(&self) -> bool {
        self.active_theme_luma() < 0.5
    }

    pub fn active_theme(&self) -> usize {
        self.active_theme
    }

    pub fn toggle_previous_theme(&mut self) {
        std::mem::swap(&mut self.active_theme, &mut self.previous_theme);
    }

    pub fn new_theme(
        &mut self,
        app: &GfaestusVk,
        theme_pipeline: &mut NodeThemePipeline,
        theme_def: &ThemeDef,
    ) -> Result<usize> {
        let theme_id = self.next_theme_id;

        theme_pipeline.upload_theme_data(app, theme_id, theme_def)?;

        self.next_theme_id += 1;

        Ok(theme_id)
    }

    pub fn remove_theme(
        &mut self,
        theme_pipeline: &mut NodeThemePipeline,
        theme_id: usize,
    ) -> Option<usize> {
        if theme_id == self.active_theme
            || theme_id == self.previous_theme
            || !theme_pipeline.has_theme(theme_id)
        {
            return None;
        }

        theme_pipeline.destroy_theme(theme_id);

        Some(theme_id)
    }

    /// Returns `Ok(None)` if there's no theme with the provided ID,
    /// `Ok(Some(theme_id))` if the theme was successfully replaced,
    /// and `Err(_)` if there was an error in uploading it to the GPU
    pub fn replace_theme(
        &mut self,
        app: &GfaestusVk,
        theme_pipeline: &mut NodeThemePipeline,
        theme_id: usize,
        theme_def: &ThemeDef,
    ) -> Result<Option<usize>> {
        if !theme_pipeline.has_theme(theme_id) {
            return Ok(None);
        }

        theme_pipeline.upload_theme_data(app, theme_id, theme_def)?;

        if theme_id == self.active_theme {
            // make sure to update the theme texture used by the GPU
            // if the active theme was replaced
            theme_pipeline.set_active_theme(self.active_theme).unwrap();
        }

        Ok(Some(theme_id))
    }
}

/// A theme definition that can be transformed into theme data usable by the GPU
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeDef {
    pub background: RGB<f32>,
    pub node_colors: Vec<RGB<f32>>,
}

impl std::default::Default for ThemeDef {
    fn default() -> Self {
        light_default()
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
    let node_colors = RAINBOW.iter().copied().map(RGB::from).collect::<Vec<_>>();

    ThemeDef {
        background,
        node_colors,
    }
}

pub fn dark_default() -> ThemeDef {
    let background = RGB::new(0.0, 0.0, 0.05);

    let node_colors = RAINBOW.iter().copied().map(RGB::from).collect::<Vec<_>>();

    ThemeDef {
        background,
        node_colors,
    }
}
