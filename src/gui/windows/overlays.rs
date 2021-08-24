use std::io::Read;
use std::{path::PathBuf, sync::Arc};

use crossbeam::atomic::AtomicCell;

use rhai::EvalAltResult;
use rustc_hash::FxHashMap;

use anyhow::Result;

use crate::reactor::{Host, Outbox, Reactor};
use crate::script::{ScriptConfig, ScriptTarget};
use crate::{
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

#[derive(Debug, Clone)]
pub struct ScriptInput {
    name: String,
    path: PathBuf,

    config: ScriptConfig,
}

pub enum ScriptMsg {
    IOError(String),
    ScriptError(String),
    Running(String),
}

impl ScriptMsg {
    fn io_error(err: &str) -> Self {
        ScriptMsg::IOError(err.to_string())
    }

    fn script_error(err: &str) -> Self {
        ScriptMsg::ScriptError(err.to_string())
    }

    fn running(msg: &str) -> Self {
        ScriptMsg::Running(msg.to_string())
    }
}

pub type ScriptResult = Result<(), ScriptMsg>;

pub struct OverlayCreator {
    name: String,
    script_path_input: String,

    file_picker: FilePicker,
    file_picker_open: bool,

    script_results: Host<ScriptInput, ScriptResult>,
    latest_result: Option<ScriptResult>,
}

impl OverlayCreator {
    pub const ID: &'static str = "overlay_creator_window";

    pub fn new(reactor: &mut Reactor) -> Result<Self> {
        let pwd = std::fs::canonicalize("./").unwrap();

        let mut file_picker = FilePicker::new(
            egui::Id::with(egui::Id::new(Self::ID), "file_picker"),
            pwd,
        )
        .unwrap();

        let script_results = {
            let tx = reactor.overlay_create_tx.clone();
            let rayon_pool = reactor.rayon_pool.clone();
            let graph = reactor.graph_query.clone();

            reactor.create_host(
                move |outbox: &Outbox<ScriptResult>, input: ScriptInput| {
                    let running_msg = |msg: &str| {
                        outbox.insert_blocking(Err(ScriptMsg::running(msg)));
                    };

                    running_msg("Loading script");

                    let mut file =
                        std::fs::File::open(input.path).map_err(|_| {
                            ScriptMsg::io_error("error loading script file")
                        })?;

                    let mut script = String::new();
                    file.read_to_string(&mut script).map_err(|_| {
                        ScriptMsg::io_error("error loading script file")
                    })?;

                    running_msg("Evaluating script");
                    let overlay_data = crate::script::overlay_colors_tgt(
                        &rayon_pool,
                        &input.config,
                        &graph,
                        &script,
                    );

                    let feedback = match overlay_data {
                        Ok(data) => {
                            let msg = OverlayCreatorMsg::NewOverlay {
                                name: input.name,
                                data,
                            };
                            tx.send(msg).unwrap();
                            Ok(())
                        }
                        Err(err) => {
                            Err(ScriptMsg::ScriptError(format!("{:?}", err)))
                        }
                    };

                    feedback
                },
            )
        };

        let extensions: [&str; 1] = ["rhai"];
        file_picker.set_visible_extensions(&extensions).unwrap();

        Ok(Self {
            name: String::new(),
            script_path_input: String::new(),

            file_picker,
            file_picker_open: false,

            script_results,
            latest_result: None,
        })
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
    ) -> Option<egui::Response> {
        let scr = ctx.input().screen_rect();

        if let Some(result) = self.script_results.take() {
            if result.is_ok() {
                self.script_path_input.clear();
                self.name.clear();
            }
            self.latest_result = Some(result);
        }

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
                let is_running = matches!(
                    self.latest_result,
                    Some(Err(ScriptMsg::Running(_)))
                );

                let name = &mut self.name;
                let file_picker = &mut self.file_picker;
                let file_picker_open = &mut self.file_picker_open;
                let latest_result = &self.latest_result;

                let script_results = &self.script_results;

                let _name_box = ui.horizontal(|ui| {
                    ui.label("Overlay name");
                    ui.separator();
                    let text_edit =
                        egui::TextEdit::singleline(name).enabled(!is_running);
                    ui.add(text_edit);
                });

                let path_str = &mut self.script_path_input;

                let _path_box = ui.horizontal(|ui| {
                    ui.label("Script path");
                    ui.separator();
                    let text_edit = egui::TextEdit::singleline(path_str)
                        .enabled(!is_running);
                    ui.add(text_edit);
                });

                ui.horizontal(|ui| {
                    let file_btn =
                        egui::Button::new("Choose file").enabled(!is_running);

                    if ui.add(file_btn).clicked() {
                        file_picker.reset_selection();
                        *file_picker_open = true;
                    }

                    ui.separator();

                    let run_script = ui.add(
                        egui::Button::new("Run script").enabled(!is_running),
                    );

                    if run_script.clicked()
                        && (latest_result.is_none()
                            || matches!(latest_result, Some(Err(_))))
                    {
                        file_picker.reset_selection();
                        let path = PathBuf::from(path_str.as_str());

                        let target = ScriptTarget::Nodes;

                        let config = ScriptConfig {
                            default_color: rgb::RGBA::new(0.3, 0.3, 0.3, 0.3),
                            target,
                        };

                        let script_input = ScriptInput {
                            name: name.to_string(),
                            path,
                            config,
                        };

                        script_results.call(script_input).unwrap();
                    }
                });

                match &self.latest_result {
                    Some(Err(ScriptMsg::IOError(err))) => {
                        eprintln!("Overlay script IO error: {:?}", err);
                        ui.label(format!("IO Error: {:?}", err));
                    }
                    Some(Err(ScriptMsg::ScriptError(err))) => {
                        eprintln!("Overlay script execution error: {:?}", err);
                        ui.label(format!("Script Error: {:?}", err));
                    }
                    Some(Err(ScriptMsg::Running(msg))) => {
                        ui.label(msg);
                    }
                    Some(Ok(_)) => {
                        ui.label("Created new overlay");
                    }
                    _ => (),
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
