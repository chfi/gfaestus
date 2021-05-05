use std::{path::PathBuf, sync::Arc};

use crossbeam::atomic::AtomicCell;
use crossbeam::channel::{Receiver, Sender};

use bstr::{BStr, ByteSlice};

use std::collections::HashMap;

use anyhow::Result;

use crate::{app::OverlayState, gluon::GraphHandle, graph_query::GraphQuery};

use crate::gluon::GluonVM;

pub struct OverlayList {
    overlay_state: OverlayState,

    overlay_names: HashMap<usize, String>,
}

#[derive(Debug, Clone)]
pub enum OverlayListMsg {
    InsertOverlay { overlay_id: usize, name: String },
    RemoveOverlay { overlay_id: usize },
}

pub struct OverlayCreator {
    name: String,
    script_path_input: String,
    script_path: PathBuf,

    script_error: String,

    gluon: GluonVM,

    new_overlay_tx: Sender<OverlayCreatorMsg>,
    new_overlay_rx: Receiver<OverlayCreatorMsg>,
}

impl OverlayCreator {
    const ID: &'static str = "overlay_creator_window";

    pub fn new() -> Result<Self> {
        let (new_overlay_tx, new_overlay_rx) = crossbeam::channel::unbounded::<OverlayCreatorMsg>();

        let gluonvm = crate::gluon::GluonVM::new()?;

        Ok(Self {
            name: String::new(),
            script_path_input: String::new(),

            script_path: PathBuf::new(),
            script_error: String::new(),

            gluon: gluonvm,

            new_overlay_tx,
            new_overlay_rx,
        })
    }

    pub fn new_overlay_rx(&self) -> &Receiver<OverlayCreatorMsg> {
        &self.new_overlay_rx
    }

    pub fn ui(&mut self, graph: &GraphHandle, ctx: &egui::CtxRef) -> Option<egui::Response> {
        egui::Window::new("Create Overlay")
            .id(egui::Id::new(Self::ID))
            .show(ctx, |ui| {
                let name = &mut self.name;

                let name_box = ui.horizontal(|ui| {
                    ui.label("Overlay name");
                    ui.separator();
                    ui.text_edit_singleline(name)
                });

                let mut path_str = &mut self.script_path_input;

                let path_box = ui.horizontal(|ui| {
                    ui.label("Script path");
                    ui.separator();
                    ui.text_edit_singleline(&mut path_str)
                });

                let run_script = ui.button("Load and execute");

                if run_script.clicked() {
                    let path = PathBuf::from(path_str.as_str());
                    println!("loading gluon script from path {:?}", path.to_str());

                    let result = self.gluon.load_overlay_per_node_expr(graph, &path);

                    match result {
                        Ok(colors) => {
                            let msg = OverlayCreatorMsg::NewOverlay {
                                name: name.to_owned(),
                                colors,
                            };
                            self.new_overlay_tx.send(msg).unwrap();
                        }
                        Err(err) => {
                            eprintln!("Script error:\n{:?}", err);
                        }
                    }
                }
            })
    }
}

pub enum OverlayCreatorMsg {
    NewOverlay {
        name: String,
        colors: Vec<rgb::RGB<f32>>,
    },
}
