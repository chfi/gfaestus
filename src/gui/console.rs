use std::{collections::HashMap, path::PathBuf, sync::Arc};

use clipboard::{ClipboardContext, ClipboardProvider};

#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use anyhow::Result;

use log::debug;

use crossbeam::atomic::AtomicCell;
use rustc_hash::FxHashMap;

use rhai::plugin::*;

use crate::overlays::OverlayKind;
use crate::{app::AppSettings, graph_query::GraphQuery};
use crate::{app::OverlayState, geometry::*};

pub struct Console<'a> {
    input_line: String,

    input_history: Vec<String>,
    output_history: Vec<String>,

    scope: rhai::Scope<'a>,

    request_focus: bool,

    settings: AppSettings,

    get_set: Arc<GetSetTruth>,
}

impl<'a> Console<'a> {
    pub const ID: &'static str = "quake_console";
    pub const ID_TEXT: &'static str = "quake_console_input";

    pub fn new(settings: AppSettings) -> Self {
        let scope = rhai::Scope::new();

        let mut get_set = GetSetTruth::default();

        get_set.add_arc_atomic_cell_get_set(
            "label_radius",
            settings.label_radius().clone(),
            |rad| rad.into(),
            |rad| rad.as_float().unwrap() as f32,
        );

        Self {
            input_line: String::new(),

            input_history: Vec::new(),
            output_history: Vec::new(),

            scope,

            request_focus: false,
            settings,

            get_set: Arc::new(get_set),
        }
    }

    fn create_engine(&self) -> rhai::Engine {
        use rhai::plugin::*;

        let mut engine = rhai::Engine::new();

        engine.register_type::<NodeId>();
        engine.register_type::<Handle>();

        let rad1 = self.settings.label_radius().clone();
        let rad2 = self.settings.label_radius().clone();

        engine.register_fn("get_label_radius", move || rad1.load());

        engine.register_fn("set_label_radius", move |x: f32| {
            rad2.store(x);
        });

        let get_set = self.get_set.clone();

        engine.register_fn("get", move |name: &str| {
            let getter = get_set.getters.get(name).unwrap();
            getter()
        });

        let get_set = self.get_set.clone();

        engine.register_fn("set", move |name: &str, val: rhai::Dynamic| {
            let setter = get_set.setters.get(name).unwrap();
            setter(val);
        });

        let handle = exported_module!(crate::script::plugins::handle_plugin);

        engine.register_global_module(handle.into());

        engine.register_fn("print_test", || {
            println!("hello world");
        });

        engine
    }

    pub fn eval(&mut self) -> Result<()> {
        let engine = self.create_engine();

        debug!("evaluating: {}", &self.input_line);

        let result = engine.eval_with_scope::<rhai::Dynamic>(
            &mut self.scope,
            &self.input_line,
        );
        match result {
            Ok(result) => {
                debug!("Eval success!");
                self.output_history.push(format!("{:?}", result));
            }
            Err(err) => {
                debug!("Eval error: {:?}", err);
                self.output_history.push(format!("Error: {:?}", err));
            }
        }

        Ok(())
    }

    pub fn ui(&mut self, ctx: &egui::CtxRef, is_down: bool) {
        if !is_down {
            return;
        }

        egui::Area::new(Self::ID)
            .enabled(is_down)
            .anchor(egui::Align2::CENTER_TOP, Point::new(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(ctx.input().screen_rect().width());

                let skip_count =
                    self.output_history.len().checked_sub(20).unwrap_or(0);

                for (ix, output_line) in self
                    .output_history
                    .iter()
                    .skip(skip_count)
                    .enumerate()
                    .take(20)
                {
                    ui.add(egui::Label::new(output_line).code());
                }

                let input = ui.add_sized(
                    ui.available_size(),
                    egui::TextEdit::singleline(&mut self.input_line)
                        .id(egui::Id::new(Self::ID_TEXT))
                        .code_editor()
                        .lock_focus(true),
                );

                // hack to keep input
                if self.request_focus {
                    if input.has_focus() {
                        self.request_focus = false;
                    } else {
                        input.request_focus();
                    }
                }

                if input.lost_focus()
                    && ui.input().key_pressed(egui::Key::Enter)
                {
                    self.input_history.push(self.input_line.clone());
                    self.output_history.push(format!("> {}", self.input_line));

                    self.eval().unwrap();

                    let mut line =
                        String::with_capacity(self.input_line.capacity());
                    std::mem::swap(&mut self.input_line, &mut line);

                    self.input_line.clear();

                    // input.request_focus() has to be called the
                    // frame *after* this piece of code is ran, hence
                    // the bool etc.
                    // input.request_focus();
                    self.request_focus = true;
                }
            });
        // });
    }
}

// #[export_module]
// pub mod console_plugin {
//     pub fn get_edge_width(

// }

#[derive(Default)]
pub struct GetSetTruth {
    getters:
        HashMap<String, Box<dyn Fn() -> rhai::Dynamic + Send + Sync + 'static>>,
    setters:
        HashMap<String, Box<dyn Fn(rhai::Dynamic) + Send + Sync + 'static>>,
}

impl GetSetTruth {
    pub fn add_arc_atomic_cell_get_set<T>(
        &mut self,
        name: &str,
        arc: Arc<AtomicCell<T>>,
        to_dyn: impl Fn(T) -> rhai::Dynamic + Send + Sync + 'static,
        from_dyn: impl Fn(rhai::Dynamic) -> T + Send + Sync + 'static,
    ) where
        T: Copy + Send + Sync + 'static,
    {
        let arc_ = arc.clone();
        let getter = move || {
            let t = arc_.load();
            to_dyn(t)
        };

        let setter = move |v: rhai::Dynamic| {
            let v = from_dyn(v);
            arc.store(v);
        };

        self.getters.insert(name.to_string(), Box::new(getter) as _);
        self.setters.insert(name.to_string(), Box::new(setter) as _);
    }
}
