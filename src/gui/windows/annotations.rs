use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bstr::ByteSlice;
use crossbeam::channel::Sender;
use futures::executor::ThreadPool;

pub mod bed;
pub mod gff;

pub use bed::*;
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
use rustc_hash::FxHashMap;

use crate::{
    annotations::{
        AnnotationCollection, AnnotationFileType, AnnotationLabelSet,
        AnnotationRecord, Annotations, BedRecords, ColumnKey, Gff3Column,
        Gff3Records,
    },
    app::AppMsg,
    asynchronous::AsyncResult,
    geometry::Point,
    graph_query::{GraphQuery, GraphQueryWorker},
    gui::{util::grid_row_label, GuiMsg, Windows},
    overlays::OverlayData,
};

use super::{file::FilePicker, overlays::OverlayCreatorMsg};

pub struct AnnotationRecordList {
    current_file: Option<String>,

    file_type: AnnotationFileType,
    offset: usize,
    slot_count: usize,
}

pub struct LabelSetList {}

impl LabelSetList {
    pub const ID: &'static str = "label_set_list";

    pub fn ui(
        // &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        annotations: &Annotations,
    ) -> Option<egui::Response> {
        egui::Window::new("Label sets")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |mut ui| {
                egui::ScrollArea::auto_sized().show(&mut ui, |mut ui| {
                    egui::Grid::new("label_set_list_grid").striped(true).show(
                        &mut ui,
                        |ui| {
                            ui.label("Name");
                            ui.label("File");
                            ui.label("Column");
                            ui.label("Path");
                            ui.label("Visible");
                            ui.end_row();

                            let mut label_sets = annotations
                                .label_sets()
                                .into_iter()
                                .collect::<Vec<_>>();

                            label_sets.sort_by(|(n1, _), (n2, _)| n1.cmp(n2));

                            for (name, label_set) in label_sets {
                                let file_name =
                                    if label_set.annotation_name.len() > 20 {
                                        let file_name =
                                            label_set.annotation_name.as_str();
                                        let len = file_name.len();

                                        let start = &file_name[0..8];
                                        let end = &file_name[len - 8..];

                                        format!("{}...{}", start, end)
                                    } else {
                                        label_set.annotation_name.to_string()
                                    };

                                let is_visible =
                                    format!("{}", label_set.is_visible());

                                let fields: [&str; 5] = [
                                    &name,
                                    &file_name,
                                    &label_set.column_str,
                                    &label_set.path_name,
                                    &is_visible,
                                ];

                                let row = grid_row_label(
                                    ui,
                                    egui::Id::new(ui.id().with(name)),
                                    &fields,
                                    false,
                                );

                                if row.clicked() {
                                    label_set.set_visibility(
                                        !label_set.is_visible(),
                                    );
                                }
                            }
                        },
                    );
                });
            })
    }
}

pub struct AnnotationFileList {
    current_annotation: Option<(AnnotationFileType, String)>,

    file_picker: FilePicker,
    file_picker_open: bool,

    gff3_load_result: Option<AsyncResult<Result<Gff3Records>>>,
    bed_load_result: Option<AsyncResult<Result<BedRecords>>>,
    // overlay_label_set_creator: OverlayLabelSetCreator,
    // creator_open: bool,
}

impl std::default::Default for AnnotationFileList {
    fn default() -> Self {
        let pwd = std::fs::canonicalize("./").unwrap();

        let mut file_picker = FilePicker::new(
            egui::Id::with(egui::Id::new(Self::ID), "file_picker"),
            pwd,
        )
        .unwrap();

        let extensions: [&str; 2] = ["gff3", "bed"];
        file_picker.set_visible_extensions(&extensions).unwrap();

        Self {
            current_annotation: None,

            file_picker,
            file_picker_open: false,

            gff3_load_result: None,
            bed_load_result: None,
            // overlay_label_set_creator: OverlayLabelSetCreator::new(
            //     "overlay_label_set_creator",
            // ),
            // creator_open: false,
        }
    }
}

impl AnnotationFileList {
    pub const ID: &'static str = "annotation_file_list";

