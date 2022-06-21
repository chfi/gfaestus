use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use bstr::ByteSlice;
use crossbeam::{atomic::AtomicCell, channel::Sender};

pub mod filter;
pub mod records_list;

pub use filter::*;
use parking_lot::{RwLock, RwLockReadGuard};
pub use records_list::*;

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
        record_column_hash_color, AnnotationCollection, AnnotationFileType,
        AnnotationLabelSet, AnnotationRecord, Annotations, BedRecords,
        ColumnKey, Gff3Records, Labels,
    },
    app::channels::OverlayCreatorMsg,
    app::AppMsg,
    geometry::Point,
    graph_query::GraphQuery,
    gui::{util::grid_row_label, GuiMsg, Windows},
    overlays::OverlayData,
    reactor::{Host, Outbox, Reactor},
};

use super::file::FilePicker;

pub struct LabelSetList {}

impl LabelSetList {
    pub const ID: &'static str = "label_set_list";

    pub fn ui(
        ctx: &egui::CtxRef,
        open: &mut bool,
        labels: &Labels,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        egui::Window::new("Label sets")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("label_set_list_grid").striped(true).show(
                        ui,
                        |ui| {
                            ui.label("Name");
                            ui.label("Visible");
                            ui.end_row();

                            let mut label_sets =
                                labels.label_sets().iter().collect::<Vec<_>>();

                            label_sets.sort_by(|(x, _), (y, _)| x.cmp(y));

                            for (name, _label_set) in label_sets {
                                let view_name = if name.len() > 20 {
                                    let len = name.len();

                                    let start = &name[0..8];
                                    let end = &name[len - 8..];

                                    format!("{}...{}", start, end)
                                } else {
                                    name.to_string()
                                };

                                let is_visible = labels
                                    .visible(name)
                                    .map(|v| v.load())
                                    .unwrap_or_default();
                                let visible_str = format!("{}", is_visible);

                                let fields: [&str; 2] =
                                    [&view_name, &visible_str];

                                let inner = grid_row_label(
                                    ui,
                                    egui::Id::new(ui.id().with(name)),
                                    &fields,
                                    false,
                                    None,
                                );
                                let row = inner.response;

                                if row.clicked() {
                                    if let Some(visibility) =
                                        labels.visible(name)
                                    {
                                        visibility.fetch_xor(true);
                                    }
                                }
                            }
                        },
                    );
                });
            })
    }
}

pub enum AnnotMsg {
    IOError(String),
    ParseError(String),
    Running(String),
}

#[allow(dead_code)]
impl AnnotMsg {
    fn io_error(err: &str) -> Self {
        AnnotMsg::IOError(err.to_string())
    }

    fn parse_error(err: &str) -> Self {
        AnnotMsg::ParseError(err.to_string())
    }

    fn running(msg: &str) -> Self {
        AnnotMsg::Running(msg.to_string())
    }
}

pub type AnnotResult =
    std::result::Result<(AnnotationFileType, String), AnnotMsg>;

pub struct AnnotationFileList {
    pub current_annotation: Arc<RwLock<Option<(AnnotationFileType, String)>>>,

    file_picker: FilePicker,
    file_picker_open: bool,

    load_host: Host<PathBuf, AnnotResult>,

    latest_result: Option<AnnotResult>,
}

impl AnnotationFileList {
    pub const ID: &'static str = "annotation_file_list";

