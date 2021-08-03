use crate::app::{AppSettings, SharedState};

pub mod debug;
pub mod main_view;

use debug::*;
use main_view::*;

pub struct SettingsWindow {
    current_tab: SettingsTab,

    pub(crate) main_view: MainViewSettings,
    pub(crate) debug: DebugSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
enum SettingsTab {
    MainView,
    Debug,
}

impl SettingsWindow {
    pub const ID: &'static str = "settings_window";

    pub fn new(settings: &AppSettings, shared_state: &SharedState) -> Self {
        let current_tab = SettingsTab::MainView;

        let main_view =
            MainViewSettings::new(settings, shared_state.clone_edges_enabled());

        Self {
            current_tab,
            main_view,
            debug: Default::default(),
        }
    }

    pub fn ui(&mut self, ctx: &egui::CtxRef) -> Option<egui::Response> {
        egui::Window::new("Settings")
            .id(egui::Id::new(Self::ID))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.current_tab,
                        SettingsTab::MainView,
                        "Main View",
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
                }
            })
    }
}
