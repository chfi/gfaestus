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

    let dims = Point::new(screen_rect.width(), screen_rect.height());

    let screen_pos = screen_pos + dims / 2.0;

    // hacky way to ensure that the text is only being rendered when
    // (more or less) on the screen, without being cut off if the
    // center of the text is just outside the visible area
    if screen_pos.x > -screen_rect.width()
        && screen_pos.x < 2.0 * screen_rect.width()
        && screen_pos.y > -screen_rect.height()
        && screen_pos.y < 2.0 * screen_rect.height()
    {
        let rect = paint_area.painter().text(
            screen_pos.into(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::TextStyle::Body,
            egui::Color32::BLACK,
        );

        let stroke =
            egui::Stroke::new(2.0, egui::Color32::from_rgb(128, 128, 128));
        paint_area.painter().rect_stroke(rect, 0.0, stroke);
    }
}