    pub fn new(
        reactor: &Reactor,
        app_msg_tx: Sender<AppMsg>,
        gui_msg_tx: Sender<GuiMsg>,
    ) -> Result<Self> {
        let pwd = std::fs::canonicalize("./").unwrap();

        let mut file_picker = FilePicker::new(
            egui::Id::with(egui::Id::new(Self::ID), "file_picker"),
            pwd,
        )
        .unwrap();

        let extensions: [&str; 2] = ["gff3", "bed"];
        file_picker.set_visible_extensions(&extensions).unwrap();

        let load_host = reactor.create_host(
            move |outbox: &Outbox<AnnotResult>, file: PathBuf| {
                let running_msg = |msg: &str| {
                    outbox.insert_blocking(Err(AnnotMsg::running(msg)));
                };

                let ext =
                    file.extension().and_then(|ext| ext.to_str()).map_or(
                        Err(AnnotMsg::IOError(format!(
                            "Missing file extension in: {:?}",
                            file
                        ))),
                        |ext| Ok(ext),
                    )?;

                if ext == "gff3" {
                    running_msg("Loading GFF3");

                    let records = Gff3Records::parse_gff3_file(&file);
                    match records {
                        Ok(records) => {
                            let file_name = records.file_name().to_string();

                            app_msg_tx
                                .send(AppMsg::raw("add_gff3_records", records))
                                .unwrap();
                            gui_msg_tx
                                .send(GuiMsg::SetWindowOpen {
                                    window: Windows::AnnotationRecords,
                                    open: Some(true),
                                })
                                .unwrap();

                            return Ok((AnnotationFileType::Gff3, file_name));
                        }
                        Err(err) => {
                            return Err(AnnotMsg::ParseError(format!(
                                "Error parsing GFF3 file: {:?}",
                                err
                            )));
                        }
                    }
                } else if ext == "bed" {
                    running_msg("Loading BED");

                    let records = BedRecords::parse_bed_file(&file);
                    match records {
                        Ok(records) => {
                            let file_name = records.file_name().to_string();

                            app_msg_tx
                                .send(AppMsg::raw("add_bed_records", records))
                                .unwrap();
                            gui_msg_tx
                                .send(GuiMsg::SetWindowOpen {
                                    window: Windows::AnnotationRecords,
                                    open: Some(true),
                                })
                                .unwrap();

                            return Ok((AnnotationFileType::Bed, file_name));
                        }
                        Err(err) => {
                            return Err(AnnotMsg::ParseError(format!(
                                "Error parsing BED file: {:?}",
                                err
                            )));
                        }
                    }
                };

                Err(AnnotMsg::ParseError(format!(
                    "Incompatible file type: {:?}",
                    file
                )))
            },
        );

        Ok(Self {
            current_annotation: Arc::new(RwLock::new(None)),

            file_picker,
            file_picker_open: false,

            load_host,
            latest_result: None,
        })
    }

    // pub fn current_annotation(&self) -> Option<(AnnotationFileType, &str)> {
    pub fn current_annotation(
        &self,
    ) -> RwLockReadGuard<Option<(AnnotationFileType, String)>> {
        self.current_annotation.read()
    }

    pub fn file_picker_(&mut self, ctx: &egui::CtxRef) {
        self.file_picker.ui(ctx, &mut self.file_picker_open);
    }

