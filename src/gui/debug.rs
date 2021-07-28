use crate::view::View;
use crate::{geometry::Point, view::ScreenDims};

pub struct ViewDebugInfo;

impl ViewDebugInfo {
    pub fn ui(ctx: &egui::CtxRef, view: View) {
        let screen_rect = ctx.input().screen_rect();

        egui::Area::new("view_debug_info")
            .movable(true)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Center: ({}, {})",
                    view.center.x, view.center.y
                ));
                ui.label(format!("Scale: {}", view.scale));

                ui.separator();

                let dims = Point {
                    x: screen_rect.width(),
                    y: screen_rect.height(),
                };

                let visible_top_left = view.center - (dims / 2.0);
                let visible_bottom_right = view.center + (dims / 2.0);

                ui.label("Visible area");
                ui.label(format!(
                    "Top left: ({}, {})",
                    visible_top_left.x, visible_top_left.y,
                ));
                ui.label(format!(
                    "Bottom right: ({}, {})",
                    visible_bottom_right.x, visible_bottom_right.y,
                ));
            });
    }
}
