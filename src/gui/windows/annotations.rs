pub mod gff;

use std::collections::{HashMap, HashSet};

use futures::executor::ThreadPool;
pub use gff::*;

#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    packedgraph::index::OneBasedIndex,
    packedgraph::*,
    path_position::*,
    pathhandlegraph::*,
};

use anyhow::Result;

use crate::{
    annotations::{
        AnnotationCollection, AnnotationRecord, Annotations, Gff3Records,
    },
    app::AppMsg,
    asynchronous::AsyncResult,
    geometry::Point,
};

use super::file::FilePicker;

pub struct AnnotationFileList {
    current_annotation: Option<String>,

    file_picker: FilePicker,
    file_picker_open: bool,

    gff3_load_result: Option<AsyncResult<Result<Gff3Records>>>,
}

impl std::default::Default for AnnotationFileList {
    fn default() -> Self {
        let pwd = std::fs::canonicalize("./").unwrap();

        let file_picker = FilePicker::new(
            egui::Id::with(egui::Id::new(Self::ID), "file_picker"),
            pwd,
        )
        .unwrap();

        Self {
            current_annotation: None,

            file_picker,
            file_picker_open: false,

            gff3_load_result: None,
        }
    }
}

impl AnnotationFileList {
    pub const ID: &'static str = "annotation_file_list";

    pub fn current_annotation(&self) -> Option<&str> {
        self.current_annotation.as_ref().map(|n| n.as_str())
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        thread_pool: &ThreadPool,
        open: &mut bool,
        app_msg_tx: &crossbeam::channel::Sender<AppMsg>,
        annotations: &Annotations,
    ) -> Option<egui::Response> {
        if let Some(query) = self.gff3_load_result.as_mut() {
            query.move_result_if_ready();
            self.file_picker.reset_selection();
        }

        if let Some(gff3_result) = self
            .gff3_load_result
            .as_mut()
            .and_then(|r| r.take_result_if_ready())
        {
            match gff3_result {
                Ok(records) => {
                    app_msg_tx.send(AppMsg::AddGff3Records(records)).unwrap();
                }
                Err(err) => {
                    eprintln!("error loading GFF3 file: {}", err);
                }
            }
            self.gff3_load_result = None;
        }

        if self.file_picker.selected_path().is_some() {
            self.file_picker_open = false;
        }

        self.file_picker.ui(ctx, &mut self.file_picker_open);

        egui::Window::new("Annotation Files")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |mut ui| {
                if self.gff3_load_result.is_none() {
                    if ui.button("Choose GFF3 file").clicked() {
                        self.file_picker_open = true;
                    }

                    let _label = if let Some(path) = self
                        .file_picker
                        .selected_path()
                        .and_then(|p| p.to_str())
                    {
                        ui.label(path)
                    } else {
                        ui.label("No file selected")
                    };

                    if ui.button("Load").clicked() {
                        if let Some(path) = self.file_picker.selected_path() {
                            let path_str = path.to_str();
                            eprintln!("Loading GFF3 file {:?}", path_str);
                            let path = path.to_owned();
                            let query =
                                AsyncResult::new(thread_pool, async move {
                                    println!("parsing gff3 file");
                                    let records =
                                        Gff3Records::parse_gff3_file(path);
                                    println!("parsing complete");
                                    records
                                });
                            self.gff3_load_result = Some(query);
                        }
                    }
                } else {
                    ui.label("Loading file");
                }

                ui.separator();

                egui::ScrollArea::auto_sized().show(&mut ui, |mut ui| {
                    egui::Grid::new("annotations_file_list_grid")
                        .spacing(Point::new(10.0, 5.0))
                        .striped(true)
                        .show(&mut ui, |ui| {
                            ui.label("File name");
                            ui.separator();

                            ui.label("# Records");
                            ui.separator();

                            // ui.label("Ref. path");
                            // ui.separator();

                            // ui.label("Type");
                            // ui.separator();

                            ui.end_row();

                            for (name, annot_type) in annotations.annot_names()
                            {
                                let record =
                                    annotations.get_gff3(name).unwrap();

                                let mut row = ui.label(name);
                                row = row.union(ui.separator());

                                row = row.union(
                                    ui.label(format!("{}", record.len())),
                                );
                                row = row.union(ui.separator());

                                // row = row.union(ui.label("TODO path"));
                                // row = row.union(ui.separator());

                                let row_interact = ui.interact(
                                    row.rect,
                                    egui::Id::new(ui.id().with(name)),
                                    egui::Sense::click(),
                                );

                                if row_interact.clicked() {
                                    self.current_annotation =
                                        Some(name.to_string());
                                }

                                ui.end_row();
                            }
                        })
                });
            })
    }
}

