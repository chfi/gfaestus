use crate::{
    annotations::{AnnotationRecord, ColumnKey},
    gui::windows::filters::FilterString,
};

use super::ColumnPickerMany;

#[derive(Debug, Clone, PartialEq)]
pub struct QuickFilter<T: ColumnKey> {
    filter: FilterString,
    columns: ColumnPickerMany<T>,
    column_picker_open: bool,
}

impl<T: ColumnKey> QuickFilter<T> {
    pub fn new(id_source: &str) -> Self {
        Self {
            filter: Default::default(),
            columns: ColumnPickerMany::new(id_source),
            column_picker_open: false,
        }
    }

    pub fn column_picker_mut(&mut self) -> &mut ColumnPickerMany<T> {
        &mut self.columns
    }

    pub fn filter_record<R>(&self, record: &R) -> bool
    where
        R: AnnotationRecord<ColumnKey = T>,
    {
        self.columns
            .enabled_columns
            .iter()
            .filter_map(|(c, enabled)| if *enabled { Some(c) } else { None })
            .any(|column| {
                let values = record.get_all(column);
                values.iter().any(|v| self.filter.filter_bytes(v))
            })
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        ui.horizontal(|ui| {
            ui.heading("Quick filter");
            if ui
                .selectable_label(self.column_picker_open, "Choose columns")
                .clicked()
            {
                self.column_picker_open = !self.column_picker_open;
            }
        });

        let filter_resp = self.filter.ui(ui);

        let open = &mut self.column_picker_open;
        let column_picker = &mut self.columns;

        let ctx = ui.ctx();
        column_picker.ui(ctx, None, open, "Quick filter columns");

        if let Some(resp) = filter_resp {
            resp.has_focus() && ctx.input().key_pressed(egui::Key::Enter)
        } else {
            false
        }
    }
}