    pub fn ui_(
        &mut self,
        gui_msg_tx: &crossbeam::channel::Sender<GuiMsg>,
        annotations: &Annotations,
        ui: &mut egui::Ui,
    ) {
        if let Some(result) = self.load_host.take() {
            if let Ok((file_type, name)) = &result {
                let mut write = self.current_annotation.write();
                *write = Some((*file_type, name.to_owned()));
            }

            self.latest_result = Some(result);
        }

        let is_running =
            matches!(self.latest_result, Some(Err(AnnotMsg::Running(_))));

        if self.file_picker.selected_path().is_some() {
            self.file_picker_open = false;
        }

        // self.file_picker.ui(ctx, &mut self.file_picker_open);

        if ui
            .add_enabled(
                !is_running,
                egui::Button::new("Choose annotation file"),
            )
            .clicked()
        {
            self.file_picker.reset_selection();
            self.file_picker_open = true;
        }

        let _label = if let Some(path) =
            self.file_picker.selected_path().and_then(|p| p.to_str())
        {
            ui.label(path)
        } else {
            ui.label("No file selected")
        };

        let selected_path = self.file_picker.selected_path();

        if ui
            .add_enabled(
                !is_running && selected_path.is_some(),
                egui::Button::new("Load"),
            )
            .clicked()
        {
            if let Some(path) = selected_path {
                self.load_host.call(path.to_owned()).unwrap();
            }
        }

        if is_running {
            ui.label("Loading file");
        }

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |mut ui| {
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

                    for (name, annot_type) in annotations.annot_names() {
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

                        let inner = grid_row_label(
                            ui,
                            egui::Id::new(ui.id().with(name)),
                            &fields,
                            true,
                            None,
                        );

                        let row = inner.response;

                        if row.clicked() {
                            {
                                let mut write = self.current_annotation.write();
                                *write = Some((*annot_type, name.to_string()));
                            }

                            gui_msg_tx
                                .send(GuiMsg::SetWindowOpen {
                                    window: Windows::AnnotationRecords,
                                    open: Some(true),
                                })
                                .unwrap();
                        }
                    }
                })
        });
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        gui_msg_tx: &crossbeam::channel::Sender<GuiMsg>,
        annotations: &Annotations,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        if let Some(result) = self.load_host.take() {
            if let Ok((file_type, name)) = &result {
                let mut write = self.current_annotation.write();
                *write = Some((*file_type, name.to_owned()));
            }

            self.latest_result = Some(result);
        }

        let is_running =
            matches!(self.latest_result, Some(Err(AnnotMsg::Running(_))));

        if self.file_picker.selected_path().is_some() {
            self.file_picker_open = false;
        }

        self.file_picker.ui(ctx, &mut self.file_picker_open);

        egui::Window::new("Annotation Files")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |mut ui| {
                if ui
                    .add_enabled(
                        !is_running,
                        egui::Button::new("Choose annotation file"),
                    )
                    .clicked()
                {
                    self.file_picker.reset_selection();
                    self.file_picker_open = true;
                }

                let _label = if let Some(path) =
                    self.file_picker.selected_path().and_then(|p| p.to_str())
                {
                    ui.label(path)
                } else {
                    ui.label("No file selected")
                };

                let selected_path = self.file_picker.selected_path();

                if ui
                    .add_enabled(
                        !is_running && selected_path.is_some(),
                        egui::Button::new("Load"),
                    )
                    .clicked()
                {
                    if let Some(path) = selected_path {
                        self.load_host.call(path.to_owned()).unwrap();
                    }
                }

                if is_running {
                    ui.label("Loading file");
                }

                ui.separator();

                egui::ScrollArea::vertical().show(&mut ui, |mut ui| {
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

                                let inner = grid_row_label(
                                    ui,
                                    egui::Id::new(ui.id().with(name)),
                                    &fields,
                                    true,
                                    None,
                                );

                                let row = inner.response;

                                if row.clicked() {
                                    {
                                        let mut write =
                                            self.current_annotation.write();
                                        *write = Some((
                                            *annot_type,
                                            name.to_string(),
                                        ));
                                    }

                                    gui_msg_tx
                                        .send(GuiMsg::SetWindowOpen {
                                            window: Windows::AnnotationRecords,
                                            open: Some(true),
                                        })
                                        .unwrap();
                                }
                            }
                        })
                });
            })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnPickerOne<T: ColumnKey> {
    columns: Vec<T>,
    chosen_column: Option<usize>,

    id: egui::Id,
}

impl<T: ColumnKey> ColumnPickerOne<T> {
    pub fn new(id: egui::Id) -> Self {
        Self {
            columns: Vec::new(),
            chosen_column: None,

            id,
        }
    }

    pub fn update_columns<C>(&mut self, records: &C)
    where
        C: AnnotationCollection<ColumnKey = T>,
    {
        self.chosen_column = None;
        self.columns = records.all_columns();
    }

