use crate::gui::console::Console;

pub struct PathPositionList {}

impl PathPositionList {
    pub const ID: &'static str = "path_position_list";

    pub fn ui(ctx: &egui::CtxRef, console: &Console, open: &mut bool) {
        egui::Window::new("Path View")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |ui| {
                //
            });
    }
}
