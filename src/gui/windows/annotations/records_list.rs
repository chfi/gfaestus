use std::collections::HashMap;
use std::sync::Arc;

use bstr::ByteSlice;
use crossbeam::channel::Sender;
use handlegraph::pathhandlegraph::PathId;
use rustc_hash::FxHashSet;

use crate::{
    annotations::{AnnotationCollection, AnnotationRecord, ColumnKey},
    app::AppMsg,
    graph_query::{GraphQuery, GraphQueryWorker},
    gui::{
        util::grid_row_label,
        windows::{graph_picker::PathPicker, overlays::OverlayCreatorMsg},
    },
};

use super::{filter::RecordFilter, ColumnPickerMany, OverlayLabelSetCreator};

pub struct RecordList<T: ColumnKey> {
    id: egui::Id,
    current_file: Option<String>,

    filtered_records: Vec<usize>,

    offset: usize,
    slot_count: usize,

    filter_open: bool,
    filters: HashMap<String, RecordFilter<T>>,

    column_picker_open: bool,
    enabled_columns: HashMap<String, ColumnPickerMany<T>>,

    path_picker_open: bool,
    path_picker: PathPicker,

    creator_open: bool,
    creator: OverlayLabelSetCreator,
    overlay_tx: Sender<OverlayCreatorMsg>,
}

impl<T: ColumnKey> RecordList<T> {
    pub fn new(
        id: egui::Id,
        path_picker: PathPicker,
        new_overlay_tx: Sender<OverlayCreatorMsg>,
    ) -> Self {
        let filtered_records = Vec::new();

        Self {
            id,
            current_file: None,

            filtered_records,

            offset: 0,
            slot_count: 20,

            filter_open: false,
            filters: HashMap::default(),

            column_picker_open: false,
            enabled_columns: HashMap::default(),

            path_picker_open: false,
            path_picker,

            creator_open: false,
            creator: OverlayLabelSetCreator::new("overlay_label_set_creator"),
            overlay_tx: new_overlay_tx,
        }
    }

    pub fn scroll_to_label_record<C>(
        &mut self,
        records: &C,
        column: &T,
        value: &[u8],
    ) where
        C: AnnotationCollection<ColumnKey = T>,
    {
        let ix = self
            .filtered_records
            .iter()
            .enumerate()
            .find(|&(_ix, record_ix)| {
                let record = &records.records()[*record_ix];
                let column_values = record.get_all(column);
                column_values.iter().any(|&rec_val| rec_val == value)
            })
            .map(|(ix, _)| ix);

        if let Some(ix) = ix {
            self.offset = ix;
        }
    }

    fn ui_row<C, R>(
        &self,
        ui: &mut egui::Ui,
        file_name: &str,
        records: &C,
        record: &R,
        index: usize,
    ) -> egui::Response
    where
        C: AnnotationCollection<ColumnKey = T, Record = R>,
        R: AnnotationRecord<ColumnKey = T>,
    {
        let mut fields: Vec<String> = vec![
            format!("{}", record.seq_id().as_bstr()),
            format!("{}", record.start()),
            format!("{}", record.end()),
        ];
        let enabled_columns = self.enabled_columns.get(file_name).unwrap();

        let mut enabled = enabled_columns.enabled_columns.iter().filter_map(
            |(col, enabled)| {
                if *enabled {
                    Some(col)
                } else {
                    None
                }
            },
        );

        for column in enabled {
            let values = record.get_all(column);

            let mut label = String::new();

            for (count, value) in values.into_iter().enumerate() {
                if count != 0 {
                    label.push_str(";");
                }
                let val_str = value.to_str().unwrap();
                label.push_str(val_str);
            }

            fields.push(label);
        }

        let fields_ref: Vec<&str> =
            fields.iter().map(|f| f.as_str()).collect::<Vec<_>>();

        let resp = grid_row_label(
            ui,
            egui::Id::new(ui.id().with(index)),
            &fields_ref,
            false,
        );
        ui.end_row();

        resp
    }

    fn select_record<R>(
        &self,
        app_msg_tx: &crossbeam::channel::Sender<AppMsg>,
        graph_query: &GraphQuery,
        record: &R,
    ) where
        R: AnnotationRecord<ColumnKey = T>,
    {
        let active_path = self.path_picker.active_path();

        if let Some((path_id, name)) = active_path {
            let mut start = record.start();
            let mut end = record.end();

            if let Some(offset) =
                crate::annotations::path_name_offset(name.as_bytes())
            {
                start -= offset;
                end -= offset;
            }

            if let Some(range) =
                graph_query.path_basepair_range(path_id, start, end)
            {
                let nodes = range
                    .into_iter()
                    .map(|(handle, _, _)| handle.id())
                    .collect::<FxHashSet<_>>();

                use crate::app::Select;

                let select = Select::Many { nodes, clear: true };
                let msg = AppMsg::Selection(select);
                app_msg_tx.send(msg).unwrap();
            }
        }
    }

    fn apply_filter<C>(&mut self, file_name: &str, records: &C)
    where
        C: AnnotationCollection<ColumnKey = T>,
    {
        self.filtered_records.clear();

        eprintln!("applying filter");
        let total = records.records().len();

        let records = &records.records();
        let filter = self.filters.get(file_name).unwrap();
        let filtered_records = &mut self.filtered_records;

        filtered_records.extend(records.iter().enumerate().filter_map(
            |(ix, rec)| {
                if filter.filter_record(rec) {
                    Some(ix)
                } else {
                    None
                }
            },
        ));
        let filtered = self.filtered_records.len();
        eprintln!(
            "filter complete, showing {} out of {} records",
            filtered, total
        );

        self.offset = 0;
    }

    fn clear_filter(&mut self) {
        self.filtered_records.clear();
    }

    pub fn active_path_id(&self) -> Option<PathId> {
        let (path, _) = self.path_picker.active_path()?;
        Some(path)
    }
}
