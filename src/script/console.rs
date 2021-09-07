use std::{collections::HashMap, path::PathBuf, sync::Arc};

use futures::{future::RemoteHandle, AsyncBufRead, AsyncWrite};
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

use rhai::{plugin::*, Func};
use rustc_hash::FxHashSet;

use crate::{
    app::{
        selection::NodeSelection, AppChannels, AppMsg, OverlayCreatorMsg,
        OverlayState, Select,
    },
    geometry::*,
    reactor::Reactor,
    script::{overlay_colors_tgt_, ScriptConfig, ScriptTarget},
    view::View,
};
use crate::{
    app::{AppSettings, SharedState},
    graph_query::GraphQuery,
};
use crate::{overlays::OverlayKind, vulkan::draw_system::edges::EdgesUBO};

use parking_lot::Mutex;
use parking_lot::RwLock;

pub type ScriptEvalResult =
    std::result::Result<rhai::Dynamic, Box<rhai::EvalAltResult>>;

#[derive(Clone)]
pub struct ConsoleCore {
    settings: AppSettings,
    shared_state: SharedState,
    channels: AppChannels,
    get_set: Arc<GetSetTruth>,
    graph: Arc<GraphQuery>,

    overlay_list: Arc<Mutex<Vec<(usize, OverlayKind, String)>>>,
    label_map: Arc<Mutex<HashMap<String, (Point, String)>>>,
    key_code_map: Arc<HashMap<String, winit::event::VirtualKeyCode>>,

    // is this a bad idea? i should probably just use a global pool
    rayon_pool: Arc<rayon::ThreadPool>,

    // probably not these? or, no, the result_tx is probably fine
    input_rx: crossbeam::channel::Receiver<String>,
    result_tx: crossbeam::channel::Sender<ScriptEvalResult>,

    modules: Arc<Mutex<Vec<Arc<rhai::Module>>>>,
}