    pub fn chosen_column(&self) -> Option<&T> {
        let ix = self.chosen_column?;
        self.columns.get(ix)
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        window_name: &str,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        egui::Window::new(window_name).id(self.id).open(open).show(
            ctx,
            |mut ui| {
                egui::ScrollArea::vertical()
                    .max_height(ui.input().screen_rect.height() - 250.0)
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

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnPickerMany<T: ColumnKey> {
    enabled_columns: HashMap<T, bool>,

    hidden_columns: HashSet<T>,

    id: egui::Id,
}

impl<T: ColumnKey> ColumnPickerMany<T> {
    pub fn new(id: egui::Id) -> Self {
        Self {
            enabled_columns: Default::default(),
            hidden_columns: Default::default(),

            id,
        }
    }

    pub fn clone_with_id(&self, id: egui::Id) -> Self {
        Self { id, ..self.clone() }
    }

    pub fn update_columns<C>(&mut self, records: &C)
    where
        C: AnnotationCollection<ColumnKey = T>,
    {
        let columns = records.all_columns();
        self.enabled_columns =
            columns.into_iter().map(|c| (c, false)).collect();
        self.hidden_columns.clear();
    }

    pub fn get_column(&self, column: &T) -> bool {
        self.enabled_columns.get(column).copied().unwrap_or(false)
    }

    pub fn set_column(&mut self, column: &T, to: bool) {
        self.enabled_columns.insert(column.clone(), to);
    }

    pub fn hide_column_from_gui(&mut self, column: &T, hide: bool) {
        if hide {
            self.hidden_columns.insert(column.clone());
        } else {
            self.hidden_columns.remove(column);
        }
    }

    pub fn compact_widget(&mut self, ui: &mut egui::Ui) {
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

        ui.horizontal_wrapped(|ui| {
            for (key, enabled) in mandatory.into_iter() {
                if ui.selectable_label(*enabled, key.to_string()).clicked() {
                    *enabled = !*enabled;
                }
            }
        });

        ui.horizontal_wrapped(|ui| {
            for (key, enabled) in optional.into_iter() {
                if ui.selectable_label(*enabled, key.to_string()).clicked() {
                    *enabled = !*enabled;
                }
            }
        });
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        pos: Option<egui::Pos2>,
        open: &mut bool,
        window_name: &str,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        let window = egui::Window::new(window_name).id(self.id);

        let window = if let Some(pos) = pos {
            window.fixed_pos(pos)
        } else {
            window
        };

        window.collapsible(false).open(open).show(ctx, |ui| {
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

            let scroll_height = max_height - 50.0;

            ui.collapsing("Mandatory fields", |mut ui| {
                egui::ScrollArea::vertical().max_height(scroll_height).show(
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
                egui::ScrollArea::vertical().max_height(scroll_height).show(
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

struct OverlayInput<C: AnnotationCollection + Send + Sync + 'static> {
    name: String,
    column: C::ColumnKey,
    indices: Vec<usize>,
    path: PathId,
    records: Arc<C>,
}

enum OverlayFeedback {
    Error(String),
    Running(String),
}

type OverlayResult = std::result::Result<(), OverlayFeedback>;

pub struct OverlayLabelSetCreator<C>
where
    C: AnnotationCollection + Send + Sync + 'static,
{
    path_id: Option<PathId>,
    path_name: String,

    overlay_name: String,

    host_data: Host<OverlayInput<C>, OverlayResult>,
    latest_result: Option<OverlayResult>,

    label_set_name: String,

    column_picker: ColumnPickerOne<C::ColumnKey>,
    column_picker_open: bool,
    current_annotation_file: Option<String>,

    id: egui::Id,
}

impl<C> OverlayLabelSetCreator<C>
where
    C: AnnotationCollection + Send + Sync + 'static,
{
    pub fn new(reactor: &Reactor, id: egui::Id) -> Self {
        let graph = reactor.graph_query.clone();
        let overlay_tx = reactor.overlay_create_tx.clone();

        let rayon_pool = reactor.rayon_pool.clone();

        let host_data = reactor.create_host(
            move |outbox: &Outbox<_>, input: OverlayInput<C>| {
                use rayon::prelude::*;

                let running_msg = |msg: &str| {
                    outbox.insert_blocking(Err(OverlayFeedback::Running(
                        msg.to_string(),
                    )));
                };

                running_msg("Retrieving path steps");

                let steps =
                    graph.path_pos_steps(input.path).ok_or_else(|| {
                        OverlayFeedback::Error(format!(
                            "Path {} does not exist",
                            input.path.0
                        ))
                    })?;

                let offset =
                    graph.graph().get_path_name_vec(input.path).and_then(
                        |name| crate::annotations::path_name_offset(&name),
                    );

                let indices = &input.indices;

                running_msg("Calculating node colors");

                let colors_vec: Vec<(Vec<NodeId>, rgb::RGBA<f32>)> = rayon_pool
                    .install(|| {
                        indices
                            .into_par_iter()
                            .filter_map(|&ix| {
                                let record = input.records.records().get(ix)?;

                                let color = record_column_hash_color(
                                    record,
                                    &input.column,
                                )?;

                                let range =
                                    crate::annotations::path_step_range(
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
                            .collect::<Vec<_>>()
                    });

                let mut node_colors: FxHashMap<NodeId, rgb::RGBA<f32>> =
                    FxHashMap::default();

                for (ids, color) in colors_vec {
                    for id in ids {
                        node_colors.insert(id, color);
                    }
                }

                let mut data = vec![
                    rgb::RGBA::new(0.3, 0.3, 0.3, 0.3);
                    graph.node_count()
                ];

                for (id, color) in node_colors {
                    let ix = (id.0 - 1) as usize;
                    data[ix] = color;
                }

                let overlay_data = OverlayData::RGB(data);

                overlay_tx
                    .send(OverlayCreatorMsg::NewOverlay {
                        name: input.name,
                        data: overlay_data,
                    })
                    .unwrap();

                Ok(())
            },
        );

        Self {
            path_id: None,
            path_name: String::new(),

            overlay_name: String::new(),

            host_data,
            latest_result: None,

            label_set_name: String::new(),

            column_picker: ColumnPickerOne::new(id.with("column_picker_one")),
            column_picker_open: false,
            current_annotation_file: None,

            id,
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        app_msg_tx: &Sender<AppMsg>,
        // graph: &GraphQueryWorker,
        graph: &GraphQuery,
        open: &mut bool,
        file_name: &str,
        path_id: PathId,
        records: Arc<C>,
        filtered_records: &[usize],
    ) -> Option<egui::InnerResponse<Option<()>>> {
        if let Some(result) = self.host_data.take() {
            if result.is_ok() {
                self.overlay_name.clear();
            }
            self.latest_result = Some(result);
        }

        if Some(path_id) != self.path_id {
            let path_name = graph.graph().get_path_name_vec(path_id).unwrap();
            let path_name = path_name.to_str().unwrap().to_string();
            self.path_id = Some(path_id);
            self.path_name = path_name;
        }

        if self.current_annotation_file.as_ref().map(|s| s.as_str())
            != Some(file_name)
        {
            self.current_annotation_file = Some(file_name.to_string());
            self.column_picker.update_columns(records.as_ref());
        }

        {
            let column_picker_open = &mut self.column_picker_open;

            self.column_picker.ui(ctx, column_picker_open, "Columns");
        }

        let label = {
            let column_picker = &self.column_picker;
            let column = column_picker.chosen_column();

            if let Some(column) = column {
                format!("Use column {}", column)
            } else {
                format!("Choose column")
            }
        };

        let is_running = matches!(
            self.latest_result,
            Some(Err(OverlayFeedback::Running(_)))
        );
        // log::warn!("is_running: {}", is_running);

        egui::Window::new("Create Annotation Labels & Overlays")
            .id(self.id)
            .open(open)
            .show(ctx, |ui| {
                ui.label(file_name);

                let column_picker_open = &mut self.column_picker_open;

                let column_picker_btn =
                    { ui.selectable_label(*column_picker_open, label) };

                if column_picker_btn.clicked() {
                    *column_picker_open = !*column_picker_open;
                }

                ui.separator();

                let name = &mut self.overlay_name;

                let mut create_overlay = false;

                ui.horizontal(|ui| {
                    ui.label("Overlay name");
                    ui.separator();
                    let text_edit = ui.text_edit_singleline(name);

                    if text_edit.has_focus()
                        && ui.input().key_pressed(egui::Key::Enter)
                    {
                        create_overlay = true;
                    }
                });

                let column_picker = &self.column_picker;
                let column = column_picker.chosen_column();

                let create_overlay_btn = ui.add_enabled(
                    column.is_some() && !is_running,
                    egui::Button::new("Create overlay"),
                );

                match &self.latest_result {
                    Some(Err(OverlayFeedback::Running(msg))) => {
                        ui.label(msg);
                    }
                    Some(Err(OverlayFeedback::Error(err))) => {
                        ui.label(err);
                    }
                    // Some(Ok(_)) => {
                    //     ui.label("Overlay complete");
                    // }
                    _ => {
                        ui.label("Idle");
                    }
                }

                create_overlay |= create_overlay_btn.clicked();

                if create_overlay && !is_running {
                    log::warn!("Creating overlay!");
                    if let Some(column) = column {
                        log::warn!("with column");
                        let indices = filtered_records
                            .iter()
                            .copied()
                            .collect::<Vec<_>>();

                        let input: OverlayInput<C> = OverlayInput {
                            name: name.to_owned(),
                            column: column.to_owned(),
                            indices: indices.clone(),
                            path: path_id,
                            records: records.clone(),
                        };

                        self.host_data.call(input).unwrap();
                    }
                }

                ui.separator();

                let mut create_label_set = false;

                {
                    let name = &mut self.label_set_name;

                    ui.horizontal(|ui| {
                        ui.label("Label set name");
                        ui.separator();
                        let text_edit = ui.text_edit_singleline(name);

                        if text_edit.has_focus()
                            && ui.input().key_pressed(egui::Key::Enter)
                        {
                            create_label_set = true;
                        }
                    });
                }

                let column_picker = &self.column_picker;
                let column = column_picker.chosen_column();

                let create_label_set_btn = ui.add_enabled(
                    column.is_some(),
                    egui::Button::new("Create label set"),
                );

                create_label_set |= create_label_set_btn.clicked();

                if create_label_set {
                    if let Some(label_set) = calculate_annotation_set(
                        graph,
                        records.as_ref(),
                        filtered_records,
                        path_id,
                        &self.path_name,
                        column.unwrap(),
                        &self.label_set_name,
                    ) {
                        let name = std::mem::take(&mut self.label_set_name);

                        app_msg_tx
                            .send(AppMsg::NewNodeLabels { name, label_set })
                            .unwrap();
                    }
                }
            })
    }
}

pub(crate) fn calculate_annotation_set<C>(
    graph: &GraphQuery,
    records: &C,
    record_indices: &[usize],
    path_id: PathId,
    path_name: &str,
    column: &C::ColumnKey,
    label_set_name: &str,
) -> Option<AnnotationLabelSet>
where
    C: AnnotationCollection + Send + Sync + 'static,
{
    log::warn!("checking record_indices.is_empty");
    if record_indices.is_empty() {
        return None;
    }

    let offset = crate::annotations::path_name_offset(path_name.as_bytes());

    log::warn!("getting path steps");
    let steps = graph.path_pos_steps(path_id)?;

    let mut label_strings: Vec<String> =
        Vec::with_capacity(record_indices.len());
    let mut label_indices: FxHashMap<NodeId, Vec<usize>> = FxHashMap::default();

    for &record_ix in record_indices.iter() {
        log::trace!("getting record");
        let record = records.records().get(record_ix)?;

        if let Some(range) = crate::annotations::path_step_range(
            &steps,
            offset,
            record.start(),
            record.end(),
        ) {
            if let Some(value) = record.get_first(column) {
                if let Some((mid, _, _)) = range.get(range.len() / 2) {
                    let index = label_strings.len();
                    let label = format!("{}", value.as_bstr());
                    label_strings.push(label);
                    label_indices.entry(mid.id()).or_default().push(index);
                }
            }
        }
    }

    for labels in label_indices.values_mut() {
        labels.sort();
        labels.dedup();
        labels.shrink_to_fit();
    }

    label_strings.shrink_to_fit();
    label_indices.shrink_to_fit();

    Some(AnnotationLabelSet::new(
        records,
        path_id,
        path_name.as_bytes(),
        column,
        label_set_name,
        label_strings,
        label_indices,
    ))
}
