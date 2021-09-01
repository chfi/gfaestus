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

use bstr::ByteSlice;

use crate::{
    app::{AppChannels, OverlayState},
    geometry::*,
};
use crate::{
    app::{AppSettings, SharedState},
    graph_query::GraphQuery,
};
use crate::{overlays::OverlayKind, vulkan::draw_system::edges::EdgesUBO};

pub struct Console<'a> {
    input_line: String,

    input_history_ix: Option<usize>,

    input_history: Vec<String>,
    output_history: Vec<String>,

    scope: rhai::Scope<'a>,

    request_focus: bool,

    settings: AppSettings,
    shared_state: SharedState,
    channels: AppChannels,

    get_set: Arc<GetSetTruth>,
}

impl<'a> Console<'a> {
    pub const ID: &'static str = "quake_console";
    pub const ID_TEXT: &'static str = "quake_console_input";

    pub fn new(
        channels: AppChannels,
        settings: AppSettings,
        shared_state: SharedState,
    ) -> Self {
        let scope = rhai::Scope::new();

        let mut get_set = GetSetTruth::default();

        macro_rules! add_t {
            ($type:ty, $name:literal, $arc:expr) => {
                get_set.add_arc_atomic_cell_get_set(
                    $name,
                    $arc,
                    |x| rhai::Dynamic::from(x),
                    |x: rhai::Dynamic| x.try_cast::<$type>(),
                );
            };
        }

        macro_rules! add_nested_t {
            ($into:expr, $from:expr, $ubo:expr, $name:tt, $field:tt) => {
                get_set.add_arc_atomic_cell_get_set($name, $ubo, $into, $from);
            };
        }

        macro_rules! add_nested_cast {
            ($ubo:expr, $field:tt, $type:ty) => {{
                let name = stringify!($field);

                get_set.add_arc_atomic_cell_get_set(
                    name,
                    $ubo,
                    move |cont| rhai::Dynamic::from(cont.$field),
                    {
                        let ubo = $ubo.clone();
                        move |val: rhai::Dynamic| {
                            let x = val.try_cast::<$type>()?;
                            let mut ubo = ubo.load();
                            ubo.$field = x;
                            Some(ubo)
                        }
                    },
                );
            }};
        }

        macro_rules! add_nested_cell {
            ($obj:expr, $get:tt, $set:tt) => {
                let nw = $obj.clone();
                let nw_ = $obj.clone();

                get_set.add_dynamic(
                    stringify!($get),
                    move || nw.$get(),
                    move |v| {
                        nw_.$set(v);
                    },
                )
            };
        }

        add_t!(f32, "label_radius", settings.label_radius().clone());
        add_t!(Point, "mouse_pos", shared_state.mouse_pos.clone());

        add_t!(
            rgb::RGB<f32>,
            "background_color_light",
            settings.background_color_light().clone()
        );
        add_t!(
            rgb::RGB<f32>,
            "background_color_dark",
            settings.background_color_dark().clone()
        );

        let edge = settings.edge_renderer().clone();

        add_nested_cast!(edge.clone(), edge_color, rgb::RGB<f32>);
        add_nested_cast!(edge.clone(), edge_width, f32);
        add_nested_cast!(edge.clone(), curve_offset, f32);

        let e1 = edge.clone();
        let e2 = edge.clone();

        get_set.add_dynamic(
            "tess_levels",
            move || {
                let tl = e1.load().tess_levels;
                let get = |ix| rhai::Dynamic::from(tl[ix]);
                vec![get(0), get(1), get(2), get(3), get(4)]
            },
            move |tess_vec: Vec<rhai::Dynamic>| {
                let get = |ix| {
                    tess_vec
                        .get(ix)
                        .cloned()
                        .and_then(|v: rhai::Dynamic| v.try_cast())
                        .unwrap_or(0.0f32)
                };
                let arr = [get(0), get(1), get(2), get(3), get(4)];
                let mut ubo = e2.load();
                ubo.tess_levels = arr;
                e2.store(ubo);
            },
        );

        add_nested_cell!(
            settings.node_width().clone(),
            min_node_width,
            set_min_node_width
        );
        add_nested_cell!(
            settings.node_width().clone(),
            max_node_width,
            set_max_node_width
        );
        add_nested_cell!(
            settings.node_width().clone(),
            min_node_scale,
            set_min_node_scale
        );
        add_nested_cell!(
            settings.node_width().clone(),
            max_node_scale,
            set_max_node_scale
        );

        Self {
            input_line: String::new(),

            input_history_ix: None,

            input_history: Vec::new(),
            output_history: Vec::new(),

            scope,

            request_focus: false,

            channels,
            settings,
            shared_state,

            get_set: Arc::new(get_set),
        }
    }

