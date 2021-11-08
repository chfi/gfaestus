pub fn grid_row_label(
    ui: &mut egui::Ui,
    id: egui::Id,
    fields: &[&str],
    with_separator: bool,
) -> egui::Response {
    assert!(!fields.is_empty());

    let mut fields = fields.into_iter();
    let mut row = ui.label(*fields.next().unwrap());

    for field in fields {
        if with_separator {
            row = row.union(ui.separator())
        };
        row = row.union(ui.label(*field));
    }

    let row = ui.interact(
        row.rect,
        id,
        egui::Sense::click().union(egui::Sense::hover()),
    );

    let visuals = ui.style().interact_selectable(&row, false);

    ui.end_row();

    if row.hovered() {
        // let mut rect = row.rect;
        // rect.max.x = ui.max_rect().right();

        let rect = row.rect.expand(visuals.expansion);

        ui.painter().rect_stroke(rect, 0.0, visuals.bg_stroke);
    }

    row
}