    pub fn current_annotation(&self) -> Option<(AnnotationFileType, &str)> {
        self.current_annotation
            .as_ref()
            .map(|(t, n)| (*t, n.as_str()))
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        thread_pool: &ThreadPool,
        open: &mut bool,
        app_msg_tx: &crossbeam::channel::Sender<AppMsg>,
        gui_msg_tx: &crossbeam::channel::Sender<GuiMsg>,
        annotations: &Annotations,
    ) -> Option<egui::Response> {
        if let Some(query) = self.gff3_load_result.as_mut() {
            query.move_result_if_ready();
        }

        if let Some(gff3_result) = self
            .gff3_load_result
            .as_mut()
            .and_then(|r| r.take_result_if_ready())
        {
            match gff3_result {
                Ok(records) => {
                    let name = records.file_name().to_string();
                    app_msg_tx.send(AppMsg::AddGff3Records(records)).unwrap();
                    gui_msg_tx
                        .send(GuiMsg::SetWindowOpen {
                            window: Windows::AnnotationRecords,
                            open: Some(true),
                        })
                        .unwrap();

                    self.current_annotation =
                        Some((AnnotationFileType::Gff3, name));
                }
                Err(err) => {
                    eprintln!("error loading GFF3 file: {}", err);
                }
            }
            self.gff3_load_result = None;
        }

        if let Some(query) = self.bed_load_result.as_mut() {
            query.move_result_if_ready();
        }

        if let Some(bed_result) = self
            .bed_load_result
            .as_mut()
            .and_then(|r| r.take_result_if_ready())
        {
            match bed_result {
                Ok(records) => {
                    let name = records.file_name().to_string();
                    app_msg_tx.send(AppMsg::AddBedRecords(records)).unwrap();
                    gui_msg_tx
                        .send(GuiMsg::SetWindowOpen {
                            window: Windows::AnnotationRecords,
                            open: Some(true),
                        })
                        .unwrap();

                    self.current_annotation =
                        Some((AnnotationFileType::Bed, name));
                }
                Err(err) => {
                    eprintln!("error loading BED file: {}", err);
                }
            }
            self.bed_load_result = None;
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
                    if ui.button("Choose annotation file").clicked() {
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
                        if let Some((path, ext)) =
                            self.file_picker.selected_path().and_then(|path| {
                                let ext = path.extension()?;
                                let ext_str = ext.to_str()?;
                                Some((path, ext_str))
                            })
                        {
                            let path_str = path.to_str();

                            let ext = ext.to_ascii_lowercase();
                            if ext == "gff3" {
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
                                self.file_picker.reset_selection();
                            } else if ext == "bed" {
                                eprintln!("Loading BED file {:?}", path_str);
                                let path = path.to_owned();
                                let query =
                                    AsyncResult::new(thread_pool, async move {
                                        println!("parsing bed file");
                                        let records =
                                            BedRecords::parse_bed_file(path);
                                        println!("parsing complete");
                                        records
                                    });
                                self.bed_load_result = Some(query);
                                self.file_picker.reset_selection();
                            }
                        }
                    }
                } else {
                    ui.label("Loading file");
                }

                // if ui
                //     .selectable_label(self.creator_open, "Open creator")
                //     .clicked()
                // {
                //     self.creator_open = !self.creator_open;
                // }

                // self.overlay_label_set_creator.ui

                ui.separator();

                egui::ScrollArea::auto_sized().show(&mut ui, |mut ui| {
                    egui::Grid::new("annotations_file_list_grid")
                        .spacing(Point::new(10.0, 5.0))
                        .striped(true)
                        .show(&mut ui, |ui| {
                            ui.label("File name");

                            ui.separator();
                            ui.label("# Records");

                            // ui.separator();
                            // ui.label("Ref. path");

                            ui.separator();
                            ui.label("Type");

                            ui.end_row();

                            for (name, annot_type) in annotations.annot_names()
                            {
                                let record_len = match annot_type {
                                    AnnotationFileType::Gff3 => {
                                        let records =
                                            annotations.get_gff3(name).unwrap();
                                        format!("{}", records.len())
                                    }
                                    AnnotationFileType::Bed => {
                                        let records =
                                            annotations.get_bed(name).unwrap();
                                        format!("{}", records.len())
                                    }
                                };

                                let type_str = format!("{:?}", annot_type);

                                let fields = [
                                    name.as_str(),
                                    record_len.as_str(),
                                    type_str.as_str(),
                                ];

                                let row = grid_row_label(
                                    ui,
                                    egui::Id::new(ui.id().with(name)),
                                    &fields,
                                    true,
                                );

                                if row.clicked() {
                                    self.current_annotation =
                                        Some((*annot_type, name.to_string()));

                                    gui_msg_tx
                                        .send(GuiMsg::SetWindowOpen {
                                            window: Windows::AnnotationRecords,
                                            open: Some(true),
                                        })
                                        .unwrap();
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

// pub struct OverlayLabelSetCreator<T: AnnotationCollection> {
pub struct OverlayLabelSetCreator {
    path_id: Option<PathId>,
    path_name: String,

    overlay_name: String,
    overlay_description: String,

    overlay_query: Option<AsyncResult<OverlayData>>,

    label_set_name: String,

    // new_overlay_tx: Sender<OverlayCreatorMsg>,

    // column_picker: ColumnPickerOne<T>,
    column_picker_gff3: ColumnPickerOne<Gff3Records>,
    column_picker_bed: ColumnPickerOne<BedRecords>,
    column_picker_open: bool,
    current_annotation_file: Option<String>,
    // current_annotation_type: Option<AnnotationFileType>,
    id: egui::Id,
}

// impl<T: AnnotationCollection> OverlayLabelSetCreator<T> {
impl OverlayLabelSetCreator {
    pub fn new(id_source: &str) -> Self {
        let id = egui::Id::new(id_source);

        Self {
            path_id: None,
            path_name: String::new(),

            overlay_name: String::new(),
            overlay_description: String::new(),

            overlay_query: None,

            label_set_name: String::new(),

            column_picker_gff3: ColumnPickerOne::new(
                "label_set_overlay_creator_gff3_columns",
            ),
            column_picker_bed: ColumnPickerOne::new(
                "label_set_overlay_creator_bed_columns",
            ),
            column_picker_open: false,
            current_annotation_file: None,

            id,
        }
    }

    // for now it's hardcoded to use Gff3Records, but that should be
    // replaced with an enum or something similar later
    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        overlay_tx: &Sender<OverlayCreatorMsg>,
        app_msg_tx: &Sender<AppMsg>,
        graph: &GraphQueryWorker,
        open: &mut bool,
        file_name: &str,
        path_id: PathId,
        records: Arc<Gff3Records>,
        filtered_records: &[usize],
    ) -> Option<egui::Response> {
        if let Some(query) = self.overlay_query.as_mut() {
            query.move_result_if_ready();
        }

        if Some(path_id) != self.path_id {
            let path_name =
                graph.graph().graph().get_path_name_vec(path_id).unwrap();
            let path_name = path_name.to_str().unwrap().to_string();
            self.path_id = Some(path_id);
            self.path_name = path_name;
        }

        if let Some(ov_data) = self
            .overlay_query
            .as_mut()
            .and_then(|r| r.take_result_if_ready())
        {
            let msg = OverlayCreatorMsg::NewOverlay {
                name: self.overlay_name.clone(),
                data: ov_data,
            };

            self.overlay_name.clear();
            overlay_tx.send(msg).unwrap();

            self.overlay_query = None;
        }

        if self.current_annotation_file.as_ref().map(|s| s.as_str())
            != Some(file_name)
        {
            self.current_annotation_file = Some(file_name.to_string());
            self.column_picker_gff3.update_columns(&records);
        }

        {
            let column_picker_open = &mut self.column_picker_open;

            self.column_picker_gff3
                .ui(ctx, column_picker_open, "GFF3 Columns");
        }

        let label = {
            let column_picker = &self.column_picker_gff3;
            let column = column_picker.chosen_column();

            if let Some(column) = column {
                format!("Use column {}", column)
            } else {
                format!("Choose column")
            }
        };

        egui::Window::new("Create Annotation Labels & Overlays")
            .id(self.id)
            .open(open)
            .show(ctx, |ui| {
                ui.label(file_name);

                ui.label(&label);

                let column_picker_open = &mut self.column_picker_open;

                let column_picker_btn =
                    { ui.selectable_label(*column_picker_open, label) };

                if column_picker_btn.clicked() {
                    *column_picker_open = !*column_picker_open;
                }

                ui.separator();

                let name = &mut self.overlay_name;

                let _name_box = ui.horizontal(|ui| {
                    ui.label("Overlay name");
                    ui.separator();
                    ui.text_edit_singleline(name)
                });
                let column_picker = &self.column_picker_gff3;
                let column = column_picker.chosen_column();

                let create_overlay = ui.add(
                    egui::Button::new("Create overlay")
                        .enabled(column.is_some()),
                );

                if create_overlay.clicked() && self.overlay_query.is_none() {
                    println!("creating overlay");
                    if let Some(column) = column {
                        let indices = filtered_records
                            .iter()
                            .copied()
                            .collect::<Vec<_>>();

                        let column = column.to_owned();

                        let records = records.clone();

                        let query = graph.run_query(move |graph| async move {
                            use rayon::prelude::*;

                            use crate::annotations as annots;

                            dbg!();

                            let steps = graph.path_pos_steps(path_id).unwrap();

                            let offset = graph
                                .graph()
                                .get_path_name_vec(path_id)
                                .and_then(|name| {
                                    annots::path_name_offset(&name)
                                });

                            println!("using annotation offset {:?}", offset);

                            let t0 = std::time::Instant::now();
                            let colors_vec: Vec<(Vec<NodeId>, rgb::RGB<f32>)> =
                                indices
                                    .into_par_iter()
                                    .filter_map(|ix| {
                                        let record = records.records.get(ix)?;

                                        let color = gff3_column_hash_color(
                                            record, &column,
                                        )?;

                                        let range = annots::path_step_range(
                                            &steps,
                                            offset,
                                            record.start(),
                                            record.end(),
                                        )?;

                                        let ids = range
                                            .into_iter()
                                            .map(|(h, _, _)| h.id())
                                            .collect();

                                        Some((ids, color))
                                    })
                                    .collect::<Vec<_>>();

                            println!(
                                "parallel processing took {} seconds",
                                t0.elapsed().as_secs_f64()
                            );
                            let applied_records_count = colors_vec.len();
                            println!(
                                "colored record count: {}",
                                applied_records_count
                            );
                            let colored_node_count: usize = colors_vec
                                .iter()
                                .map(|(nodes, _)| nodes.len())
                                .sum();
                            println!(
                                "colored node count: {}",
                                colored_node_count
                            );

                            dbg!();

                            let t1 = std::time::Instant::now();
                            let mut node_colors: FxHashMap<
                                NodeId,
                                rgb::RGB<f32>,
                            > = FxHashMap::default();

                            for (ids, color) in colors_vec {
                                for id in ids {
                                    node_colors.insert(id, color);
                                }
                            }

                            println!(
                                "building color map took {} seconds",
                                t1.elapsed().as_secs_f64()
                            );

                            let t2 = std::time::Instant::now();
                            let mut data =
                                vec![
                                    rgb::RGBA::new(0.3, 0.3, 0.3, 0.3);
                                    graph.node_count()
                                ];

                            for (id, color) in node_colors {
                                let ix = (id.0 - 1) as usize;
                                data[ix] = rgb::RGBA::new(
                                    color.r, color.g, color.b, 1.0,
                                );
                            }

                            println!(
                                "building color vector took {} seconds",
                                t2.elapsed().as_secs_f64()
                            );

                            OverlayData::RGB(data)
                        });

                        self.overlay_query = Some(query);
                    }
                }

                ui.separator();

                {
                    let name = &mut self.label_set_name;

                    let _name_box = ui.horizontal(|ui| {
                        ui.label("Label set name");
                        ui.separator();
                        ui.text_edit_singleline(name)
                    });
                }

                let column_picker = &self.column_picker_gff3;
                let column = column_picker.chosen_column();

                let create_label_set = ui.add(
                    egui::Button::new("Create label set")
                        .enabled(column.is_some()),
                );

                if create_label_set.clicked() {
                    if let Some((path, labels)) = Self::calculate_annotations(
                        graph.graph(),
                        records.as_ref(),
                        filtered_records,
                        path_id,
                        &self.path_name,
                        column.unwrap(),
                    ) {
                        let label_set = AnnotationLabelSet::new(
                            records.as_ref(),
                            path,
                            self.path_name.as_bytes(),
                            column.unwrap(),
                            labels,
                        );

                        let name = std::mem::take(&mut self.label_set_name);

                        app_msg_tx
                            .send(AppMsg::NewNodeLabels { name, label_set })
                            .unwrap();
                    }
                }
            })
    }

    pub fn ui_bed(
        &mut self,
        ctx: &egui::CtxRef,
        overlay_tx: &Sender<OverlayCreatorMsg>,
        app_msg_tx: &Sender<AppMsg>,
        graph: &GraphQueryWorker,
        open: &mut bool,
        file_name: &str,
        path_id: PathId,
        records: Arc<BedRecords>,
        filtered_records: &[usize],
    ) -> Option<egui::Response> {
        if let Some(query) = self.overlay_query.as_mut() {
            query.move_result_if_ready();
        }

        if Some(path_id) != self.path_id {
            let path_name =
                graph.graph().graph().get_path_name_vec(path_id).unwrap();
            let path_name = path_name.to_str().unwrap().to_string();
            self.path_id = Some(path_id);
            self.path_name = path_name;
        }

        if let Some(ov_data) = self
            .overlay_query
            .as_mut()
            .and_then(|r| r.take_result_if_ready())
        {
            let msg = OverlayCreatorMsg::NewOverlay {
                name: self.overlay_name.clone(),
                data: ov_data,
            };

            self.overlay_name.clear();
            overlay_tx.send(msg).unwrap();

            self.overlay_query = None;
        }

        if self.current_annotation_file.as_ref().map(|s| s.as_str())
            != Some(file_name)
        {
            self.current_annotation_file = Some(file_name.to_string());
            self.column_picker_bed.update_columns(&records);
        }

        {
            let column_picker_open = &mut self.column_picker_open;

            self.column_picker_bed
                .ui(ctx, column_picker_open, "BED Columns");
        }

        let label = {
            let column_picker = &self.column_picker_bed;
            let column = column_picker.chosen_column();

            if let Some(column) = column {
                format!("Use column {}", column)
            } else {
                format!("Choose column")
            }
        };

        egui::Window::new("Create Annotation Labels & Overlays")
            .id(egui::Id::with(self.id, "bed"))
            .open(open)
            .show(ctx, |ui| {
                ui.label(file_name);

                ui.label(&label);

                let column_picker_open = &mut self.column_picker_open;

                let column_picker_btn =
                    { ui.selectable_label(*column_picker_open, label) };

                if column_picker_btn.clicked() {
                    *column_picker_open = !*column_picker_open;
                }

                ui.separator();

                let name = &mut self.overlay_name;

                let _name_box = ui.horizontal(|ui| {
                    ui.label("Overlay name");
                    ui.separator();
                    ui.text_edit_singleline(name)
                });
                let column_picker = &self.column_picker_bed;
                let column = column_picker.chosen_column();

                let create_overlay = ui.add(
                    egui::Button::new("Create overlay")
                        .enabled(column.is_some()),
                );

                if create_overlay.clicked() && self.overlay_query.is_none() {
                    println!("creating overlay");
                    if let Some(column) = column {
                        let indices = filtered_records
                            .iter()
                            .copied()
                            .collect::<Vec<_>>();

                        let column = column.to_owned();

                        let records = records.clone();

                        let query = graph.run_query(move |graph| async move {
                            use rayon::prelude::*;

                            use crate::annotations as annots;

                            dbg!();

                            let steps = graph.path_pos_steps(path_id).unwrap();

                            let offset = graph
                                .graph()
                                .get_path_name_vec(path_id)
                                .and_then(|name| {
                                    annots::path_name_offset(&name)
                                });

                            println!("using annotation offset {:?}", offset);

                            let t0 = std::time::Instant::now();
                            let colors_vec: Vec<(Vec<NodeId>, rgb::RGB<f32>)> =
                                indices
                                    .into_par_iter()
                                    .filter_map(|ix| {
                                        let record = records.records.get(ix)?;

                                        let color = bed_column_hash_color(
                                            record, &column,
                                        )?;

                                        let range = annots::path_step_range(
                                            &steps,
                                            offset,
                                            record.start(),
                                            record.end(),
                                        )?;

                                        let ids = range
                                            .into_iter()
                                            .map(|(h, _, _)| h.id())
                                            .collect();

                                        Some((ids, color))
                                    })
                                    .collect::<Vec<_>>();

                            println!(
                                "parallel processing took {} seconds",
                                t0.elapsed().as_secs_f64()
                            );
                            let applied_records_count = colors_vec.len();
                            println!(
                                "colored record count: {}",
                                applied_records_count
                            );
                            let colored_node_count: usize = colors_vec
                                .iter()
                                .map(|(nodes, _)| nodes.len())
                                .sum();
                            println!(
                                "colored node count: {}",
                                colored_node_count
                            );

                            dbg!();

                            let t1 = std::time::Instant::now();
                            let mut node_colors: FxHashMap<
                                NodeId,
                                rgb::RGB<f32>,
                            > = FxHashMap::default();

                            for (ids, color) in colors_vec {
                                for id in ids {
                                    node_colors.insert(id, color);
                                }
                            }

                            println!(
                                "building color map took {} seconds",
                                t1.elapsed().as_secs_f64()
                            );

                            let t2 = std::time::Instant::now();
                            let mut data =
                                vec![
                                    rgb::RGBA::new(0.3, 0.3, 0.3, 0.3);
                                    graph.node_count()
                                ];

                            for (id, color) in node_colors {
                                let ix = (id.0 - 1) as usize;
                                data[ix] = rgb::RGBA::new(
                                    color.r, color.g, color.b, 1.0,
                                );
                            }

                            println!(
                                "building color vector took {} seconds",
                                t2.elapsed().as_secs_f64()
                            );

                            OverlayData::RGB(data)
                        });

                        self.overlay_query = Some(query);
                    }
                }

                ui.separator();

                {
                    let name = &mut self.label_set_name;

                    let _name_box = ui.horizontal(|ui| {
                        ui.label("Label set name");
                        ui.separator();
                        ui.text_edit_singleline(name)
                    });
                }

                let column_picker = &self.column_picker_bed;
                let column = column_picker.chosen_column();

                let create_label_set = ui.add(
                    egui::Button::new("Create label set")
                        .enabled(column.is_some()),
                );

                if create_label_set.clicked() {
                    if let Some((path, labels)) = Self::calculate_annotations(
                        graph.graph(),
                        records.as_ref(),
                        filtered_records,
                        path_id,
                        &self.path_name,
                        column.unwrap(),
                    ) {
                        let label_set = AnnotationLabelSet::new(
                            records.as_ref(),
                            path,
                            self.path_name.as_bytes(),
                            column.unwrap(),
                            labels,
                        );

                        let name = std::mem::take(&mut self.label_set_name);

                        app_msg_tx
                            .send(AppMsg::NewNodeLabels { name, label_set })
                            .unwrap();
                    }
                }
            })
    }

    fn calculate_annotations<C, R, K>(
        graph: &GraphQuery,
        records: &C,
        record_indices: &[usize],
        path_id: PathId,
        path_name: &str,
        column: &K,
    ) -> Option<(PathId, FxHashMap<NodeId, Vec<String>>)>
    where
        C: AnnotationCollection<ColumnKey = K, Record = R>,
        R: AnnotationRecord<ColumnKey = K>,
        K: ColumnKey,
    {
        if record_indices.is_empty() {
            return None;
        }

        let offset = crate::annotations::path_name_offset(path_name.as_bytes());

        let steps = graph.path_pos_steps(path_id)?;

        // let records_slice = records.records()
        let mut result: FxHashMap<NodeId, Vec<String>> = FxHashMap::default();

        for &record_ix in record_indices.iter() {
            let record = records.records().get(record_ix)?;

            if let Some(range) = crate::annotations::path_step_range(
                &steps,
                offset,
                record.start(),
                record.end(),
            ) {
                if let Some(value) = record.get_first(column) {
                    if let Some((mid, _, _)) = range.get(range.len() / 2) {
                        let label = format!("{}", value.as_bstr());
                        result.entry(mid.id()).or_default().push(label);
                    }
                }
            }
        }

        for labels in result.values_mut() {
            labels.sort();
            labels.dedup();
            labels.shrink_to_fit();
        }

        Some((path_id, result))
    }
}
