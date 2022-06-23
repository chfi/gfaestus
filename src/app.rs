pub mod channels;
pub mod mainview;
pub mod selection;
pub mod settings;
pub mod shared_state;

pub use channels::*;
use handlegraph::pathhandlegraph::PathId;
pub use settings::*;
pub use shared_state::*;

use crossbeam::channel::Sender;

use rustc_hash::{FxHashMap, FxHashSet};

use handlegraph::handle::NodeId;

use anyhow::Result;

use argh::FromArgs;

use std::any::TypeId;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use self::mainview::MainViewMsg;
use crate::annotations::{
    AnnotationCollection, AnnotationLabelSet, Annotations, BedRecords,
    Gff3Records, LabelSet, Labels,
};
use crate::app::selection::NodeSelection;
use crate::graph_query::GraphQuery;
use crate::gui::GuiMsg;
use crate::reactor::Reactor;
use crate::view::*;
use crate::{geometry::*, input::binds::SystemInputBindings};
use crate::{
    input::binds::{BindableInput, KeyBind, SystemInput},
    universe::Node,
};

pub struct App {
    pub shared_state: SharedState,
    pub channels: AppChannels,
    pub settings: AppSettings,

    layout_boundary: Rect,

    pub reactor: Reactor,

    selected_nodes: FxHashSet<NodeId>,
    selection_changed: bool,

    pub selected_nodes_bounding_box: Option<(Point, Point)>,

    pub annotations: Annotations,

    pub labels: Labels,