    fn create_engine(&self) -> rhai::Engine {
        use rhai::plugin::*;

        let mut engine = rhai::Engine::new();

        let colors = exported_module!(crate::script::plugins::colors);

        engine.register_type::<NodeId>();
        engine.register_type::<Handle>();
        engine.register_type::<Point>();

        engine.register_global_module(colors.into());

        let get_set = self.get_set.clone();

        engine.register_fn("Point", |x: f32, y: f32| Point::new(x, y));
        engine.register_fn("x", |point: &mut Point| point.x);
        engine.register_fn("y", |point: &mut Point| point.y);

        {
            let arc = self.shared_state.hover_node.clone();
            engine.register_fn("get_hover_node", move || arc.load());

            let arc = self.shared_state.hover_node.clone();
            engine.register_fn("get_hover_node", move || arc.load());
        }

        let app_msg_tx = self.channels.app_tx.clone();
        engine.register_fn("toggle_dark_mode", move || {
            app_msg_tx.send(crate::app::AppMsg::ToggleDarkMode).unwrap();
        });
        let app_msg_tx = self.channels.app_tx.clone();
        engine.register_fn("toggle_overlay", move || {
            app_msg_tx.send(crate::app::AppMsg::ToggleOverlay).unwrap();
        });

        engine.register_fn("get", move |name: &str| {
            if let Some(getter) = get_set.getters.get(name) {
                getter()
            } else {
                rhai::Dynamic::FALSE
            }
        });

        let get_set = self.get_set.clone();

        engine.register_fn("set", move |name: &str, val: rhai::Dynamic| {
            if let Some(setter) = get_set.setters.get(name) {
                setter(val);
            }
        });

        let handle = exported_module!(crate::script::plugins::handle_plugin);

        engine.register_global_module(handle.into());

        engine.register_fn("print_test", || {
            println!("hello world");
        });

        engine
    }

    pub fn eval_file(&mut self, print: bool, path: &str) -> Result<()> {
        use std::io::prelude::*;
        let mut file = std::fs::File::open(path)?;
        let mut script = String::new();
        let _count = file.read_to_string(&mut script)?;

        if print {
            self.output_history
                .push(format!(">>> Evaluating file '{}'", path));
        }

        self.eval_line(print, &script)
    }

    pub fn eval_line(&mut self, print: bool, input_line: &str) -> Result<()> {
        let mut old_input = input_line.to_string();
        std::mem::swap(&mut old_input, &mut self.input_line);

        self.eval(print)?;
        std::mem::swap(&mut old_input, &mut self.input_line);

        Ok(())
    }

    // fn eval_file_interval(&mut self, print: bool, handle_name: &str, path: &str) -> Result<()> {

    // }

    pub fn eval(&mut self, print: bool) -> Result<()> {
        let engine = self.create_engine();

        debug!("evaluating: {}", &self.input_line);

        if self.input_line.starts_with(":clear") {
            self.input_line.clear();
            self.output_history.clear();

            return Ok(());
        } else if self.input_line.starts_with(":exec ") {
            let file_path = &self.input_line[6..].to_string();
            let result = self.eval_file(print, &file_path);

            if let Err(err) = result {
                debug!(
                    "console :exec of file '{}' failed: {:?}",
                    file_path, err
                );
            }
            self.input_line.clear();

            return Ok(());
        }

        let result = engine.eval_with_scope::<rhai::Dynamic>(
            &mut self.scope,
            &self.input_line,
        );
        match result {
            Ok(result) => {
                debug!("Eval success!");
                if print {
                    if let Some(color) =
                        result.clone().try_cast::<rgb::RGB<f32>>()
                    {
                        self.output_history.push(format!("{}", color))
                    } else if let Some(color) =
                        result.clone().try_cast::<rgb::RGBA<f32>>()
                    {
                        self.output_history.push(format!("{}", color));
                    } else {
                        self.output_history.push(format!("{:?}", result));
                    }
                }
            }
            Err(err) => {
                debug!("Eval error: {:?}", err);
                if print {
                    self.output_history.push(format!("Error: {:?}", err));
                }
            }
        }

        Ok(())
    }