pub struct ColumnPickerOne<T: AnnotationCollection> {
    columns: Vec<T::ColumnKey>,
    chosen_column: Option<usize>,

    id: egui::Id,
}

impl<T: AnnotationCollection> ColumnPickerOne<T> {
    pub fn new(id_source: &str) -> Self {
        let id = egui::Id::new(id_source);

        Self {
            columns: Vec::new(),
            chosen_column: None,

            id,
        }
    }

    pub fn update_columns(&mut self, records: &T) {
        self.chosen_column = None;
        self.columns = records.all_columns();
    }

    pub fn chosen_column(&self) -> Option<&T::ColumnKey> {
        let ix = self.chosen_column?;
        self.columns.get(ix)
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        window_name: &str,
    ) -> Option<egui::Response> {
        egui::Window::new(window_name).id(self.id).open(open).show(
            ctx,
            |mut ui| {
                egui::ScrollArea::from_max_height(
                    ui.input().screen_rect.height() - 250.0,
                )
                .show(&mut ui, |ui| {
                    let chosen_column = self.chosen_column;

                    for (ix, col) in self.columns.iter().enumerate() {
                        let active = chosen_column == Some(ix);
                        if ui
                            .selectable_label(active, col.to_string())
                            .clicked()
                        {
                            if active {
                                self.chosen_column = None;
                            } else {
                                self.chosen_column = Some(ix);
                            }
                        }
                    }
                });
            },
        )
    }
}

pub struct ColumnPickerMany<T: AnnotationCollection> {
    enabled_columns: HashMap<T::ColumnKey, bool>,

    hidden_columns: HashSet<T::ColumnKey>,

    id: egui::Id,
}

impl<T: AnnotationCollection> ColumnPickerMany<T> {
    pub fn new(id_source: &str) -> Self {
        let id = egui::Id::new(id_source);

        Self {
            enabled_columns: Default::default(),
            hidden_columns: Default::default(),

            id,
        }
    }

    pub fn update_columns(&mut self, records: &T) {
        let columns = records.all_columns();
        self.enabled_columns =
            columns.into_iter().map(|c| (c, false)).collect();
        self.hidden_columns.clear();
    }

    pub fn get_column(&self, column: &T::ColumnKey) -> bool {
        self.enabled_columns.get(column).copied().unwrap_or(false)
    }

    pub fn set_column(&mut self, column: &T::ColumnKey, to: bool) {
        self.enabled_columns.insert(column.clone(), to);
    }

    pub fn hide_column_from_gui(&mut self, column: &T::ColumnKey, hide: bool) {
        if hide {
            self.hidden_columns.insert(column.clone());
        } else {
            self.hidden_columns.remove(column);
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        pos: impl Into<egui::Pos2>,
        open: &mut bool,
        window_name: &str,
    ) -> Option<egui::Response> {
        egui::Window::new(window_name)
            .id(self.id)
            .fixed_pos(pos)
            .collapsible(false)
            .open(open)
            .show(ctx, |ui| {
                let max_height = ui.input().screen_rect.height() - 250.0;
                ui.set_max_height(max_height);

                let hidden_columns = &self.hidden_columns;

                let mut columns = self
                    .enabled_columns
                    .iter_mut()
                    .filter(|(c, _)| !hidden_columns.contains(c))
                    .collect::<Vec<_>>();

                columns.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

                let (optional, mandatory): (Vec<_>, Vec<_>) = columns
                    .into_iter()
                    .partition(|(col, _en)| T::is_column_optional(col));

                let scroll_height = (max_height / 2.0) - 50.0;

                ui.collapsing("Mandatory fields", |mut ui| {
                    egui::ScrollArea::from_max_height(scroll_height).show(
                        &mut ui,
                        |ui| {
                            for (key, enabled) in mandatory.into_iter() {
                                if ui
                                    .selectable_label(*enabled, key.to_string())
                                    .clicked()
                                {
                                    *enabled = !*enabled;
                                }
                            }
                        },
                    );
                });

                ui.collapsing("Optional fields", |mut ui| {
                    egui::ScrollArea::from_max_height(scroll_height).show(
                        &mut ui,
                        |ui| {
                            for (key, enabled) in optional.into_iter() {
                                if ui
                                    .selectable_label(*enabled, key.to_string())
                                    .clicked()
                                {
                                    *enabled = !*enabled;
                                }
                            }
                        },
                    );
                });
            })
    }
}
