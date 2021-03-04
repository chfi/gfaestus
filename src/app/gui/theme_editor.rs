use egui::widgets;
use egui::widgets::color_picker;

use egui::vec2;

use rgb::*;

use crossbeam::channel;

use crate::app::settings::AppConfigState;
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

/// The window that contains the theme editor widget, and lets the
/// user choose which theme to edit
pub struct ThemeEditorWindow {
    open: bool,
    tx_theme: channel::Sender<AppConfigState>,

    editing_theme: ThemeId,

    primary: ThemeEditor,
    secondary: ThemeEditor,
}

impl ThemeEditorWindow {
    pub fn new(tx_theme: channel::Sender<AppConfigState>) -> Self {
        let primary_def = ThemeDef::default();
        let secondary_def = ThemeDef::default();

        let editing_theme = ThemeId::Primary;

        let primary = ThemeEditor::from_theme_def(&primary_def);
        let mut secondary = ThemeEditor::from_theme_def(&secondary_def);
        secondary.id = ThemeId::Secondary;

        Self {
            open: true,
            tx_theme,

            editing_theme,

            primary,
            secondary,
        }
    }

    pub fn update_theme(&mut self, id: ThemeId, theme: &ThemeDef) {
        let editor = match id {
            ThemeId::Primary => &mut self.primary,

            ThemeId::Secondary => &mut self.secondary,
        };

        editor.set_theme_id(id);
        editor.update_from_themedef(theme);
    }

    // pub fn apply_theme(&self) -> AppConfigState {
    // }

    // pub fn show(&mut self, ctx: &egui::CtxRef, open: &mut bool) {
    pub fn show(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new("Theme Editor")
            // .open(open)
            .default_size(vec2(512.0, 512.0))
            .scroll(true)
            .show(ctx, |ui| {
                let mut primary_btn = widgets::Button::new("Primary");
                let mut secondary_btn = widgets::Button::new("Secondary");

                if self.editing_theme == ThemeId::Primary {
                    primary_btn = primary_btn.enabled(false);
                    secondary_btn = secondary_btn.enabled(true);
                } else {
                    primary_btn = primary_btn.enabled(true);
                    secondary_btn = secondary_btn.enabled(false);
                }

                if ui.add(primary_btn).clicked() {
                    self.editing_theme = ThemeId::Primary;
                }
                if ui.add(secondary_btn).clicked() {
                    self.editing_theme = ThemeId::Secondary;
                }

                match self.editing_theme {
                    ThemeId::Primary => {
                        self.primary.ui(ui);
                    }
                    ThemeId::Secondary => {
                        self.secondary.ui(ui);
                    }
                }

                if ui.button("Apply").clicked() {
                    let (id, def) = match self.editing_theme {
                        ThemeId::Primary => {
                            let id = ThemeId::Primary;
                            let def = self.primary.state_to_themedef();
                            (id, def)
                        }
                        ThemeId::Secondary => {
                            let id = ThemeId::Secondary;
                            let def = self.secondary.state_to_themedef();
                            (id, def)
                        }
                    };

                    self.tx_theme
                        .send(AppConfigState::Theme { id, def })
                        .unwrap();
                    println!("Sent new theme");
                }
            });
    }
}

/// The widget for editing a specific theme
pub struct ThemeEditor {
    // background: RGB<f32>,
    id: ThemeId,
    // open: bool,
    background: egui::Color32,
    node_colors: Vec<egui::Color32>,
    // node_colors: Vec<RGB<f32>>,
    // tx_theme: channel::Sender<AppConfigState>,
    // rx_theme: channel::Receiver<AppConfigState>,
}

impl ThemeEditor {
    pub fn from_theme_def(def: &ThemeDef) -> Self {
        ThemeEditor::new(def.background, &def.node_colors)
    }

    pub fn new(
        // tx_theme: channel::Sender<AppConfigState>,
        // rx_theme: channel::Receiver<AppConfigState>,
        background: RGB<f32>,
        node_colors: &[RGB<f32>],
    ) -> Self {
        let node_colors = node_colors
            .iter()
            .map(|&c| rgb_to_color32(c))
            .collect::<Vec<_>>();

        Self {
            // open: true,
            id: ThemeId::Primary,
            background: rgb_to_color32(background),
            node_colors,
            // tx_theme,
            // rx_theme,
        }
    }

    pub fn window(&self) -> egui::Window {
        egui::Window::new("Theme Editor").title_bar(true)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(format!("Theme: {}", self.id));

            ui.colored_label(self.background, "select a color");
            ui.color_edit_button_srgba(&mut self.background);

            // if ui.button("Apply").clicked() {
            //     let def = self.state_to_themedef();
            //     let id = self.id;

            //     self.tx_theme
            //         .send(AppConfigState::Theme { id, def })
            //         .unwrap();
            //     println!("Sent new theme");
            // }
        });
    }

    pub fn show(&mut self, ctx: &egui::CtxRef) {
        let window = egui::Window::new("Theme Editor").title_bar(true);
        window.show(ctx, |ui| self.ui(ui));
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
