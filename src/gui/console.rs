use std::{collections::HashMap, path::PathBuf, sync::Arc};

use clipboard::{ClipboardContext, ClipboardProvider};

use futures::future::RemoteHandle;
#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};
use handlegraph::{packedgraph::PackedGraph, path_position::PathPositionMap};

use anyhow::Result;

use log::debug;

use crossbeam::atomic::AtomicCell;
use rustc_hash::FxHashMap;

use rhai::{plugin::*, Func};

use bstr::ByteSlice;

use crate::{
    app::{
        selection::NodeSelection, AppChannels, AppMsg, OverlayState, Select,
    },
    geometry::*,
    reactor::Reactor,
};
use crate::{
    app::{AppSettings, SharedState},
    graph_query::GraphQuery,
};
use crate::{overlays::OverlayKind, vulkan::draw_system::edges::EdgesUBO};

use parking_lot::{Mutex, MutexGuard};

pub type ScriptEvalResult =
    std::result::Result<rhai::Dynamic, Box<rhai::EvalAltResult>>;

pub struct Console<'a> {
    input_line: String,

    input_history_ix: Option<usize>,

    input_history: Vec<String>,
    output_history: Vec<String>,

    scope: Arc<Mutex<rhai::Scope<'a>>>,

    request_focus: bool,

    settings: AppSettings,
    shared_state: SharedState,
    channels: AppChannels,

    get_set: Arc<GetSetTruth>,

    remote_handles: HashMap<String, RemoteHandle<()>>,

    result_rx: crossbeam::channel::Receiver<ScriptEvalResult>,
    result_tx: crossbeam::channel::Sender<ScriptEvalResult>,

    graph: Arc<PackedGraph>,
    path_positions: Arc<PathPositionMap>,
}

