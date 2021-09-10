pub struct GuiSettings {
    pub(crate) show_fps: bool,
    pub(crate) show_graph_stats: bool,
}

impl std::default::Default for GuiSettings {
    fn default() -> Self {
        Self {
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
