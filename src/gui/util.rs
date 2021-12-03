use crossbeam::atomic::AtomicCell;
use std::cell::RefCell;
use std::rc::Rc;
// use parking_lot::RefCell

#[derive(Default)]
pub struct ColumnWidthsVec {
    widths_hdr: Rc<RefCell<Vec<f32>>>,
    widths: Rc<RefCell<Vec<f32>>>,
}

impl ColumnWidthsVec {
    pub fn get(&self) -> Vec<f32> {
        let prev_hdr = self.widths_hdr.borrow();
        let prev = self.widths.borrow();
        // let mut ws = [0.0f32; N];

        // let prev_hdr = self.widths_hdr.load();
        // let prev = self.widths.load();

        let prevs = prev_hdr.iter().zip(prev.iter()).map(|(h, r)| h.max(*r));

        prevs.collect()
    }

    pub fn set_hdr(&self, widths: &[f32]) {
        let mut ws = self.widths_hdr.borrow_mut();
        ws.clear();
        ws.extend_from_slice(widths);
    }

    pub fn set(&self, widths: &[f32]) {
        let mut ws = self.widths.borrow_mut();
        ws.clear();
        ws.extend_from_slice(widths);
    }
}

pub struct ColumnWidths<const N: usize> {
    widths_hdr: AtomicCell<[f32; N]>,
    widths: AtomicCell<[f32; N]>,
}

impl<const N: usize> ColumnWidths<N> {
    pub fn get(&self) -> [f32; N] {
        let mut ws = [0.0f32; N];

        let prev_hdr = self.widths_hdr.load();
        let prev = self.widths.load();

        let prevs = prev_hdr.iter().zip(prev).map(|(h, r)| h.max(r));

        for (w, prev) in ws.iter_mut().zip(prevs) {
            *w = prev
        }

        ws
    }

    pub fn set_hdr(&self, widths: &[f32]) {
        let mut ws = self.widths_hdr.load();

        for (ix, w) in ws.iter_mut().enumerate() {
            if let Some(new_w) = widths.get(ix).copied() {
                *w = new_w;
            }
        }
        self.widths_hdr.store(ws);
    }

    pub fn set(&self, widths: &[f32]) {
        let mut ws = self.widths.load();

        for (ix, w) in ws.iter_mut().enumerate() {
            if let Some(new_w) = widths.get(ix).copied() {
                *w = new_w;
            }
        }
        self.widths.store(ws);
    }
}

impl<const N: usize> std::default::Default for ColumnWidths<N> {
    fn default() -> Self {
        let arr = [0.0; N];
        Self {
            widths_hdr: arr.into(),
            widths: arr.into(),
        }
    }
}

fn add_label_width(
    ui: &mut egui::Ui,
    width: f32,
    text: &str,
) -> (f32, egui::Response) {
    let label = egui::Label::new(text);
    let galley = label.layout(ui);
    let size = galley.size();
    let real_width = size.x;

    let resp = ui
        .with_layout(egui::Layout::right_to_left(), |ui| {
            ui.set_min_width(width.max(real_width));
            ui.add(label)
        })
        .response;

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

    let mut row: Option<egui::Response> = None;

    let cols = fields.len();
    let mut prev_widths = prev_widths
        .map(|ws| Vec::from(ws))
        .unwrap_or(vec![0.0f32; cols]);

    if prev_widths.len() < fields.len() {
        for _ in 0..(cols - prev_widths.len()) {
            prev_widths.push(0.0);
        }
    }

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

pub fn add_scroll_buttons(ui: &mut egui::Ui) -> Option<egui::Align> {
    ui.horizontal(|ui| {
        let mut r = None;
        if ui.button("Top").clicked() {
            r = Some(egui::Align::TOP);
        }

        if ui.button("Bottom").clicked() {
            r = Some(egui::Align::BOTTOM);
        }

        r
    })
    .inner
}

pub fn scrolled_area(
    ui: &mut egui::Ui,
    num_rows: usize,
    scroll_align: Option<egui::Align>,
) -> egui::ScrollArea {
    let text_style = egui::TextStyle::Body;
    let row_height = ui.fonts()[text_style].row_height();
    let spacing = ui.style().spacing.item_spacing.y;

    let mut scroll_area = egui::ScrollArea::vertical();

    if let Some(align) = scroll_align {
        let h = row_height + spacing;
        let offset = match align {
            egui::Align::Min => 0.0,
            egui::Align::Max => h * (num_rows + 1) as f32,
            _ => 0.0,
        };
        scroll_area = scroll_area.scroll_offset(offset);
    }

    scroll_area
}