    pub fn ui(&mut self, ctx: &egui::CtxRef, is_down: bool) {
        if !is_down {
            return;
        }

        egui::Window::new(Self::ID)
            .resizable(false)
            .auto_sized()
            .title_bar(false)
            .collapsible(false)
            .enabled(is_down)
            .anchor(egui::Align2::CENTER_TOP, Point::new(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(ctx.input().screen_rect().width());

                let skip_count =
                    self.output_history.len().checked_sub(20).unwrap_or(0);

                for (_ix, output_line) in self
                    .output_history
                    .iter()
                    .skip(skip_count)
                    .enumerate()
                    .take(20)
                {
                    let label = egui::Label::new(output_line).monospace();
                    ui.add(label);
                }

                let input = ui.add(
                    egui::TextEdit::singleline(&mut self.input_line)
                        .id(egui::Id::new(Self::ID_TEXT))
                        .code_editor()
                        .lock_focus(true)
                        .desired_width(ui.available_width()),
                );

                // hack to keep input
                if self.request_focus {
                    if input.has_focus() {
                        self.request_focus = false;
                    }
                    input.request_focus();
                }

                if ui.input().key_pressed(egui::Key::ArrowUp) {
                    self.step_history(true);
                }

                if ui.input().key_pressed(egui::Key::ArrowDown) {
                    self.step_history(false);
                }

                if input.lost_focus()
                    && ui.input().key_pressed(egui::Key::Enter)
                {
                    self.input_history.push(self.input_line.clone());
                    self.output_history.push(format!("> {}", self.input_line));

                    self.eval(true).unwrap();

                    let mut line =
                        String::with_capacity(self.input_line.capacity());
                    std::mem::swap(&mut self.input_line, &mut line);

                    self.input_line.clear();

                    self.input_history_ix.take();

                    // input.request_focus() has to be called the
                    // frame *after* this piece of code is ran, hence
                    // the bool etc.
                    // input.request_focus();
                    self.request_focus = true;
                }
            });
    }

    fn step_history(&mut self, backward: bool) {
        if self.input_history.is_empty() {
            return;
        }

        if let Some(ix) = self.input_history_ix.as_mut() {
            #[rustfmt::skip]
            let ix = (backward && *ix > 0)
                      .then(|| *ix -= 1)
                .or((!backward && *ix < self.input_history.len())
                      .then(|| *ix += 1))
                .map(|_| *ix);

            let input_history = &self.input_history;
            if let Some(ix) = ix.and_then(|ix| input_history.get(ix)) {
                self.input_line.clone_from(ix);
            } else {
                self.input_line.clear();
                self.input_history_ix = None;
            }
        } else {
            let ix = backward
                .then(|| self.input_history.len().checked_sub(1))
                .flatten()
                .unwrap_or(0);

            self.input_history_ix = Some(ix);

            if let Some(line) = self.input_history.get(ix) {
                self.input_line.clone_from(line);
            }
        }
    }
}

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
        from_dyn: impl Fn(rhai::Dynamic) -> Option<T> + Send + Sync + 'static,
    ) where
        T: Copy + Send + Sync + 'static,
    {
        let arc_ = arc.clone();
        let getter = move || {
            let t = arc_.load();
            to_dyn(t)
        };

        let setter = move |v: rhai::Dynamic| {
            if let Some(v) = from_dyn(v) {
                arc.store(v);
            }
        };

        self.getters.insert(name.to_string(), Box::new(getter) as _);
        self.setters.insert(name.to_string(), Box::new(setter) as _);
    }

    pub fn add_dynamic<T>(
        &mut self,
        name: &str,
        get: impl Fn() -> T + Send + Sync + 'static,
        set: impl Fn(T) + Send + Sync + 'static,
    ) where
        T: Clone + Send + Sync + 'static,
    {
        let getter = move || {
            let v = get();
            rhai::Dynamic::from(v)
        };

        let setter = move |val: rhai::Dynamic| {
            let val: T = val.cast();
            set(val);
        };

        self.getters.insert(name.to_string(), Box::new(getter) as _);
        self.setters.insert(name.to_string(), Box::new(setter) as _);
    }
}
