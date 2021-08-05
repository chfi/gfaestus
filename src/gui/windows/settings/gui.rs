use crossbeam::atomic::AtomicCell;
use std::sync::Arc;

use crate::app::{AppSettings, NodeWidth};

pub struct GuiSettings {
    // show_fps: Arc<AtomicCell<bool>>,
    // show_graph_stats: Arc<AtomicCell<bool>>,
    pub(crate) show_fps: bool,
    pub(crate) show_graph_stats: bool,
}

impl std::default::Default for GuiSettings {
    fn default() -> Self {
        Self {
            // show_fps: Arc::new(false.into()),
            // show_graph_stats: Arc::new(true.into()),
            show_fps: false,
            show_graph_stats: false,
        }
    }
}

impl GuiSettings {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.show_fps, "Display FPS");
        ui.checkbox(&mut self.show_graph_stats, "Display graph stats");
    }
}
