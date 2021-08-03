use handlegraph::handle::NodeId;

use crate::{geometry::Point, universe::Node, view::View};

pub fn offset_align(dir: &Point) -> egui::Align2 {
    let norm = *dir / dir.length();

    let align_for = |v: f32| {
        if v > 0.67 {
            egui::Align::Max
        } else if v < -0.67 {
            egui::Align::Min
        } else {
            egui::Align::Center
        }
    };

    let hor_align = align_for(norm.x);
    let ver_align = align_for(norm.y);

    egui::Align2([hor_align, ver_align])
}

pub fn draw_text_at_node_anchor(
    ctx: &egui::CtxRef,
    node_positions: &[Node],
    view: View,
    node: NodeId,
    screen_offset: Point,
    anchor_dir: Point,
    text: &str,
) {
    let node_ix = (node.0 - 1) as usize;

    if let Some(node) = node_positions.get(node_ix) {
        let pos = node.center();

        draw_text_at_aligned_world_point_offset(
            ctx,
            view,
            pos,
            screen_offset,
            anchor_dir,
            text,
        );
    }
}

pub fn draw_text_at_world_point(
    ctx: &egui::CtxRef,
    view: View,
    world: Point,
    text: &str,
) {
    draw_text_at_world_point_offset(ctx, view, world, Point::ZERO, text);
}

pub fn draw_text_at_node(
    ctx: &egui::CtxRef,
    node_positions: &[Node],
    view: View,
    node: NodeId,
    screen_offset: Point,
    text: &str,
) {
    let node_ix = (node.0 - 1) as usize;

    if let Some(node) = node_positions.get(node_ix) {
        let pos = node.center();

        draw_text_at_world_point_offset(ctx, view, pos, screen_offset, text);
    }
}

pub fn draw_text_at_world_point_offset(
    ctx: &egui::CtxRef,
    view: View,
    world: Point,
    screen_offset: Point,
    text: &str,
) {
    draw_text_at_aligned_world_point_offset(
        ctx,
        view,
        world,
        screen_offset,
        Point::ZERO,
        text,
    )
}

pub fn draw_text_at_aligned_world_point_offset(
    ctx: &egui::CtxRef,
    view: View,
    world: Point,
    screen_offset: Point,
    anchor_dir: Point,
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

    let mut screen_pos = screen_pos + dims / 2.0;
    screen_pos += screen_offset;

    // hacky way to ensure that the text is only being rendered when
    // (more or less) on the screen, without being cut off if the
    // center of the text is just outside the visible area
    if screen_pos.x > -screen_rect.width()
        && screen_pos.x < 2.0 * screen_rect.width()
        && screen_pos.y > -screen_rect.height()
        && screen_pos.y < 2.0 * screen_rect.height()
    {
        let align = offset_align(&anchor_dir);

        let _rect = paint_area.painter().text(
            screen_pos.into(),
            align,
            // egui::Align2::CENTER_CENTER,
            text,
            egui::TextStyle::Body,
            egui::Color32::BLACK,
        );
    }
}
