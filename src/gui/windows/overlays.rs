use std::io::Read;
use std::{path::PathBuf, sync::Arc};

use crossbeam::atomic::AtomicCell;
use crossbeam::channel::{Receiver, Sender};

use rhai::EvalAltResult;
use rustc_hash::FxHashMap;

use anyhow::Result;

use futures::executor::ThreadPool;

use crate::graph_query::GraphQuery;
use crate::script::{ScriptConfig, ScriptTarget};
use crate::{
    asynchronous::AsyncResult,
    geometry::Point,
    vulkan::texture::{GradientName, Gradients},
};

use crate::app::OverlayState;
use crate::overlays::{OverlayData, OverlayKind};

use super::file::FilePicker;

pub struct OverlayList {
    overlay_state: OverlayState,

    overlay_names: FxHashMap<usize, (OverlayKind, String)>,

    gradient_picker: GradientPicker,

    gradient_picker_open: AtomicCell<bool>,
}

impl OverlayList {
    pub const ID: &'static str = "overlay_list_window";

    pub fn new(overlay_state: OverlayState) -> Self {
        let gradient_picker = GradientPicker::new(overlay_state.clone());

        Self {
            overlay_state,
            overlay_names: Default::default(),

            gradient_picker,

            gradient_picker_open: AtomicCell::new(false),
        }
    }

    pub fn populate_names<'a>(
        &mut self,
        names: impl Iterator<Item = (usize, OverlayKind, &'a str)>,
    ) {
        self.overlay_names.clear();
        self.overlay_names
            .extend(names.map(|(x, k, n)| (x, (k, n.to_string()))));
    }

    pub fn ui(
        &self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        open_creator: &mut bool,
    ) -> Option<egui::Response> {
        egui::Window::new("Overlay List")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |mut ui| {
                ui.set_min_width(300.0);

                let use_overlay = self.overlay_state.use_overlay();

                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(use_overlay, "Overlay enabled")
                        .clicked()
                    {
                        self.overlay_state.toggle_overlay();
                    }

                    if ui
                        .selectable_label(*open_creator, "Overlay creator")
                        .clicked()
                    {
                        *open_creator = !*open_creator;
                    }
                });

                let open_gradient_picker = self.gradient_picker_open.load();
                if ui
                    .selectable_label(open_gradient_picker, "Gradients")
                    .clicked()
                {
                    self.gradient_picker_open.store(!open_gradient_picker);
                }

                egui::Grid::new("overlay_list_window_grid").show(
                    &mut ui,
                    |ui| {
                        ui.label("Active overlay");
                        ui.end_row();

                        let mut overlay_names =
                            self.overlay_names.iter().collect::<Vec<_>>();
                        overlay_names.sort_by_key(|(id, _)| *id);

                        let mut current_overlay =
                            self.overlay_state.current_overlay();

                        for (id, (kind, name)) in overlay_names {
                            if ui
                                .radio_value(
                                    &mut current_overlay,
                                    Some((*id, *kind)),
                                    name,
                                )
                                .clicked()
                            {
                                self.overlay_state
                                    .set_current_overlay(current_overlay);
                            }

                            ui.end_row();
                        }
                    },
                );
            })
    }

    pub fn gradient_picker_ui(
        &self,
        ctx: &egui::CtxRef,
    ) -> Option<egui::Response> {
        let mut open = self.gradient_picker_open.load();
        let resp = self.gradient_picker.ui(ctx, &mut open);
        self.gradient_picker_open.store(open);
        resp
    }
}

#[derive(Debug, Clone)]
pub enum OverlayListMsg {
    InsertOverlay { overlay_id: usize, name: String },
    RemoveOverlay { overlay_id: usize },
}

pub struct OverlayCreator {
    name: String,
    script_path_input: String,

    script_error: String,

    new_overlay_tx: Sender<OverlayCreatorMsg>,
    new_overlay_rx: Receiver<OverlayCreatorMsg>,

    file_picker: FilePicker,
    file_picker_open: bool,

    script_query: Option<
        AsyncResult<std::result::Result<OverlayData, Box<EvalAltResult>>>,
    >,
}

impl OverlayCreator {
    pub const ID: &'static str = "overlay_creator_window";

    pub fn new() -> Result<Self> {
        let (new_overlay_tx, new_overlay_rx) =
            crossbeam::channel::unbounded::<OverlayCreatorMsg>();

        let pwd = std::fs::canonicalize("./").unwrap();

        let mut file_picker = FilePicker::new(
            egui::Id::with(egui::Id::new(Self::ID), "file_picker"),
            pwd,
        )
        .unwrap();

        let extensions: [&str; 1] = ["rhai"];
        file_picker.set_visible_extensions(&extensions).unwrap();

        Ok(Self {
            name: String::new(),
            script_path_input: String::new(),

            script_error: String::new(),

            new_overlay_tx,
            new_overlay_rx,

            file_picker,
            file_picker_open: false,

            script_query: None,
        })
    }

