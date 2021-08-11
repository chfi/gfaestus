use std::collections::HashMap;

use bstr::ByteSlice;

use crate::{
    annotations::{AnnotationCollection, AnnotationRecord, ColumnKey},
    gui::windows::filters::{
        FilterNum, FilterNumOp, FilterString, FilterStringOp,
    },
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

    pub fn ui_compact(&mut self, ui: &mut egui::Ui) -> bool {
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

pub struct RecordFilter<T: ColumnKey> {
    seq_id: FilterString,
    start: FilterNum<usize>,
    end: FilterNum<usize>,

    columns: HashMap<T, FilterString>,

    quick_filter: QuickFilter<T>,
}

impl<T: ColumnKey> RecordFilter<T> {
    pub fn new<C>(id_source: &str, records: &C) -> Self
    where
        C: AnnotationCollection<ColumnKey = T>,
    {
        let mut quick_filter_id_src = id_source.to_string();
        quick_filter_id_src.push_str("_quick_filter");

        let mut columns: HashMap<T, FilterString> = HashMap::new();

        let to_remove = [T::seq_id(), T::start(), T::end()];

        let mut to_add = records.all_columns();
        to_add.retain(|c| !to_remove.contains(c));

        for column in to_add {
            columns.insert(column.to_owned(), FilterString::default());
        }

        Self {
            seq_id: FilterString::default(),
            start: FilterNum::default(),
            end: FilterNum::default(),

            columns,

            quick_filter: QuickFilter::new(&quick_filter_id_src),
        }
    }

    pub fn range_filter(&mut self, mut start: usize, mut end: usize) {
        if start > 0 {
            start -= 1;
        }

        end += 1;

        self.start.op = FilterNumOp::MoreThan;
        self.start.arg1 = start;

        self.end.op = FilterNumOp::LessThan;
        self.end.arg1 = end;
    }

    pub fn chr_range_filter(
        &mut self,
        seq_id: &[u8],
        start: usize,
        end: usize,
    ) {
        if let Ok(seq_id) = seq_id.to_str().map(String::from) {
            self.seq_id.op = FilterStringOp::ContainedIn;
            self.seq_id.arg = seq_id;
        }
        self.range_filter(start, end);
    }

    pub fn filter_record<R>(&self, record: &R) -> bool
    where
        R: AnnotationRecord<ColumnKey = T>,
    {
        self.quick_filter.filter_record(record)
            && self.columns.iter().any(|(column, filter)| {
                let values = record.get_all(column);
                values.into_iter().any(|value| filter.filter_bytes(value))
            })
    }
}
