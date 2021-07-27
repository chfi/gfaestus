use crate::{geometry::Point, view::View};

pub fn draw_text_at_world_point(
    ctx: &egui::CtxRef,
    view: View,
    world: Point,
    text: &str,
) {
    let screen_rect = ctx.input().screen_rect();

    let paint_area = egui::Ui::new(
        ctx.clone(),
        egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("gui_text_background"),
        ),
        egui::Id::new("gui_text_ui"),
        screen_rect,
        screen_rect,
    );

    let screen_pos = view.world_point_to_screen(world);

    if screen_pos.x > 0.0
        && screen_pos.x < screen_rect.width()
        && screen_pos.y > 0.0
        && screen_pos.y < screen_rect.height()
    {
        paint_area.painter().text(
            screen_pos.into(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::TextStyle::Body,
            egui::Color32::WHITE,
        );
    }
}

/*
fn hover_annotation(&self) {
    if let Some(node_id) = self.hover_node_id {
        if self.ctx.is_pointer_over_area() {
            return;
        }

        let annots = self.annotations.annotations_for(node_id);

        if annots.is_empty() {
            egui::containers::popup::show_tooltip_text(
                &self.ctx,
                egui::Id::new("hover_node_id_tooltip"),
                node_id.0.to_string(),
            )
        } else {
            let mut string = String::new();

            for (name, val) in annots {
                string.push_str(name);
                string.push_str(": ");
                string.push_str(val);
                string.push_str("\n");
            }

            egui::containers::popup::show_tooltip_text(
                &self.ctx,
                egui::Id::new("hover_node_id_tooltip"),
                string,
            )
        }
    }
}
*/

// pub struct TextRenderer {

// }