    msg_handlers: HashMap<String, Arc<AppMsgHandler>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppInput {
    KeyClearSelection,
    KeyToggleTheme,
}

impl BindableInput for AppInput {
    fn default_binds() -> SystemInputBindings<Self> {
        use winit::event::VirtualKeyCode as Key;
        use AppInput as Input;

        let key_binds: FxHashMap<Key, Vec<KeyBind<Input>>> = [
            (Key::Escape, Input::KeyClearSelection),
            (Key::F9, Input::KeyToggleTheme),
        ]
        .iter()
        .copied()
        .map(|(k, i)| (k, vec![KeyBind::new(i)]))
        .collect::<FxHashMap<_, _>>();

        let mouse_binds = FxHashMap::default();

        let wheel_bind = None;

        SystemInputBindings::new(key_binds, mouse_binds, wheel_bind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Select {
    Clear,
    One {
        node: NodeId,
        clear: bool,
    },
    Many {
        nodes: FxHashSet<NodeId>,
        clear: bool,
    },
}

// #[derive(Debug)]
#[derive(Debug, Clone)]
pub enum AppMsg {
    Selection(Select),

    // TODO these two should not be here (see how they're handled in main)
    RectSelect(Rect),
    TranslateSelected(Point),

    NewNodeLabels {
        name: String,
        label_set: AnnotationLabelSet,
    },

    RequestSelection(crossbeam::channel::Sender<(Rect, FxHashSet<NodeId>)>),

    SetData {
        key: String,
        index: String,
        value: rhai::Dynamic,
    },
    ConsoleEval {
        script: String,
    },
    Raw {
        type_id: TypeId,
        msg_name: String,
        value: Arc<dyn std::any::Any + Send + Sync>,
    },
}

impl AppMsg {
    pub fn register(engine: &mut rhai::Engine) {
        engine.register_type_with_name::<AppMsg>("AppMsg");
        // engine.register_fn("to_string", |msg| {
        //     &mut AppMsg | {
        //         match msg {
        //         //
        //     }
        //     }
        // });
    }

    pub fn raw<T: std::any::Any + Send + Sync>(msg_name: &str, v: T) -> Self {
        AppMsg::Raw {
            type_id: TypeId::of::<T>(),
            msg_name: msg_name.to_string(),
            value: Arc::new(v) as _,
        }
    }

    pub fn set_clipboard_contents(contents: &str) -> Self {
        Self::raw("set_clipboard_contents", contents.to_string())
    }

    pub fn add_gff3_records(records: Gff3Records) -> Self {
        Self::raw("add_gff3_records", records)
    }

    pub fn add_bed_records(records: BedRecords) -> Self {
        Self::raw("add_bed_records", records)
    }

    pub fn goto_node(id: NodeId) -> Self {
        Self::raw("goto_node", id)
    }

    pub fn goto_rect(rect: Rect) -> Self {
        Self::raw("goto_rect", Some(rect))
    }

    pub fn goto_selection() -> Self {
        Self::raw::<Option<Rect>>("goto_rect", None)
    }

    pub fn clear_selection() -> Self {
        Self::raw("clear_selection", ())
    }

    pub fn toggle_dark_mode() -> Self {
        Self::raw("toggle_dark_mode", ())
    }

    pub fn new_label_set(
        name: String,
        label_set: LabelSet,
        // on_label_click: Option<Box<dyn Fn(usize) + Send + Sync + 'static>>,
    ) -> Self {
        let val = Arc::new((name, label_set));
        Self::raw("new_label_set", val)
    }

    pub fn request_data_(
        key: String,
        index: String,
        sender: crossbeam::channel::Sender<Result<rhai::Dynamic>>,
    ) -> Self {
        Self::raw("request_data", (key, index, sender))
    }

    pub fn request_data(
        key: String,
        index: String,
    ) -> (Self, crossbeam::channel::Receiver<Result<rhai::Dynamic>>) {
        let (tx, rx) = crossbeam::channel::bounded::<Result<rhai::Dynamic>>(1);
        let msg = Self::request_data_(key, index, tx);

        (msg, rx)
    }

    // pub fn set_data(key: &str, index: &str, value: rhai::Dynamic) -> Self {
    // Self::raw("set_data", (key.to_string(), index.to_string(), value))
    pub fn set_data(key: String, index: String, value: rhai::Dynamic) -> Self {
        Self::raw("set_data", (key, index, value))
    }
}

impl App {
    pub fn new<Dims: Into<ScreenDims>>(
        screen_dims: Dims,
        thread_pool: futures::executor::ThreadPool,
        rayon_pool: rayon::ThreadPool,
        graph_query: Arc<GraphQuery>,
        layout_boundary: Rect,
    ) -> Result<Self> {
        let shared_state = SharedState::new(screen_dims);

        let channels = AppChannels::new();

        let reactor = crate::reactor::Reactor::init(
            thread_pool,
            rayon_pool,
            graph_query,
            &channels,
        );

        let mut msg_handlers = HashMap::default();
        Self::add_msg_handlers(&mut msg_handlers);

        Ok(Self {
            shared_state,
            channels,

            reactor,

            layout_boundary,

            selected_nodes: FxHashSet::default(),
            selection_changed: false,

            selected_nodes_bounding_box: None,

            settings: AppSettings::default(),

            annotations: Annotations::default(),

            labels: Labels::default(),

            msg_handlers,
        })
    }

    pub fn send_msg(&self, msg: AppMsg) -> Result<()> {
        self.channels.app_tx.send(msg)?;

        Ok(())
    }

    pub fn send_raw<T: std::any::Any + Send + Sync>(
        &self,
        msg_name: &str,
        v: T,
    ) -> Result<()> {
        let type_id = TypeId::of::<T>();
        let msg_name = msg_name.to_string();
        let value = Arc::new(v) as _;

        log::warn!("sending raw: {:?}, {}", type_id, msg_name);

        let msg = AppMsg::Raw {
            type_id,
            msg_name,
            value,
        };

        self.channels.app_tx.send(msg)?;

        Ok(())
    }

    pub fn shared_state(&self) -> &SharedState {
        &self.shared_state
    }

    pub fn channels(&self) -> &AppChannels {
        &self.channels
    }

    pub fn clone_channels(&self) -> AppChannels {
        self.channels.clone()
    }

    pub fn hover_node(&self) -> Option<NodeId> {
        self.shared_state.hover_node.load()
    }

    pub fn has_selection(&self) -> bool {
        !self.selected_nodes.is_empty()
    }

    pub fn selection_changed(&self) -> bool {
        self.selection_changed
    }

    pub fn selected_nodes_(&self) -> Option<(Rect, &FxHashSet<NodeId>)> {
        log::warn!(
            "self.selected_nodes.is_empty() = {}",
            self.selected_nodes.is_empty()
        );
        if self.selected_nodes.is_empty() {
            None
        } else {
            let rect = self
                .selected_nodes_bounding_box
                .map(|(p0, p1)| Rect::new(p0, p1))
                .unwrap_or(Rect::default());
            log::warn!("got a bounding box");
            Some((rect, &self.selected_nodes))
        }
    }

    // not even sure where selection_changed is used anymore, if at all
    pub fn selected_nodes(&mut self) -> Option<&FxHashSet<NodeId>> {
        if self.selected_nodes.is_empty() {
            self.selection_changed = false;
            None
        } else {
            self.selection_changed = false;
            Some(&self.selected_nodes)
        }
    }

    pub fn annotations(&self) -> &Annotations {
        &self.annotations
    }

    pub fn labels(&self) -> &Labels {
        &self.labels
    }

    pub fn labels_mut(&mut self) -> &mut Labels {
        &mut self.labels
    }

    pub fn dims(&self) -> ScreenDims {
        self.shared_state.screen_dims.load()
    }

    pub fn mouse_pos(&self) -> Point {
        self.shared_state.mouse_pos.load()
    }

    pub fn update_dims<Dims: Into<ScreenDims>>(&mut self, screen_dims: Dims) {
        self.shared_state.screen_dims.store(screen_dims.into());
    }

    fn add_msg_handlers(handlers: &mut HashMap<String, Arc<AppMsgHandler>>) {
        let mut new_handler = |name: &str, handler: AppMsgHandler| {
            handlers.insert(name.to_string(), Arc::new(handler))
        };

        new_handler(
            "save_selection",
            AppMsgHandler::from_fn(|app, nodes, save_file: &PathBuf| {
                use std::io::prelude::*;
                let mut file = std::fs::File::create(save_file).unwrap();
                for node in app.selected_nodes.iter() {
                    writeln!(file, "{}", node.0).unwrap()
                }
            }),
        );

        new_handler(
            "set_clipboard_contents",
            AppMsgHandler::from_fn(|app, nodes, contents: &String| {
                app.reactor.set_clipboard_contents(contents, true);
            }),
        );

        new_handler(
            "goto_node",
            AppMsgHandler::from_fn(|app, nodes, id: &NodeId| {
                if let Some(node_pos) = nodes.get((id.0 - 1) as usize) {
                    let mut view = app.shared_state.view();
                    view.center = node_pos.center();
                    app.channels
                        .main_view_tx
                        .send(MainViewMsg::GotoView(view))
                        .unwrap();
                }
            }),
        );

        new_handler(
            "goto_rect",
            AppMsgHandler::from_fn(|app, _nodes, rect: &Option<Rect>| {
                let bounds: Option<Rect> = rect
                    .or_else(|| Some(app.selected_nodes_bounding_box?.into()));

                if let Some(rect) = bounds {
                    let view = View::from_dims_and_target(
                        app.dims(),
                        rect.min(),
                        rect.max(),
                    );
                    app.channels
                        .main_view_tx
                        .send(MainViewMsg::GotoView(view))
                        .unwrap();
                }
            }),
        );

        new_handler(
            "add_gff3_records",
            AppMsgHandler::from_fn(
                |app, _nodes, records: &Arc<Gff3Records>| {
                    let file_name = records.file_name().to_string();
                    app.annotations
                        .insert_gff3_arc(&file_name, records.clone());
                },
            ),
        );

        new_handler(
            "add_bed_records",
            AppMsgHandler::from_fn(|app, _nodes, records: &Arc<BedRecords>| {
                let file_name = records.file_name().to_string();
                app.annotations.insert_bed_arc(&file_name, records.clone());
            }),
        );

        new_handler(
            "toggle_dark_mode",
            AppMsgHandler::from_fn(|app, _nodes, _: &()| {
                app.toggle_dark_mode();
            }),
        );

        new_handler(
            "new_label_set",
            AppMsgHandler::from_fn(
                |app,
                 node_positions,
                 val: &Arc<(
                    String,
                    LabelSet,
                    // Option<Box<dyn Fn(usize) + Send + Sync + 'static>>,
                )>| {
                    // let (name, label_set) = &val;

                    app.labels.add_label_set(
                        app.layout_boundary,
                        node_positions,
                        &val.0,
                        &val.1,
                        None,
                        // on_label_click,
                    );
                },
            ),
        );

        new_handler(
            "request_data",
            AppMsgHandler::from_fn(
                |app,
                 _nodes,
                 (key, index, sender): &(
                    String,
                    String,
                    crossbeam::channel::Sender<Result<rhai::Dynamic>>,
                )| {
                    type ReqResult = Result<rhai::Dynamic>;

                    macro_rules! handle {
                        ($expr:expr, $err:literal) => {
                            if let Some(result) = $expr {
                                Ok(rhai::Dynamic::from(result.clone()))
                            } else {
                                let err = anyhow::anyhow!($err);
                                Err(err) as ReqResult
                            }
                        };
                    }

                    let boxed = match key.as_str() {
                        "annotation_file" => {
                            if let Some(records) =
                                app.annotations.get_gff3(&index)
                            {
                                Ok(rhai::Dynamic::from(records.clone()))
                            } else if let Some(records) =
                                app.annotations.get_bed(&index)
                            {
                                Ok(rhai::Dynamic::from(records.clone()))
                            } else {
                                Err(anyhow::anyhow!(
                                    "Annotation file not loaded: {}",
                                    index
                                ))
                            }
                        }
                        "annotation_names" => {
                            let names = app
                                .annotations
                                .annot_names()
                                .iter()
                                .map(|(name, _)| name.to_string())
                                .collect::<Vec<_>>();

                            Ok(rhai::Dynamic::from(names))
                        }
                        "annotation_ref_path" => {
                            if let Some(path) =
                                app.annotations.get_default_ref_path(&index)
                            {
                                Ok(rhai::Dynamic::from(path))
                            } else {
                                Ok(rhai::Dynamic::from(()))
                            }
                        }
                        _ => {
                            let err = anyhow::anyhow!(
                                "Requested unknown key from App"
                            );
                            Err(err) as ReqResult
                        }
                    };

                    sender.send(boxed).unwrap();
                },
            ),
        );

        new_handler(
            "set_data",
            AppMsgHandler::from_fn(|app, _nodes, (k, i, value): &(String, String, rhai::Dynamic)| {

                match k.as_str() {
                _ => (),

                }
            }));
    }

    pub fn apply_app_msg(
        &mut self,
        console_input_tx: &Sender<String>,
        node_positions: &[Node],
        msg: AppMsg,
    ) {
        match msg {
            AppMsg::RectSelect(_rect) => {
                //
            }
            AppMsg::TranslateSelected(delta) => {
                if let Some(bounds) = self.selected_nodes_bounding_box {
                    let min = bounds.0 + delta;
                    let max = bounds.1 + delta;

                    self.selected_nodes_bounding_box = Some((min, max));
                }
            }
            AppMsg::Selection(sel) => match sel {
                Select::Clear => {
                    self.selection_changed = true;
                    self.selected_nodes.clear();
                    self.selected_nodes_bounding_box = None;
                }
                Select::One { node, clear } => {
                    self.selection_changed = true;
                    if clear {
                        self.selected_nodes.clear();
                        self.selected_nodes_bounding_box = None;
                    }
                    self.selected_nodes.insert(node);

                    let node_pos = node_positions[(node.0 - 1) as usize];

                    if let Some(bounds) = self.selected_nodes_bounding_box {
                        let old_min = Point {
                            x: bounds.0.x.min(bounds.1.x),
                            y: bounds.0.y.min(bounds.1.y),
                        };

                        let old_max = Point {
                            x: bounds.0.x.max(bounds.1.x),
                            y: bounds.0.y.max(bounds.1.y),
                        };

                        let top_left = Point {
                            x: old_min.x.min(node_pos.p0.x.min(node_pos.p1.x)),
                            y: old_min.y.min(node_pos.p0.y.min(node_pos.p1.y)),
                        };

                        let bottom_right = Point {
                            x: old_max.x.max(node_pos.p0.x.max(node_pos.p1.x)),
                            y: old_max.y.max(node_pos.p0.y.max(node_pos.p1.y)),
                        };

                        self.selected_nodes_bounding_box =
                            Some((top_left, bottom_right));
                    } else {
                        let top_left = Point {
                            x: node_pos.p0.x.min(node_pos.p1.x),
                            y: node_pos.p0.y.min(node_pos.p1.y),
                        };

                        let bottom_right = Point {
                            x: node_pos.p0.x.max(node_pos.p1.x),
                            y: node_pos.p0.y.max(node_pos.p1.y),
                        };

                        self.selected_nodes_bounding_box =
                            Some((top_left, bottom_right));
                    }
                }
                Select::Many { nodes, clear } => {
                    self.selection_changed = true;
                    if clear {
                        self.selected_nodes.clear();
                        self.selected_nodes_bounding_box = None;
                    }
                    if self.selected_nodes.capacity() < nodes.len() {
                        let additional =
                            nodes.len() - self.selected_nodes.capacity();
                        self.selected_nodes.reserve(additional);
                    }

                    let (mut top_left, mut bottom_right) = if let Some(bounds) =
                        self.selected_nodes_bounding_box
                    {
                        let old_min = Point {
                            x: bounds.0.x.min(bounds.1.x),
                            y: bounds.0.y.min(bounds.1.y),
                        };

                        let old_max = Point {
                            x: bounds.0.x.max(bounds.1.x),
                            y: bounds.0.y.max(bounds.1.y),
                        };

                        (old_min, old_max)
                    } else {
                        let top_left = Point {
                            x: std::f32::MAX,
                            y: std::f32::MAX,
                        };

                        let bottom_right = Point {
                            x: std::f32::MIN,
                            y: std::f32::MIN,
                        };

                        (top_left, bottom_right)
                    };

                    for &node in nodes.iter() {
                        let pos = node_positions[(node.0 - 1) as usize];

                        let min_x = pos.p0.x.min(pos.p1.x);
                        let min_y = pos.p0.y.min(pos.p1.y);

                        let max_x = pos.p0.x.max(pos.p1.x);
                        let max_y = pos.p0.y.max(pos.p1.y);

                        top_left.x = top_left.x.min(min_x);
                        top_left.y = top_left.y.min(min_y);

                        bottom_right.x = bottom_right.x.max(max_x);
                        bottom_right.y = bottom_right.y.max(max_y);

                        self.selected_nodes.insert(node);
                    }

                    self.selected_nodes_bounding_box =
                        Some((top_left, bottom_right));
                }
            },
            AppMsg::NewNodeLabels { name, label_set } => {
                let label_set_ = label_set.label_set();
                self.labels.add_label_set(
                    self.layout_boundary,
                    node_positions,
                    &name,
                    &label_set_,
                    None,
                );
                self.annotations.insert_label_set(&name, label_set);
            }
            AppMsg::RequestSelection(sender) => {
                let selection = self.selected_nodes.to_owned();
                let rect = self
                    .selected_nodes_bounding_box
                    .map(|(p0, p1)| Rect::new(p0, p1))
                    .unwrap_or(Rect::default());

                sender.send((rect, selection)).unwrap();
            }

            AppMsg::SetData { key, index, value } => {
                self.send_msg(AppMsg::set_data(key, index, value)).unwrap();
            }
            AppMsg::ConsoleEval { script } => {
                console_input_tx.send(script).unwrap();
            }
            AppMsg::Raw {
                type_id,
                msg_name,
                value,
            } => {
                if let Some(handler) = self.msg_handlers.get(&msg_name) {
                    let handler: Arc<_> = handler.clone();
                    if type_id == handler.type_id {
                        handler.call(self, node_positions, value.as_ref());
                    } else {
                        log::warn!(
                            "Type mismatch for AppMsg handler \"{}\"",
                            msg_name
                        );
                    }
                } else {
                    log::warn!("Received unknown AppMsg");
                    log::warn!(
                        "msg_name: {:?}\ntype_id: {:?}\nvalue: {:?}",
                        msg_name,
                        type_id,
                        value
                    );
                }
            }
        }
    }

    fn toggle_dark_mode(&self) {
        let prev = self.shared_state.dark_mode.fetch_xor(true);

        let msg = if prev {
            GuiMsg::SetLightMode
        } else {
            GuiMsg::SetDarkMode
        };

        self.channels.gui_tx.send(msg).unwrap();
    }

    pub fn apply_input(
        &mut self,
        input: SystemInput<AppInput>,
        gui_msg: &Sender<GuiMsg>,
    ) {
        if let SystemInput::Keyboard { state, payload } = input {
            match payload {
                AppInput::KeyClearSelection => {
                    if state.pressed() {
                        self.selection_changed = true;
                        self.selected_nodes.clear();
                        self.selected_nodes_bounding_box = None;
                    }
                }
                AppInput::KeyToggleTheme => {
                    if state.pressed() {
                        self.toggle_dark_mode();
                    }
                }
            }
        }
    }
}

type RefAny<'a> = &'a (dyn std::any::Any + Send + Sync);
type ArcedAny = Arc<dyn std::any::Any + Send + Sync>;
type BoxedAny = Box<dyn std::any::Any + Send + Sync>;

trait HandlerFn<'a>: Fn(&'a mut App, &'a [Node], RefAny<'a>) + Send + Sync {}

impl<'a, T> HandlerFn<'a> for T where
    // T: 'a + Fn(&'a mut App, &'a [Node], BoxedAny) + Send + Sync
    T: 'a + Fn(&'a mut App, &'a [Node], RefAny<'a>) + Send + Sync
{
}

pub struct AppMsgHandler {
    type_id: TypeId,
    handler: Arc<dyn for<'a> HandlerFn<'a>>,
}

impl AppMsgHandler {
    fn call(&self, app: &mut App, nodes: &[Node], input: RefAny<'_>) {
        (self.handler)(app, nodes, input);
    }

