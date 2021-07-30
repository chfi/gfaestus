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

                let visible_top_left =
                    view.screen_point_to_world(dims, Point::ZERO);
                let visible_bottom_right =
                    view.screen_point_to_world(dims, dims);

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

pub struct MouseDebugInfo;

impl MouseDebugInfo {
    pub fn ui(ctx: &egui::CtxRef, view: View, mouse_screen: Point) {
        let screen_rect = ctx.input().screen_rect();

        let dims = ScreenDims {
            width: screen_rect.width(),
            height: screen_rect.height(),
        };

        let screen = mouse_screen;
        let world = view.screen_point_to_world(dims, screen);

        egui::Area::new("mouse_debug_info")
            .movable(true)
            .show(ctx, |ui| {
                ui.label("Cursor position");

                ui.separator();

                ui.label(format!("Screen: ({}, {})", screen.x, screen.y));
                ui.label(format!("World: ({}, {})", world.x, world.y));
            });
    }
}