impl Console<'static> {
    pub const ID: &'static str = "quake_console";
    pub const ID_TEXT: &'static str = "quake_console_input";

    pub fn new(
        graph: &GraphQuery,
        channels: AppChannels,
        settings: AppSettings,
        shared_state: SharedState,
    ) -> Self {
        let (result_tx, result_rx) =
            crossbeam::channel::unbounded::<ScriptEvalResult>();

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

        let scope = rhai::Scope::new();
        let scope = Arc::new(Mutex::new(scope));

        let output_history =
            vec![" < close this console with Esc >".to_string()];

        Self {
            input_line: String::new(),

            input_history_ix: None,

            input_history: Vec::new(),
            output_history,

            scope,

            request_focus: false,

            channels,
            settings,
            shared_state,

            get_set: Arc::new(get_set),

            remote_handles: Default::default(),

            result_tx,
            result_rx,

            graph: graph.graph.clone(),
            path_positions: graph.path_positions.clone(),
        }
    }

    fn create_engine(&self) -> rhai::Engine {
        use rhai::plugin::*;

        // let mut engine = rhai::Engine::new();

        let mut engine = crate::script::create_engine();

        engine.register_type::<Point>();

        let get_set = self.get_set.clone();

        let graph = self.graph.clone();
        let path_pos = self.path_positions.clone();
        engine.register_fn("get_graph", move || graph.clone());
        engine.register_fn("get_path_positions", move || path_pos.clone());

        let app_msg_tx = self.channels.app_tx.clone();
        engine.register_fn("set_selection", move |selection: NodeSelection| {
            let msg = AppMsg::Selection(Select::Many {
                nodes: selection.nodes,
                clear: true,
            });
            app_msg_tx.send(msg).unwrap();
        });

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

        engine.register_fn("test_wait", || {
            println!("sleeping 2 seconds...");
            std::thread::sleep(std::time::Duration::from_millis(2000));
            println!("waking up!");
        });

        engine.register_global_module(handle.into());

        engine.register_fn("print_test", || {
            println!("hello world");
        });

        engine
    }

    pub fn eval_file(
        &mut self,
        reactor: &mut Reactor,
        print: bool,
        path: &str,
    ) -> Result<()> {
        use std::io::prelude::*;
        let mut file = std::fs::File::open(path)?;
        let mut script = String::new();
        let _count = file.read_to_string(&mut script)?;

        if print {
            self.output_history
                .push(format!(">>> Evaluating file '{}'", path));
        }

        self.eval_line(reactor, print, &script)
    }

    pub fn eval_line(
        &mut self,
        reactor: &mut Reactor,
        print: bool,
        input_line: &str,
    ) -> Result<()> {
        let mut old_input = input_line.to_string();
        std::mem::swap(&mut old_input, &mut self.input_line);

        self.eval(reactor, print)?;
        std::mem::swap(&mut old_input, &mut self.input_line);

        Ok(())
    }

    fn eval_file_interval(
        &mut self,
        reactor: &mut Reactor,
        handle_name: &str,
        path: &str,
    ) -> Result<()> {
        let handle_name = handle_name.to_string();

        let engine = self.create_engine();

        let start = std::time::Instant::now();

        let path = PathBuf::from(path);
        let ast = engine.compile_file(path)?;

        let mut scope = {
            let scope_lock = self.scope.lock();
            let scope = scope_lock.to_owned();
            scope
        };

        let handle = reactor.spawn_interval(
            move || {
                scope.set_value(
                    "time_since_start",
                    start.elapsed().as_secs_f32(),
                );

                let _result: std::result::Result<(), _> =
                    engine.eval_ast_with_scope(&mut scope, &ast);
            },
            std::time::Duration::from_millis(30),
        )?;

        self.remote_handles.insert(handle_name, handle);

        Ok(())
    }

    fn stop_interval(&mut self, handle_name: &str) {
        self.remote_handles.remove(handle_name);
    }

    fn exec_console_command(&mut self, reactor: &mut Reactor) -> Result<bool> {
        if self.input_line.starts_with(":clear") {
            self.input_line.clear();
            self.output_history.clear();

            return Ok(true);
        } else if self.input_line.starts_with(":reset") {
            self.scope = Arc::new(Mutex::new(rhai::Scope::new()));
            self.input_line.clear();
            self.input_history.clear();
            self.output_history.clear();

            return Ok(true);
        } else if self.input_line.starts_with(":exec ") {
            let file_path = &self.input_line[6..].to_string();
            let result = self.eval_file(reactor, true, &file_path);

            if let Err(err) = result {
                debug!(
                    "console :exec of file '{}' failed: {:?}",
                    file_path, err
                );
            }
            self.input_line.clear();

            return Ok(true);
        } else if self.input_line.starts_with(":start_interval ") {
            let mut fields = self.input_line.split_ascii_whitespace();

            fields.next();
            let file_name = fields.next();
            let handle_name = fields.next();

            if let (Some(file), Some(handle)) = (file_name, handle_name) {
                let file = file.to_string();
                let handle = handle.to_string();
                self.eval_file_interval(reactor, &handle, &file)?;
            }

            return Ok(true);
        } else if self.input_line.starts_with(":end_interval ") {
            let handle = &self.input_line[":end_interval ".len()..].to_string();
            self.stop_interval(&handle);

            return Ok(true);
        }

        Ok(false)
    }

    pub fn eval_input(
        &mut self,
        reactor: &mut Reactor,
        print: bool,
    ) -> Result<()> {
        debug!("evaluating: {}", &self.input_line);

        let executed_command = self.exec_console_command(reactor)?;
        if executed_command {
            return Ok(());
        }
        self.eval(reactor, print)?;

        Ok(())
    }

    fn handle_eval_result(
        &mut self,
        print: bool,
        result: std::result::Result<rhai::Dynamic, Box<rhai::EvalAltResult>>,
    ) -> Result<()> {
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

    pub fn eval(&mut self, reactor: &mut Reactor, print: bool) -> Result<()> {
        debug!("evaluating: {}", &self.input_line);
        let engine = self.create_engine();

        let result_tx = self.result_tx.clone();

        let input = self.input_line.to_string();

        let scope = self.scope.clone();

        let handle = reactor.spawn(async move {
            let mut scope = scope.lock();
            let result =
                engine.eval_with_scope::<rhai::Dynamic>(&mut scope, &input);
            let _ = result_tx.send(result);
        })?;

        handle.forget();

        Ok(())
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        is_down: bool,
        reactor: &mut Reactor,
    ) {
        if !is_down {
            return;
        }

        while let Ok(result) = self.result_rx.try_recv() {
            self.handle_eval_result(true, result).unwrap();
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

                let scope_locked = self.scope.is_locked();

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

                let input = {
                    let line_count = self.input_line.lines().count().max(1);
                    ui.add(
                        // egui::TextEdit::singleline(&mut self.input_line)
                        egui::TextEdit::multiline(&mut self.input_line)
                            .id(egui::Id::new(Self::ID_TEXT))
                            .desired_rows(line_count)
                            .code_editor()
                            .lock_focus(true)
                            .enabled(!scope_locked)
                            .desired_width(ui.available_width()),
                    )
                };

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

                // if input.lost_focus()
                if ui.input().key_pressed(egui::Key::Enter) && !scope_locked {
                    log::warn!("input line: {}", self.input_line);

                    if ui.input().modifiers.shift {
                        // insert newline;
                        // self.input_line.push_str("\n");
                    } else {
                        // evaluate input

                        // remove the last endline added by pressing
                        // enter in a multiline text box
                        self.input_line.pop();

                        self.input_history.push(self.input_line.clone());
                        self.output_history
                            .push(format!("> {}", self.input_line));

                        self.eval_input(reactor, true).unwrap();

                        let mut line =
                            String::with_capacity(self.input_line.capacity());
                        std::mem::swap(&mut self.input_line, &mut line);

                        self.input_line.clear();

                        self.input_history_ix.take();
                    }

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