impl ConsoleCore {
    pub fn eval(
        &mut self,
        reactor: &mut Reactor,
        line: &str,
        print: bool,
        engine: Option<rhai::Engine>,
    ) {
        let engine = engine.unwrap_or_else(|| self.create_engine());

        let result_tx = self.result_tx.clone();

        let input = line.to_string();

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

    pub fn create_engine(&self) -> rhai::Engine {
        use rhai::plugin::*;

        let mut engine = crate::script::create_engine();

        // TODO this should be configurable in the app options
        engine.set_max_call_levels(16);
        engine.set_max_expr_depths(0, 0);

        engine.register_type::<Point>();

        let label_map = self.label_map.clone();
        engine.register_fn(
            "add_label",
            move |id: &str, label: &str, at: Point| {
                let mut labels = label_map.lock();
                labels.insert(id.to_string(), (at, label.to_string()));
            },
        );

        let label_map = self.label_map.clone();
        engine.register_fn("remove_label", move |id: &str| {
            let mut labels = label_map.lock();
            labels.remove(id);
        });

        let label_map = self.label_map.clone();
        engine.register_fn("remove_all_labels", move || {
            let mut labels = label_map.lock();
            labels.clear();
        });

        engine.register_fn("Point", |x: f32, y: f32| Point::new(x, y));
        engine.register_get_set(
            "x",
            |p: &mut Point| p.x,
            |p: &mut Point, x| p.x = x,
        );
        engine.register_get_set(
            "y",
            |p: &mut Point| p.y,
            |p: &mut Point, y| p.y = y,
        );

        let result_tx = self.result_tx.clone();
        engine.register_fn("log", move |v: rhai::Dynamic| {
            result_tx.send(Ok(v)).unwrap();
        });

        let graph = self.graph.clone();
        engine.register_fn("get_graph", move || graph.graph.clone());

        let graph = self.graph.clone();
        engine.register_fn("get_path_positions", move || {
            graph.path_positions.clone()
        });

        self.add_view_fns(&mut engine);

        self.add_overlay_fns(&mut engine);

        let app_msg_tx = self.channels.app_tx.clone();
        engine.register_fn("get_selection", move || {
            use crossbeam::channel;

            let (tx, rx) = channel::bounded::<(Rect, FxHashSet<NodeId>)>(16);
            let msg = AppMsg::RequestSelection(tx);

            app_msg_tx.send(msg).unwrap();

            let (_rect, result) = rx
                .recv()
                .expect("Console error when retrieving the current selection");

            NodeSelection { nodes: result }
        });

        // TODO probably... don't do it like this
        let app_msg_tx = self.channels.app_tx.clone();
        engine.register_fn("get_selection_center", move || {
            use crossbeam::channel;

            let (tx, rx) = channel::unbounded::<(Rect, FxHashSet<NodeId>)>();
            let msg = AppMsg::RequestSelection(tx.clone());

            app_msg_tx.send(msg).unwrap();

            let (rect, _result) =
                std::thread::spawn(move || rx.recv().unwrap())
                    .join()
                    .unwrap();

            rect.center()
        });

        let app_msg_tx = self.channels.app_tx.clone();
        engine.register_fn("set_selection", move |selection: NodeSelection| {
            let msg = AppMsg::Selection(Select::Many {
                nodes: selection.nodes,
                clear: true,
            });
            app_msg_tx.send(msg).unwrap();
        });

        let app_msg_tx = self.channels.app_tx.clone();
        engine.register_fn("pan_to_active_selection", move || {
            let msg = AppMsg::GotoSelection;
            app_msg_tx.send(msg).unwrap();
        });

        let graph = self.graph.graph.clone();
        engine.register_fn(
            "path_selection",
            move |path: PathId| -> NodeSelection {
                let mut selection = NodeSelection::default();
                if let Some(steps) = graph.path_steps(path) {
                    for step in steps {
                        let id = step.handle().id();
                        selection.add_one(false, id);
                    }
                }
                selection
            },
        );

        let arc = self.shared_state.hover_node.clone();
        engine.register_fn("get_hover_node", move || arc.load());

        let app_msg_tx = self.channels.app_tx.clone();
        engine.register_fn("toggle_dark_mode", move || {
            app_msg_tx.send(crate::app::AppMsg::ToggleDarkMode).unwrap();
        });
        let app_msg_tx = self.channels.app_tx.clone();
        engine.register_fn("toggle_overlay", move || {
            app_msg_tx.send(crate::app::AppMsg::ToggleOverlay).unwrap();
        });

        let get_set = self.get_set.clone();
        engine.register_result_fn("get", move |name: &str| {
            get_set
                .getters
                .get(name)
                .map(|get| get())
                .ok_or(format!("Setting `{}` not found", name).into())
        });

        let get_set = self.get_set.clone();

        engine.register_result_fn(
            "set",
            move |name: &str, val: rhai::Dynamic| {
                get_set
                    .setters
                    .get(name)
                    .map(|set| set(val))
                    .ok_or(format!("Setting `{}` not found", name).into())
            },
        );

        let get_set = self.get_set.clone();
        engine.register_result_fn("get_var", move |name: &str| {
            let lock = get_set.console_vars.try_lock();
            let val = lock.and_then(|l| l.get(name).cloned());
            val.ok_or(format!("Global variable `{}` not found", name).into())
        });

        let get_set = self.get_set.clone();
        engine.register_fn("set_var", move |name: &str, val: rhai::Dynamic| {
            let mut lock = get_set.console_vars.lock();
            lock.insert(name.to_string(), val);
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

    fn add_overlay_fns(&self, engine: &mut rhai::Engine) {
        engine.register_type_with_name::<(usize, OverlayKind)>("OverlayHandle");

        // returns `false` if there is no active overlay
        let overlay_state = self.shared_state.overlay_state.clone();
        engine.register_fn("get_active_overlay", move || -> rhai::Dynamic {
            if let Some(cur_overlay) = overlay_state.current_overlay() {
                rhai::Dynamic::from(cur_overlay)
            } else {
                false.into()
            }
        });

        let overlay_state = self.shared_state.overlay_state.clone();
        engine.register_fn("set_active_overlay", move |v: rhai::Dynamic| {
            if let Ok(v) = v.as_unit() {
                overlay_state.set_current_overlay(None);
            } else if let Some(overlay) = v.try_cast::<(usize, OverlayKind)>() {
                overlay_state.set_current_overlay(Some(overlay));
            }
        });

        let overlay_list: Arc<_> = self.overlay_list.clone();
        // let overlay_map: Arc<HashMap<String, (usize, OverlayKind)>> =
        engine.register_fn("get_overlays", move || {
            // TODO: should probably use try_lock -- but the overlays
            // shouldn't be organized like this anyway
            let overlays = overlay_list.lock();
            overlays
                .iter()
                .map(|v| rhai::Dynamic::from(v.to_owned()))
                .collect::<Vec<_>>()
        });

        engine.register_fn(
            "overlay_name",
            move |overlay: (usize, OverlayKind, String)| overlay.2,
        );

        engine.register_fn(
            "overlay_id",
            move |overlay: (usize, OverlayKind, String)| (overlay.0, overlay.1),
        );
        // engine
        //     .register_fn("get_overlays", move || overlays.as_slice().to_vec());

        // engine.register_fn(
        // "evaluate_overlay_script")
    }

    fn add_view_fns(&self, engine: &mut Engine) {
        engine.register_type::<View>();

        engine.register_get_set(
            "scale",
            |v: &mut View| v.scale,
            |v, s| v.scale = s,
        );
        engine.register_get_set(
            "center",
            |v: &mut View| v.center,
            |v, c| v.center = c,
        );

        // NB: these are not regular console get/sets because the view
        // system will likely be reworked soon, to make it a queue --
        // and remove direct mutable access by other systems
        // (but for now, load/store is enough)
        let view = self.shared_state.view.clone();
        engine.register_fn("get_view", move || view.load());

        let view = self.shared_state.view.clone();
        engine.register_fn("set_view", move |v: View| view.store(v));

        let view = self.shared_state.view.clone();
        engine.register_fn("set_view_origin", move |p: Point| {
            let mut v = view.load();
            v.center = p;
            view.store(v);
        });

        let view = self.shared_state.view.clone();
        engine.register_fn("set_scale", move |s: f32| {
            let mut v = view.load();
            v.scale = s;
            view.store(v);
        });

        let mouse = self.shared_state.mouse_pos.clone();
        let view = self.shared_state.view.clone();
        let screen_dims = self.shared_state.screen_dims.clone();

        engine.register_fn("get_cursor_world", move || {
            let screen = mouse.load();
            let view = view.load();
            let dims = screen_dims.load();
            view.screen_point_to_world(dims, screen)
        });
    }
}

pub struct Console {
    core: ConsoleCore,

    scope: Arc<Mutex<rhai::Scope<'static>>>,

    remote_handles: HashMap<String, RemoteHandle<()>>,

    input_tx: crossbeam::channel::Sender<String>,
    result_rx: crossbeam::channel::Receiver<ScriptEvalResult>,
}

pub struct ConsoleGui {
    console: Console,

    input_line: String,
    input_history: Vec<String>,
    input_history_offset: Option<usize>,

    output_history: Vec<String>,
    output_history_offset: usize,

    request_focus: bool,
}

impl Console {
    pub fn new(
        reactor: &Reactor,
        graph: &Arc<GraphQuery>,
        channels: AppChannels,
        settings: AppSettings,
        shared_state: SharedState,
    ) -> Self {
        let (result_tx, result_rx) =
            crossbeam::channel::unbounded::<ScriptEvalResult>();

        let rayon_pool = reactor.rayon_pool.clone();

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
            bool,
            "overlay_enabled",
            shared_state.overlay_state.use_overlay.clone()
        );

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

        let scope = Self::create_scope();
        let scope = Arc::new(Mutex::new(scope));

        let output_history =
            vec![" < close this console with Esc >".to_string()];

        let key_code_map = Arc::new(virtual_key_code_map());

        let overlay_list = Arc::new(Mutex::new(Vec::new()));
        let label_map = Arc::new(Mutex::new(HashMap::default()));

        let (input_tx, input_rx) = crossbeam::channel::unbounded::<String>();

        let core = ConsoleCore {
            settings,
            shared_state,
            channels,
            get_set: Arc::new(get_set),
            graph: graph.clone(),

            overlay_list,
            label_map,
            key_code_map,

            rayon_pool,

            input_rx,
            result_tx,

            modules: Arc::new(Mutex::new(Vec::new())),
        };

        Console {
            core,
            scope,
            remote_handles: Default::default(),

            input_tx,
            result_rx,
        }
    }

    pub fn labels(&self) -> Vec<(Point, String)> {
        let labels = self.core.label_map.lock();
        labels.values().cloned().collect()
    }

    pub fn populate_overlay_list(
        &mut self,
        names: &[(usize, OverlayKind, &str)],
    ) {
        let mut overlays = self.core.overlay_list.lock();
        overlays.clear();
        overlays.extend(names.iter().map(|&(a, b, s)| (a, b, s.to_string())));
    }

    fn create_scope() -> rhai::Scope<'static> {
        let scope = rhai::Scope::new();
        scope
    }

    fn create_engine(&self) -> rhai::Engine {
        let core = self.core.clone();

        let modules = core.modules.clone();

        let key_code_map = core.key_code_map.clone();
        let binds_tx = core.channels.binds_tx.clone();

        let mut engine = core.create_engine();

        engine.register_fn(
            "bind_key",
            move |key: &str, fn_name: rhai::Dynamic| {
                log::debug!("in bind_key");

                let key_code = if let Some(map) = key_code_map.get(key) {
                    map
                } else {
                    return;
                };

                if let Some(fn_name) = fn_name.try_cast::<String>() {
                    let scope = Self::create_scope();

                    let mut engine = core.create_engine();
                    {
                        let modules = modules.lock();
                        for module in modules.iter() {
                            engine.register_global_module(module.clone());
                        }
                    }

                    log::debug!("compiling to AST");
                    let script =
                        format!("fn a_function() {{\n{}();\n}}", fn_name);

                    let ast = engine.compile_with_scope(&scope, &script);

                    match ast {
                        Ok(ast) => {
                            let function =
                                rhai::Func::<(), ()>::create_from_ast(
                                    engine,
                                    ast,
                                    "a_function",
                                );

                            binds_tx
                                .send((
                                    *key_code,
                                    Some(Box::new(move || match function() {
                                        Ok(_) => (),
                                        Err(err) => log::warn!(
                                            "bound function error: {:?}",
                                            err
                                        ),
                                    })),
                                ))
                                .unwrap();
                        }
                        Err(err) => {
                            log::warn!("compilation error: {:?}", err);
                        }
                    }
                }
            },
        );

        let rayon_pool = self.core.rayon_pool.clone();
        let graph = self.core.graph.clone();
        let config = ScriptConfig {
            default_color: rgb::RGBA::new(0.3, 0.3, 0.3, 0.3),
            target: ScriptTarget::Nodes,
        };

        let overlay_tx = self.core.channels.new_overlay_tx.clone();
        let core = self.core.clone();
        let modules = core.modules.clone();
        engine.register_fn(
            "create_overlay_from_fn",
            move |name: &str, fn_name: rhai::Dynamic| {
                if let Some(fn_name) = fn_name.try_cast::<String>() {
                    let mut scope = Self::create_scope();

                    scope
                        .push("graph", graph.graph.clone())
                        .push("path_pos", graph.path_positions.clone());

                    let mut engine = core.create_engine();
                    {
                        let modules = modules.lock();
                        for module in modules.iter() {
                            engine.register_global_module(module.clone());
                        }
                    }

                    let script =
                        format!("\nfn node_color(i) {{\n{}(i);\n}}", fn_name);
                    log::debug!("script: {}", script);

                    let node_color_ast =
                        engine.compile_into_self_contained(&scope, &script);

                    match node_color_ast {
                        Ok(node_color_ast) => {
                            let result = overlay_colors_tgt_(
                                &rayon_pool,
                                &config,
                                &graph,
                                &engine,
                                scope,
                                node_color_ast,
                            );

                            match result {
                                Ok(data) => {
                                    let msg = OverlayCreatorMsg::NewOverlay {
                                        name: name.to_string(),
                                        data,
                                    };
                                    overlay_tx.send(msg).unwrap();
                                    log::info!("overlay data success");
                                }
                                Err(err) => {
                                    log::warn!("overlay failure");
                                }
                            }
                        }
                        Err(err) => {
                            log::warn!("ast failure");
                        }
                    }
                }
            },
        );

        {
            let modules = self.core.modules.lock();

            for module in modules.iter() {
                engine.register_global_module(module.clone());
            }
        }

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
            self.core.result_tx.send(Ok(format!(
                ">>> Evaluating file '{}'",
                path
            )
            .into()))?;
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
        // std::mem::swap(&mut old_input, &mut self.input_line);

        self.eval(reactor, print)?;
        // std::mem::swap(&mut old_input, &mut self.input_line);

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
            self.scope = Arc::new(Mutex::new(Self::create_scope()));
            self.input_line.clear();
            self.input_history.clear();
            self.output_history.clear();
            {
                let mut modules = self.modules.lock();
                modules.clear();
            }

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
        } else if self.input_line.starts_with(":import ") {
            log::debug!("importing file");
            let file_path = &self.input_line[8..].to_string();
            let result = self.import_file(&file_path);

            if let Err(err) = result {
                let msg = format!(
                    " >>> error importing file {}: {:?}",
                    file_path, err
                );
                self.append_output(&msg);

                log::warn!(
                    "console :import of file '{}' failed: {:?}",
                    file_path,
                    err
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
                use std::any::Any;

                if print {
                    let rtype = result.type_id();

                    if let Ok(_) = result.as_unit() {
                        // don't log unit
                    } else if rtype == rgb::RGB::<f32>::default().type_id() {
                        let color = result.cast::<rgb::RGB<f32>>();
                        self.append_output(&format!("{}", color))
                    } else if rtype == rgb::RGBA::<f32>::default().type_id() {
                        let color = result.cast::<rgb::RGBA<f32>>();
                        self.append_output(&format!("{}", color));
                    } else {
                        self.append_output(&format!("{:?}", result));
                    }
                }
            }
            Err(err) => {
                debug!("Eval error: {:?}", err);
                if print {
                    self.append_output(&format!("Error: {:?}", err));
                }
            }
        }

        Ok(())
    }

    pub fn import_file(&mut self, file: &str) -> Result<()> {
        let engine = self.create_engine();

        let ast = engine.compile_file(file.into())?;
        let module =
            rhai::Module::eval_ast_as_new(rhai::Scope::new(), &ast, &engine)?;

        let (vars, funcs, iters) = module.count();

        let msg = format!(
            " >>> imported {} variables, {} functions, and {} iterators from '{}'", vars, funcs, iters, file);
        self.append_output(&msg);

        {
            let mut modules = self.modules.lock();
            modules.push(Arc::new(module));
        }

        Ok(())
    }

    pub fn eval(
        &mut self,
        reactor: &mut Reactor,
        line: &str,
        print: bool,
    ) -> Result<()> {
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
}

impl ConsoleGui {
    pub fn new(
        reactor: &Reactor,
        graph: &Arc<GraphQuery>,
        channels: AppChannels,
        settings: AppSettings,
        shared_state: SharedState,
    ) -> Self {
        let console =
            Console::new(reactor, graph, channels, settings, shared_state);

        Self {
            console,

            input_line: String::new(),

            input_history: Vec::new(),
            input_history_offset: None,

            output_history: Vec::new(),
            output_history_offset: 0,

            request_focus: false,
        }
    }

    pub fn core(&self) -> &ConsoleCore {
        &self.console.core
    }
}

// pub trait ConsoleInput {

// }

/*
pub struct Console<I, O>
where
    I: AsyncBufRead,
    O: AsyncWrite,
{
    core: ConsoleCore,

    input: I,
    output: O,
}
*/

// TODO this should be in App or something
#[derive(Default)]
pub struct GetSetTruth {
    getters:
        HashMap<String, Box<dyn Fn() -> rhai::Dynamic + Send + Sync + 'static>>,
    setters:
        HashMap<String, Box<dyn Fn(rhai::Dynamic) + Send + Sync + 'static>>,

    console_vars: Mutex<HashMap<String, rhai::Dynamic>>,
}

impl GetSetTruth {
    pub fn add_var(&mut self, name: &str, val: rhai::Dynamic) {
        let mut lock = self.console_vars.lock();
        lock.insert(name.to_string(), val);
    }

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

fn virtual_key_code_map() -> HashMap<String, winit::event::VirtualKeyCode> {
    use winit::event::VirtualKeyCode as Key;

    let keys = [
        ("Key1", Key::Key1),
        ("Key2", Key::Key2),
        ("Key3", Key::Key3),
        ("Key4", Key::Key4),
        ("Key5", Key::Key5),
        ("Key6", Key::Key6),
        ("Key7", Key::Key7),
        ("Key8", Key::Key8),
        ("Key9", Key::Key9),
        ("Key0", Key::Key0),
        ("A", Key::A),
        ("B", Key::B),
        ("C", Key::C),
        ("D", Key::D),
        ("E", Key::E),
        ("F", Key::F),
        ("G", Key::G),
        ("H", Key::H),
        ("I", Key::I),
        ("J", Key::J),
        ("K", Key::K),
        ("L", Key::L),
        ("M", Key::M),
        ("N", Key::N),
        ("O", Key::O),
        ("P", Key::P),
        ("Q", Key::Q),
        ("R", Key::R),
        ("S", Key::S),
        ("T", Key::T),
        ("U", Key::U),
        ("V", Key::V),
        ("W", Key::W),
        ("X", Key::X),
        ("Y", Key::Y),
        ("Z", Key::Z),
        ("Escape", Key::Escape),
        ("F1", Key::F1),
        ("F2", Key::F2),
        ("F3", Key::F3),
        ("F4", Key::F4),
        ("F5", Key::F5),
        ("F6", Key::F6),
        ("F7", Key::F7),
        ("F8", Key::F8),
        ("F9", Key::F9),
        ("F10", Key::F10),
        ("F11", Key::F11),
        ("F12", Key::F12),
        ("F13", Key::F13),
        ("F14", Key::F14),
        ("F15", Key::F15),
        ("F16", Key::F16),
        ("F17", Key::F17),
        ("F18", Key::F18),
        ("F19", Key::F19),
        ("F20", Key::F20),
        ("F21", Key::F21),
        ("F22", Key::F22),
        ("F23", Key::F23),
        ("F24", Key::F24),
        ("Snapshot", Key::Snapshot),
        ("Scroll", Key::Scroll),
        ("Pause", Key::Pause),
        ("Insert", Key::Insert),
        ("Home", Key::Home),
        ("Delete", Key::Delete),
        ("End", Key::End),
        ("PageDown", Key::PageDown),
        ("PageUp", Key::PageUp),
        ("Left", Key::Left),
        ("Up", Key::Up),
        ("Right", Key::Right),
        ("Down", Key::Down),
        ("Back", Key::Back),
        ("Return", Key::Return),
        ("Space", Key::Space),
        ("Compose", Key::Compose),
        ("Caret", Key::Caret),
        ("Numlock", Key::Numlock),
        ("Numpad0", Key::Numpad0),
        ("Numpad1", Key::Numpad1),
        ("Numpad2", Key::Numpad2),
        ("Numpad3", Key::Numpad3),
        ("Numpad4", Key::Numpad4),
        ("Numpad5", Key::Numpad5),
        ("Numpad6", Key::Numpad6),
        ("Numpad7", Key::Numpad7),
        ("Numpad8", Key::Numpad8),
        ("Numpad9", Key::Numpad9),
        ("NumpadAdd", Key::NumpadAdd),
        ("NumpadDivide", Key::NumpadDivide),
        ("NumpadDecimal", Key::NumpadDecimal),
        ("NumpadComma", Key::NumpadComma),
        ("NumpadEnter", Key::NumpadEnter),
        ("NumpadEquals", Key::NumpadEquals),
        ("NumpadMultiply", Key::NumpadMultiply),
        ("NumpadSubtract", Key::NumpadSubtract),
        ("AbntC1", Key::AbntC1),
        ("AbntC2", Key::AbntC2),
        ("Apostrophe", Key::Apostrophe),
        ("Apps", Key::Apps),
        ("Asterisk", Key::Asterisk),
        ("At", Key::At),
        ("Ax", Key::Ax),
        ("Backslash", Key::Backslash),
        ("Calculator", Key::Calculator),
        ("Capital", Key::Capital),
        ("Colon", Key::Colon),
        ("Comma", Key::Comma),
        ("Convert", Key::Convert),
        ("Equals", Key::Equals),
        ("Grave", Key::Grave),
        ("Kana", Key::Kana),
        ("Kanji", Key::Kanji),
        ("LAlt", Key::LAlt),
        ("LBracket", Key::LBracket),
        ("LControl", Key::LControl),
        ("LShift", Key::LShift),
        ("LWin", Key::LWin),
        ("Mail", Key::Mail),
        ("MediaSelect", Key::MediaSelect),
        ("MediaStop", Key::MediaStop),
        ("Minus", Key::Minus),
        ("Mute", Key::Mute),
        ("MyComputer", Key::MyComputer),
        ("NavigateForward", Key::NavigateForward),
        ("NavigateBackward", Key::NavigateBackward),
        ("NextTrack", Key::NextTrack),
        ("NoConvert", Key::NoConvert),
        ("OEM102", Key::OEM102),
        ("Period", Key::Period),
        ("PlayPause", Key::PlayPause),
        ("Plus", Key::Plus),
        ("Power", Key::Power),
        ("PrevTrack", Key::PrevTrack),
        ("RAlt", Key::RAlt),
        ("RBracket", Key::RBracket),
        ("RControl", Key::RControl),
        ("RShift", Key::RShift),
        ("RWin", Key::RWin),
        ("Semicolon", Key::Semicolon),
        ("Slash", Key::Slash),
        ("Sleep", Key::Sleep),
        ("Stop", Key::Stop),
        ("Sysrq", Key::Sysrq),
        ("Tab", Key::Tab),
        ("Underline", Key::Underline),
        ("Unlabeled", Key::Unlabeled),
        ("VolumeDown", Key::VolumeDown),
        ("VolumeUp", Key::VolumeUp),
        ("Wake", Key::Wake),
        ("WebBack", Key::WebBack),
        ("WebFavorites", Key::WebFavorites),
        ("WebForward", Key::WebForward),
        ("WebHome", Key::WebHome),
        ("WebRefresh", Key::WebRefresh),
        ("WebSearch", Key::WebSearch),
        ("WebStop", Key::WebStop),
        ("Yen", Key::Yen),
        ("Copy", Key::Copy),
        ("Paste", Key::Paste),
        ("Cut", Key::Cut),
    ]
    .iter()
    .map(|(n, c)| (n.to_string(), *c))
    .collect();

    keys
}
