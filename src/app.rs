pub mod channels;
pub mod mainview;
pub mod selection;
pub mod settings;
pub mod shared_state;

pub use channels::*;
pub use settings::*;
pub use shared_state::*;

use crossbeam::channel::Sender;

use rustc_hash::{FxHashMap, FxHashSet};

use handlegraph::handle::NodeId;

use anyhow::Result;

use argh::FromArgs;

use std::sync::Arc;

use self::mainview::MainViewMsg;
use crate::annotations::{
    AnnotationCollection, AnnotationLabelSet, Annotations, BedRecords,
    Gff3Records, Labels,
};
use crate::app::selection::NodeSelection;
use crate::gui::GuiMsg;
use crate::view::*;
use crate::{geometry::*, input::binds::SystemInputBindings};
use crate::{
    input::binds::{BindableInput, KeyBind, SystemInput},
    universe::Node,
};

pub struct App {
    shared_state: SharedState,
    channels: AppChannels,
    pub settings: AppSettings,

    selected_nodes: FxHashSet<NodeId>,
    selection_changed: bool,

    pub selected_nodes_bounding_box: Option<(Point, Point)>,

    annotations: Annotations,

    labels: Labels,
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

#[derive(Debug)]
pub enum AppMsg {
    Selection(Select),
    GotoSelection,
    GotoNode(NodeId),

    // TODO these two should not be here (see how they're handled in main)
    RectSelect(Rect),
    TranslateSelected(Point),

    HoverNode(Option<NodeId>),

    ToggleDarkMode,

    AddGff3Records(Gff3Records),
    AddBedRecords(BedRecords),

    NewNodeLabels {
        name: String,
        label_set: AnnotationLabelSet,
    },

    RequestSelection(crossbeam::channel::Sender<(Rect, FxHashSet<NodeId>)>),

    RequestData {
        type_: std::any::TypeId,
        key: String,
        sender: crossbeam::channel::Sender<
            Result<rhai::Dynamic>,
            // std::result::Result<rhai::Dynamic, Box<rhai::EvalAltResult>>,
        >,
        // sender: crossbeam::channel::Sender<Box<dyn std::any::Any + Send + Sync + 'static>,
    },
}

impl App {
    pub fn new<Dims: Into<ScreenDims>>(screen_dims: Dims) -> Result<Self> {
        let shared_state = SharedState::new(screen_dims);

        Ok(Self {
            shared_state,
            channels: AppChannels::new(),

            selected_nodes: FxHashSet::default(),
            selection_changed: false,

            selected_nodes_bounding_box: None,

            settings: AppSettings::default(),

            annotations: Annotations::default(),

            labels: Labels::default(),
        })
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

    pub fn selection_changed(&self) -> bool {
        self.selection_changed
    }

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

    pub fn apply_app_msg(
        &mut self,
        boundary: Rect,
        main_view_msg_tx: &Sender<MainViewMsg>,
        gui_msg: &Sender<GuiMsg>,
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
            AppMsg::GotoSelection => {
                if let Some(bounds) = self.selected_nodes_bounding_box {
                    let view = View::from_dims_and_target(
                        self.dims(),
                        bounds.0,
                        bounds.1,
                    );
                    main_view_msg_tx.send(MainViewMsg::GotoView(view)).unwrap();
                }
            }
            AppMsg::GotoNode(id) => {
                if let Some(node_pos) = node_positions.get((id.0 - 1) as usize)
                {
                    let mut view = self.shared_state.view();
                    view.center = node_pos.center();
                    main_view_msg_tx.send(MainViewMsg::GotoView(view)).unwrap();
                }
            }
            AppMsg::HoverNode(id) => self.shared_state.hover_node.store(id),

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
            AppMsg::AddGff3Records(records) => {
                let file_name = records.file_name().to_string();
                self.annotations.insert_gff3(&file_name, records);
            }
            AppMsg::AddBedRecords(records) => {
                let file_name = records.file_name().to_string();
                self.annotations.insert_bed(&file_name, records);
            }
            AppMsg::NewNodeLabels { name, label_set } => {
                let label_set_ = label_set.label_set();
                self.labels.add_label_set(
                    boundary,
                    node_positions,
                    &name,
                    &label_set_,
                );
                self.annotations.insert_label_set(&name, label_set);
            }
            AppMsg::ToggleDarkMode => {
                self.toggle_dark_mode(gui_msg);
            }
            AppMsg::RequestSelection(sender) => {
                let selection = self.selected_nodes.to_owned();
                let rect = self
                    .selected_nodes_bounding_box
                    .map(|(p0, p1)| Rect::new(p0, p1))
                    .unwrap_or(Rect::default());

                sender.send((rect, selection)).unwrap();
            }

            AppMsg::RequestData { type_, key, sender } => {
                use std::any::TypeId;

                type ReqResult = Result<rhai::Dynamic>;

                let boxed: ReqResult =
                    if type_ == TypeId::of::<Arc<Gff3Records>>() {
                        if let Some(records) = self.annotations.get_gff3(&key) {
                            let result = records.clone();
                            Ok(rhai::Dynamic::from(result))
                        } else {
                            let err = anyhow::anyhow!(
                            "Couldn't find the requested annotation collection"
                        );
                            Err(err) as ReqResult
                        }
                    } else if type_ == TypeId::of::<Arc<BedRecords>>() {
                        if let Some(records) = self.annotations.get_bed(&key) {
                            let result = records.clone();
                            Ok(rhai::Dynamic::from(result))
                        } else {
                            let err = anyhow::anyhow!(
                            "Couldn't find the requested annotation collection"
                        );
                            Err(err) as ReqResult
                        }
                    } else {
                        let err =
                            anyhow::anyhow!("Requested invalid type from App!");
                        Err(err) as ReqResult
                    };

                sender.send(boxed).unwrap();
            }
        }
    }

    fn toggle_dark_mode(&self, gui_msg: &Sender<GuiMsg>) {
        let prev = self.shared_state.dark_mode.fetch_xor(true);

        let msg = if prev {
            GuiMsg::SetLightMode
        } else {
            GuiMsg::SetDarkMode
        };

        gui_msg.send(msg).unwrap();
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
                        self.toggle_dark_mode(gui_msg);
                    }
                }
            }
        }
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
    #[argh(option, from_str_fn(annotation_files_to_str))]
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