    fn from_fn<T, F>(f: F) -> Self
    where
        F: Fn(&mut App, &[Node], &T) + Send + Sync + 'static,
        T: std::any::Any + 'static,
    {
        let type_id = TypeId::of::<T>();

        let handler = Arc::new(
            move |app: &'_ mut App, nodes: &'_ [Node], input: RefAny<'_>| {
                if let Some(arg) = input.downcast_ref::<T>() {
                    f(app, nodes, arg);
                }
            },
        ) as Arc<dyn for<'a> HandlerFn<'a>>;

        AppMsgHandler { type_id, handler }
    }
}

#[derive(FromArgs)]
/// Gfaestus
pub struct Args {
    /// the GFA file to load
    #[argh(positional)]
    pub gfa: String,

    /// the layout file to use
    #[argh(positional)]
    pub layout: String,

    /// load and run a Rhai script file at startup, e.g. for configuration
    #[argh(option)]
    pub run_script: Option<String>,

    #[cfg(target_os = "linux")]
    /// force use of X11 window (only applicable in Wayland contexts)
    #[argh(switch)]
    pub force_x11: bool,

    /// suppress log messages
    #[argh(switch, short = 'q')]
    pub quiet: bool,

    /// log debug messages
    #[argh(switch, short = 'd')]
    pub debug: bool,

    /// log trace-level debug messages
    #[argh(switch)]
    pub trace: bool,

    /*
    /// whether or not to log to a file in the working directory
    #[argh(switch)]
    log_to_file: bool,
    */
    /// if a device name is provided, use that instead of the default graphics device
    #[argh(option)]
    pub force_graphics_device: Option<String>,

    /// path .gff3 and/or .bed file to load at startup, can be used multiple times to load several files
    #[argh(
        option,
        long = "annotation-file",
        from_str_fn(annotation_files_to_str)
    )]
    pub annotation_files: Vec<std::path::PathBuf>,
}

fn annotation_files_to_str(input: &str) -> Result<std::path::PathBuf, String> {
    use std::path::PathBuf;
    println!("parsing annotation file path: {}", input);
    let path = PathBuf::from(input.trim());
    match path.canonicalize() {
        Ok(canon) => Ok(canon),
        Err(err) => Err(format!(
            "Error when parsing annotation file list: {:?}",
            err
        )),
    }
}
