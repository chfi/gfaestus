#[derive(Debug, Clone, Copy)]
pub struct DebugSettings {
    pub(crate) view_info: bool,
    pub(crate) cursor_info: bool,

    pub(crate) egui_inspection: bool,
    pub(crate) egui_settings: bool,
    pub(crate) egui_memory: bool,
}

impl std::default::Default for DebugSettings {
    fn default() -> Self {
        Self {
            view_info: false,
            cursor_info: false,

            egui_inspection: false,
            egui_settings: false,
            egui_memory: false,
        }
    }
}

impl DebugSettings {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.view_info, "Viewport Info");
        ui.checkbox(&mut self.cursor_info, "Cursor Info");

        ui.separator();
        ui.label("Egui Debug Windows");

        ui.checkbox(&mut self.egui_inspection, "Inspection");
        ui.checkbox(&mut self.egui_settings, "Settings");
        ui.checkbox(&mut self.egui_memory, "Memory");
    }
}
