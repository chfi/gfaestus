use egui::widgets;
use egui::widgets::color_picker;

use rgb::*;

use crate::app::options::AppConfigState;
use crate::app::theme::{Theme, ThemeDef, ThemeId};

fn rgb_to_color32(color: RGB<f32>) -> egui::Color32 {
    let r = (255.0 * color.r).floor();
    let g = (255.0 * color.g).floor();
    let b = (255.0 * color.b).floor();
    egui::Color32::from_rgb(r as u8, g as u8, b as u8)
}

fn color32_to_rgb(color: egui::Color32) -> RGB<f32> {
    let r = (color.r() as f32) / 255.0;
    let g = (color.g() as f32) / 255.0;
    let b = (color.b() as f32) / 255.0;
    RGB::new(r, g, b)
}

pub struct ThemeEditor {
    // background: RGB<f32>,
    id: ThemeId,
    open: bool,
    background: egui::Color32,
    node_colors: Vec<egui::Color32>,
    // node_colors: Vec<RGB<f32>>,
}

impl ThemeEditor {
    pub fn new(background: RGB<f32>, node_colors: &[RGB<f32>]) -> Self {
        let node_colors = node_colors
            .iter()
            .map(|&c| rgb_to_color32(c))
            .collect::<Vec<_>>();

        Self {
            open: true,
            id: ThemeId::Primary,
            background: rgb_to_color32(background),
            node_colors,
        }
    }

    pub fn window(&mut self) -> egui::Window {
        egui::Window::new("Theme Editor").title_bar(true)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.colored_label(self.background, "select a color");
            ui.color_edit_button_srgba(&mut self.background);
        });
    }

    pub fn set_theme_id(&mut self, id: ThemeId) {
        self.id = id;
    }

    pub fn update_from_themedef(&mut self, theme: &ThemeDef) {
        self.background = rgb_to_color32(theme.background);
        self.node_colors.clear();
        self.node_colors
            .extend(theme.node_colors.iter().map(|&c| rgb_to_color32(c)));
    }

    pub fn theme_id(&self) -> ThemeId {
        self.id
    }

    pub fn state_to_themedef(&self) -> ThemeDef {
        ThemeDef {
            background: color32_to_rgb(self.background),
            node_colors: self
                .node_colors
                .iter()
                .map(|&c| color32_to_rgb(c))
                .collect(),
        }
    }
}

pub(super) fn theme_editor(
    ctx: &egui::CtxRef,
    background: &mut RGB<f32>,
    // node_colors: &mut Vec<RGB<f32>>,
) -> Option<egui::Response> {
    let mut bg32 = rgb_to_color32(*background);

    egui::Window::new("Theme Editor")
        // .id("theme_editor")
        .title_bar(true)
        .show(ctx, |ui| {
            ui.label("Background color");
            // color_picker(ui,
        })
}
