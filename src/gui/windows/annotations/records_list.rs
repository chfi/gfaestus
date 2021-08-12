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
    creator: OverlayLabelSetCreator<T>,
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
            creator: OverlayLabelSetCreator::new(egui::Id::new(
                "overlay_label_set_creator",
            )),
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

impl<T: ColumnKey + 'static> RecordList<T> {
    pub fn ui<C>(
        &mut self,
        ui: &mut egui::Ui,
        graph_query: &GraphQueryWorker,
        app_msg_tx: &crossbeam::channel::Sender<AppMsg>,
        file_name: &str,
        records: &Arc<C>,
    ) where
        C: AnnotationCollection<ColumnKey = T> + Send + Sync + 'static,
    {
        let active_path_name = self
            .path_picker
            .active_path()
            .map(|(_id, name)| name.to_owned());

        if !self.enabled_columns.contains_key(file_name) {
            let mut enabled_columns: ColumnPickerMany<T> =
                ColumnPickerMany::new(egui::Id::new(file_name));

            enabled_columns.update_columns(records.as_ref());

            for col in [T::seq_id(), T::start(), T::end()] {
                // enabled_columns.set_column(&col, true);
                enabled_columns.hide_column_from_gui(&col, true);
            }

            self.enabled_columns
                .insert(file_name.to_string(), enabled_columns);

            /*
            use Gff3Column as Gff;
            for col in [Gff::Source, Gff::Type, Gff::Frame] {
                enabled_columns.set_column(&col, true);
            }

            for col in [Gff::SeqId, Gff::Start, Gff::End, Gff::Strand] {
                enabled_columns.hide_column_from_gui(&col, true);
            }

            self.gff3_enabled_columns
                .insert(file_name.to_string(), enabled_columns);
            */
        }

        {
            let filter = self
                .filters
                .entry(file_name.to_string())
                .or_insert(RecordFilter::new(self.id, records.as_ref()));

            let ctx = ui.ctx();

            egui::Window::new("Filter records")
                .id(self.id)
                .default_pos(egui::Pos2::new(600.0, 200.0))
                .open(&mut self.filter_open)
                .show(ctx, |ui| {
                    ui.set_max_width(400.0);
                    filter.ui(ui);
                });
        }

        if self.current_file.as_ref().map(|s| s.as_str()) != Some(file_name) {
            self.current_file = Some(file_name.to_string());
            self.apply_filter(file_name, records.as_ref());
        }

        self.path_picker.ui(ui.ctx(), &mut self.path_picker_open);

        if let Some(path) = self.path_picker.active_path().map(|(p, _)| p) {
            if self
                .creator
                .current_annotation_file
                .as_ref()
                .map(|s| s.as_str())
                != Some(file_name)
            {
                self.creator.current_annotation_file =
                    Some(file_name.to_string());
                self.creator.column_picker.update_columns(records.as_ref());
            }

            self.creator.ui(
                ui.ctx(),
                &self.overlay_tx,
                app_msg_tx,
                graph_query,
                &mut self.creator_open,
                file_name,
                path,
                records.clone(),
                &self.filtered_records,
            );
        }

        // TODO configurable window title
        /*
        let resp = egui::Window::new("GFF3")
            .id(Self::list_id())
            .default_pos(egui::Pos2::new(600.0, 200.0))
            .collapsible(true)
            .open(open)
            // .resizable(true)
            .show(ctx, |mut ui| {
                */
        ui.set_min_height(200.0);
        ui.set_max_height(ui.input().screen_rect.height() - 100.0);

        ui.label(file_name);
        ui.separator();

        let apply_filter = {
            let filters = self.filters.get_mut(file_name).unwrap();
            let qf_cols = filters.quick_filter.column_picker_mut();

            let popup_id = ui
                .make_persistent_id(self.id.with("quick_filter_columns_popup"));

            let button_inner = ui.horizontal(|ui| {
                ui.heading("Quick filter");
                let btn = ui.button("Choose columns");

                if btn.clicked() {
                    println!("popup clicked");
                    ui.memory().toggle_popup(popup_id);
                }

                btn
            });

            let button = &button_inner.response;

            crate::gui::windows::util::popup_below_widget(
                ui,
                popup_id,
                &button,
                |ui| qf_cols.compact_widget(ui),
            );

            filters.add_quick_filter(ui)
        };

        ui.separator();

        ui.horizontal(|ui| {
            let filter_config_open = self.filter_open;
            if ui
                .selectable_label(filter_config_open, "Configure filter")
                .clicked()
            {
                self.filter_open = !self.filter_open;
            }

            let column_picker_open = self.column_picker_open;

            if ui
                .selectable_label(column_picker_open, "Enabled columns")
                .clicked()
            {
                self.column_picker_open = !self.column_picker_open;
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Apply filter").clicked() || apply_filter {
                self.apply_filter(file_name, records.as_ref());
            }

            if ui.button("Clear filter").clicked() {
                self.clear_filter();
            }
        });

        ui.horizontal(|ui| {
            let path_picker_btn = {
                let label = if let Some(name) = &active_path_name {
                    format!("Path: {}", name)
                } else {
                    "Select a path".to_string()
                };

                ui.button(label)
            };

            if path_picker_btn.clicked() {
                self.path_picker_open = !self.path_picker_open;
            }

            let creator_btn = ui.button("Label & Overlay creator");

            if creator_btn.clicked() {
                self.creator_open = !self.creator_open;
            }
        });

        ui.horizontal(|ui| {
            let path_name_range = if let Some(name) = &active_path_name {
                let n = name.as_bytes();
                crate::annotations::path_name_chr_range(n.as_bytes())
            } else {
                None
            };

            let range_filter_btn = ui.add(
                egui::Button::new("Filter by path range")
                    .enabled(path_name_range.is_some()),
            );

            if let Some((chr, start, end)) = path_name_range {
                if range_filter_btn.clicked() {
                    let filter = self.filters.get_mut(file_name).unwrap();
                    filter.chr_range_filter(chr, start, end);
                }
            }
        });

        let enabled_columns = self.enabled_columns.get(file_name).unwrap();

        let grid =
            egui::Grid::new("record_list_grid")
                .striped(true)
                .show(ui, |ui| {
                    ui.label("seq_id");
                    ui.label("start");
                    ui.label("end");

                    // TODO fix header

                    /*
                    if enabled_columns.get_column(&Gff::Source) {
                        ui.label("source");
                    }
                    if enabled_columns.get_column(&Gff::Type) {
                        ui.label("type");
                    }
                    if enabled_columns.get_column(&Gff::Frame) {
                        ui.label("frame");
                    }

                    let mut keys =
                        records.attribute_keys.iter().collect::<Vec<_>>();
                    keys.sort_by(|k1, k2| k1.cmp(k2));

                    for key in keys {
                        if enabled_columns
                            .get_column(&Gff::Attribute(key.to_owned()))
                        {
                            ui.label(format!("{}", key.as_bstr()));
                        }
                    }
                    */

                    ui.end_row();

                    for i in 0..self.slot_count {
                        let row_record = if self.filtered_records.is_empty() {
                            records.records().get(self.offset + i).map(
                                |record| {
                                    (
                                        self.ui_row(
                                            ui,
                                            file_name,
                                            records.as_ref(),
                                            record,
                                            i,
                                        ),
                                        record,
                                    )
                                },
                            )
                        } else {
                            self.filtered_records.get(self.offset + i).and_then(
                                |&ix| {
                                    let record = records.records().get(ix)?;
                                    let row = self.ui_row(
                                        ui,
                                        file_name,
                                        records.as_ref(),
                                        record,
                                        i,
                                    );
                                    Some((row, record))
                                },
                            )
                        };

                        if let Some((row, record)) = row_record {
                            let row_interact = ui.interact(
                                row.rect,
                                egui::Id::new(ui.id().with(i)),
                                egui::Sense::click(),
                            );

                            if row_interact.clicked() {
                                self.select_record(
                                    app_msg_tx,
                                    graph_query.graph(),
                                    record,
                                );
                            }
                            if row_interact.double_clicked() {
                                app_msg_tx.send(AppMsg::GotoSelection).unwrap();
                            }
                        }
                    }
                });

        if grid.response.hover_pos().is_some() {
            let scroll = ui.input().scroll_delta;
            if scroll.y.abs() >= 4.0 {
                let sig = (scroll.y.signum() as isize) * -1;
                let delta = sig * ((scroll.y.abs() as isize) / 4);

                let mut offset = self.offset as isize;

                offset += delta;

                offset = offset.clamp(
                    0,
                    (records.records().len() - self.slot_count) as isize,
                );
                self.offset = offset as usize;
            }
        }

        // if let Some(resp) = &resp {
        //     let pos = resp.rect.right_top();
        let enabled_columns = self.enabled_columns.get_mut(file_name).unwrap();
        enabled_columns.ui(
            ui.ctx(),
            None,
            // Some(pos.into()),
            &mut self.column_picker_open,
            "Gff3 Columns",
        );
        // }
    }
}
