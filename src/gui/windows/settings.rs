use std::sync::Arc;

use crate::app::AppSettings;

use super::MainViewSettings;

pub mod debug;
use crossbeam::atomic::AtomicCell;
use debug::*;

pub struct SettingsWindow {
    current_tab: SettingsTab,

    main_view: MainViewSettings,

    debug: DebugSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
enum SettingsTab {
    MainView,
    Debug,
}

impl SettingsWindow {
    pub const ID: &'static str = "settings_window";

    pub fn new(
        settings: &AppSettings,
        edges_enabled: Arc<AtomicCell<bool>>,
    ) -> Self {
        // let current_tab = SettingsTab::MainView;
        let current_tab = SettingsTab::Debug;
        let main_view = MainViewSettings::new(settings, edges_enabled);

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
                        self.main_view.inner_ui(ui);
                    }
                    SettingsTab::Debug => {
                        self.debug.ui(ui);
                    }
                }
            })
    }
}
