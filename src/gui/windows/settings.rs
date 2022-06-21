use crate::{
    app::{AppSettings, SharedState},
    geometry::Point,
};

pub mod debug;
pub mod gui;
pub mod main_view;

use debug::*;
use gui::*;
use main_view::*;

pub struct SettingsWindow {
    current_tab: SettingsTab,

    pub(crate) debug: DebugSettings,
    pub(crate) gui: GuiSettings,
    pub(crate) main_view: MainViewSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
enum SettingsTab {
    MainView,
    Debug,
    Gui,
}

impl SettingsWindow {
    pub const ID: &'static str = "settings_window";

    pub fn new(settings: &AppSettings, shared_state: &SharedState) -> Self {
        let current_tab = SettingsTab::MainView;

        let main_view =
            MainViewSettings::new(settings, shared_state.edges_enabled.clone());

        Self {
            current_tab,

            debug: Default::default(),
            gui: Default::default(),
            main_view,
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        // ) -> Option<egui::Response> {
    ) -> Option<egui::InnerResponse<Option<()>>> {
        egui::Window::new("Settings")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .default_pos(Point::new(300.0, 300.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.current_tab,
                        SettingsTab::MainView,
                        "Main View",
                    );
                    ui.selectable_value(
                        &mut self.current_tab,
                        SettingsTab::Gui,
                        "GUI",
                    );
                    ui.selectable_value(
                        &mut self.current_tab,
                        SettingsTab::Debug,
                        "Debug",
                    );
                });

                match self.current_tab {
                    SettingsTab::MainView => {
                        self.main_view.ui(ui);
                    }
                    SettingsTab::Debug => {
                        self.debug.ui(ui);
                    }
                    SettingsTab::Gui => {
                        self.gui.ui(ui);
                    }
                }
            })
    }
}