    pub fn new_overlay_tx(&self) -> &Sender<OverlayCreatorMsg> {
        &self.new_overlay_tx
    }

    pub fn new_overlay_rx(&self) -> &Receiver<OverlayCreatorMsg> {
        &self.new_overlay_rx
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        graph: Arc<GraphQuery>,
        // graph: &GraphHandle,
        thread_pool: &ThreadPool,
        rayon_pool: Arc<rayon::ThreadPool>,
    ) -> Option<egui::Response> {
        let scr = ctx.input().screen_rect();

        let pos = egui::pos2(scr.center().x - 150.0, scr.center().y - 60.0);

        if self.file_picker.selected_path().is_some() {
            self.file_picker_open = false;
        }

        self.file_picker.ui(ctx, &mut self.file_picker_open);

        if let Some(path) = self.file_picker.selected_path() {
            let path_str = path.to_str().unwrap();
            self.script_path_input = path_str.to_string();
        }

        egui::Window::new("Create Overlay")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .default_pos(pos)
            .show(ctx, |ui| {
                if self.script_query.is_none() {
                    if ui.button("Choose script file").clicked() {
                        self.file_picker.reset_selection();
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
                }

                let name = &mut self.name;

                let _name_box = ui.horizontal(|ui| {
                    ui.label("Overlay name");
                    ui.separator();
                    ui.text_edit_singleline(name)
                });

                let path_str = &mut self.script_path_input;

                let path_box = ui.horizontal(|ui| {
                    ui.label("Script path");
                    ui.separator();
                    ui.text_edit_singleline(path_str)
                });

                let run_script = ui.button("Load and execute");

                let _script_error_msg = ui.label(&self.script_error);

                if run_script.clicked() && self.script_query.is_none() {
                    self.file_picker.reset_selection();
                    let path = PathBuf::from(path_str.as_str());
                    println!(
                        "loading gluon script from path {:?}",
                        path.to_str()
                    );

                    let target = ScriptTarget::Nodes;

                    let config = ScriptConfig {
                        default_color: rgb::RGBA::new(0.3, 0.3, 0.3, 0.3),
                    };

                    dbg!();
                    let query = AsyncResult::new(thread_pool, async move {
                        let mut file = std::fs::File::open(path)
                            .map_err(|_| "error loading script file")?;
                        let mut script = String::new();
                        file.read_to_string(&mut script)
                            .map_err(|_| "error loading script file")?;

                        let overlay_data = crate::script::overlay_colors_tgt(
                            &rayon_pool,
                            config,
                            &target,
                            &graph,
                            &script,
                        );
                        overlay_data
                    });

                    self.script_query = Some(query);
                }

                if let Some(query) = self.script_query.as_mut() {
                    query.move_result_if_ready();
                }

                if let Some(script_result) = self
                    .script_query
                    .as_mut()
                    .and_then(|r| r.take_result_if_ready())
                {
                    match script_result {
                        Ok(data) => {
                            let msg = OverlayCreatorMsg::NewOverlay {
                                name: name.to_owned(),
                                data,
                            };
                            self.script_path_input.clear();
                            self.name.clear();

                            self.script_error = "Success".to_string();

                            self.new_overlay_tx.send(msg).unwrap();
                        }
                        Err(err) => {
                            self.script_error = err.to_string();

                            eprintln!("Script error:\n{:?}", err);
                        }
                    }

                    self.script_query = None;
                }
            })
    }
}

pub enum OverlayCreatorMsg {
    NewOverlay { name: String, data: OverlayData },
}

pub struct GradientPicker {
    overlay_state: OverlayState,
    gradient_names: Vec<(GradientName, String)>,
}

impl GradientPicker {
    pub const ID: &'static str = "gradient_picker_window";

    pub fn new(overlay_state: OverlayState) -> Self {
        let gradient_names =
            std::array::IntoIter::new(Gradients::GRADIENT_NAMES)
                .map(|name| (name, name.to_string()))
                .collect::<Vec<_>>();

        Self {
            overlay_state,
            gradient_names,
        }
    }

    pub fn ui(
        &self,
        ctx: &egui::CtxRef,
        open: &mut bool,
    ) -> Option<egui::Response> {
        egui::Window::new("Gradients")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |ui| {
                egui::ScrollArea::auto_sized().show(ui, |ui| {
                    egui::Grid::new("gradient_picker_list").show(ui, |ui| {
                        ui.label("Name");
                        ui.separator();

                        ui.label("Gradient");
                        ui.end_row();

                        let mut current_gradient =
                            self.overlay_state.gradient();

                        for (gradient_name, name) in self.gradient_names.iter()
                        {
                            let gradient_select = ui.selectable_value(
                                &mut current_gradient,
                                *gradient_name,
                                name,
                            );
                            ui.separator();

                            if gradient_select.clicked() {
                                self.overlay_state.set_gradient(*gradient_name);
                            }

                            ui.image(
                                gradient_name.texture_id(),
                                Point { x: 130.0, y: 15.0 },
                            );
                            ui.end_row();
                        }
                    });
                });
            })
    }
}
