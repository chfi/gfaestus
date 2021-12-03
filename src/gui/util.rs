fn add_label_width(
    ui: &mut egui::Ui,
    width: f32,
    text: &str,
) -> (f32, egui::Response) {
    let label = egui::Label::new(text);
    let galley = label.layout(ui);
    let size = galley.size();
    let real_width = size.x;
    let resp = ui.add_sized([width.max(real_width), size.y], label);
    (real_width, resp)
}

pub fn grid_row_label(
    ui: &mut egui::Ui,
    id: egui::Id,
    fields: &[&str],
    with_separator: bool,
    prev_widths: Option<&[f32]>,
) -> egui::InnerResponse<Vec<f32>> {
    assert!(!fields.is_empty());

    // let mut fields = fields_strs.into_iter();
    // let mut row = ui.label(*fields.next().unwrap());

    let mut row: Option<egui::Response> = None;

    let cols = fields.len();
    let prev_widths = prev_widths
        .map(|ws| Vec::from(ws))
        .unwrap_or(vec![0.0f32; cols]);

    let mut widths = vec![0.0f32; cols];

    for (ix, (field, width)) in fields.into_iter().zip(prev_widths).enumerate()
    {
        if with_separator {
            if let Some(r) = row.as_mut() {
                *r = r.union(ui.separator());
            }
        };

        let (w, resp) = add_label_width(ui, width, field);

        widths[ix] = w;

        if let Some(r) = row.as_mut() {
            *r = r.union(resp);
        } else {
            row = Some(resp);
        }
    }

    let row = ui.interact(
        row.unwrap().rect,
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

    egui::InnerResponse {
        inner: widths,
        response: row,
    }
}
