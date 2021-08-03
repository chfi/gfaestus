#[derive(Debug, Clone, Copy)]
pub struct DebugSettings {
    pub(crate) view_info: bool,
    pub(crate) cursor_info: bool,
}

impl std::default::Default for DebugSettings {
    fn default() -> Self {
        Self {
            view_info: false,
            cursor_info: false,
        }
    }
}

impl DebugSettings {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // let view = ui.checkbox(&mut self.view_info, "Viewport Info");
        // let cursor = ui.checkbox(&mut self.cursor_info, "Cursor Info");
        ui.checkbox(&mut self.view_info, "Viewport Info");
        ui.checkbox(&mut self.cursor_info, "Cursor Info");
    }
}
