use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::{
    any::TypeId,
    collections::{hash_map::DefaultHasher, HashMap},
    hash::Hasher,
    sync::Arc,
};

use crate::{app::App, universe::Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WindowRoot {
    Main,
    Auxiliary { id: usize },
}

struct EguiWindowCfg {
    builder: Box<dyn Fn() -> egui::Window<'static>>,
    id: egui::Id,
}

impl EguiWindowCfg {
    fn show_with_open<F>(&self, ctx: &egui::CtxRef, open: &mut bool, render: F)
    where
        F: FnOnce(&mut egui::Ui),
    {
        let window = (self.builder)();
        window.open(open).show(ctx, render);
    }

    fn show_with_atomic_open<F>(
        &self,
        ctx: &egui::CtxRef,
        cell: &AtomicCell<bool>,
        render: F,
    ) where
        F: FnOnce(&mut egui::Ui),
    {
        let mut open = cell.load();
        let window = (self.builder)();
        window.open(&mut open).show(ctx, render);
        cell.store(open);
    }
}

// impl std::default::Default

pub struct WindowCfg {
    // root: WindowRoot,
    title: String,

    egui_window_cfg: EguiWindowCfg,
}

pub struct WrapWin {
    //
    id: egui::Id,

    title: String,

    show: Box<dyn FnMut(&App, &mut egui::Ui, &[Node]) + Send + Sync>,
}

/*
pub trait AnySendSync: std::any::Any + Send + Sync + 'static {}
impl<T: std::any::Any + Send + Sync + 'static> AnySendSync for T {}

pub struct WrapWinT<T: AnySendSync> {
    id: egui::Id,

    title: String,

    type_id: TypeId,

    show: Box<dyn Fn(&mut egui::Ui, &mut Box<T>) + Send + Sync>,

    window_state: Box<T>,
}

pub struct WrapWin_ {
    id: egui::Id,

    title: String,

    type_id: TypeId,

    show: Box<dyn Fn(&mut egui::Ui, &mut Box<dyn AnySendSync>) + Send + Sync>,

    window_state: Box<dyn AnySendSync>,
    // show: Box<dyn FnMut(&mut egui::Ui, &mut Box<dyn std::any::Any + Send + Sync>) + Send + Sync>,
}

impl<T: AnySendSync> WrapWinT<T> {
    pub fn new<F>(title: &str, show: F, state: T) -> Self
    where
        F: Fn(&mut egui::Ui, &mut T) + Send + Sync,
    {
        unimplemented!();
    }
}
*/

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GuiId(u64);

impl GuiId {
    pub fn new(id: impl std::hash::Hash) -> Self {
        let mut hasher = DefaultHasher::default();
        id.hash(&mut hasher);
        GuiId(hasher.finish())
    }
}

/*
pub struct GuiChn<T: std::any::Any + Send + Sync + 'static> {
    id: GuiId,
    type_id: TypeId,
    _type: std::marker::PhantomData<T>,

    tx: crossbeam::channel::Sender<T>,
    rx: crossbeam::channel::Receiver<T>,
}
*/

#[derive(Debug, Clone)]
pub struct GuiChnInfo<T: std::any::Any + Send + Sync + 'static> {
    id: GuiId,
    type_id: TypeId,
    _type: std::marker::PhantomData<T>,

    tx: crossbeam::channel::Sender<T>,
    rx: crossbeam::channel::Receiver<T>,
}

pub struct GuiChannels {
    channel_types: FxHashMap<GuiId, TypeId>,

    tx_channels: FxHashMap<GuiId, Box<dyn std::any::Any>>,
    rx_channels: FxHashMap<GuiId, Box<dyn std::any::Any>>,
    // clone_tx: FxHashMap<GuiId, Arc<dyn Fn() -> Box<dyn std::any::Any>>>,
    // clone_rx: FxHashMap<GuiId, Arc<dyn Fn() -> Box<dyn std::any::Any>>>,
}

impl GuiChannels {
    pub fn new() -> Self {
        let channel_types = FxHashMap::default();

        let tx_channels = FxHashMap::default();
        let rx_channels = FxHashMap::default();

        Self {
            channel_types,
            tx_channels,
            rx_channels,
        }
    }

    pub fn get_rx<T>(
        &self,
        id: GuiId,
    ) -> Option<&crossbeam::channel::Receiver<T>>
    where
        T: std::any::Any + Send + Sync + 'static,
    {
        let chn_type_id = *self.channel_types.get(&id)?;

        let t_type_id = TypeId::of::<T>();

        if chn_type_id != t_type_id {
            return None;
        }

        let rx_ = self.rx_channels.get(&id)?;
        rx_.downcast_ref()
    }

    pub fn get_tx<T>(&self, id: GuiId) -> Option<&crossbeam::channel::Sender<T>>
    where
        T: std::any::Any + Send + Sync + 'static,
    {
        let chn_type_id = *self.channel_types.get(&id)?;

        let t_type_id = TypeId::of::<T>();

        if chn_type_id != t_type_id {
            return None;
        }

        let tx_ = self.tx_channels.get(&id)?;
        tx_.downcast_ref()
    }

    pub fn has_channel<T>(&self, id: GuiId) -> bool
    where
        T: std::any::Any + Send + Sync + 'static,
    {
        if let Some(type_id) = self.channel_types.get(&id) {
            let t_type_id = TypeId::of::<T>();
            return *type_id == t_type_id;
        }

        false
    }

    // if returns Err(TypeId) if a channel with that id already
    // existed, where TypeId is that of the existing channel
    pub fn add_channel<T>(
        &mut self,
        id: GuiId,
        bounded: Option<usize>,
    ) -> std::result::Result<GuiChnInfo<T>, TypeId>
    where
        T: std::any::Any + Send + Sync + 'static,
    {
        if let Some(type_id) = self.channel_types.get(&id) {
            return Err(*type_id);
        }

        let (tx, rx) = if let Some(n) = bounded {
            crossbeam::channel::bounded::<T>(n)
        } else {
            crossbeam::channel::unbounded::<T>()
        };

        let t_type_id = TypeId::of::<T>();

        self.channel_types.insert(id, t_type_id);
        self.tx_channels.insert(id, Box::new(tx.clone()) as _);
        self.rx_channels.insert(id, Box::new(rx.clone()) as _);

        let info = GuiChnInfo {
            id,
            type_id: t_type_id,
            tx,
            rx,
            _type: std::marker::PhantomData,
        };

        Ok(info)
    }

    // fn has_rx<T: std::any::Any + 'static>(&self, id: GuiId) -> Option<bool> {

    //     unimplemented!();
    // }
}

#[derive(Default)]
pub struct GuiWindows {
    windows: FxHashMap<GuiId, Arc<Mutex<WrapWin>>>,

    open_windows: FxHashMap<GuiId, Arc<AtomicCell<bool>>>,
}

impl GuiWindows {
    pub fn open_windows(&self) -> Vec<GuiId> {
        self.open_windows
            .iter()
            .filter_map(|(id, v)| if v.load() { Some(*id) } else { None })
            .collect()
    }

    // pub fn show_in(&self, id: GuiId, ui: &mut Ui) -> Option<()> {
    pub fn show_in_window(
        &self,
        app: &App,
        ctx: &egui::CtxRef,
        nodes: &[Node],
        id: GuiId,
        window: egui::Window,
    ) -> Option<()> {
        let cell = self.open_windows.get(&id)?;
        let mut open = cell.load();
        let w = self.windows.get(&id)?;

        {
            let mut lock = w.lock();

            window.open(&mut open).show(ctx, |ui| {
                (lock.show)(app, ui, nodes);
            });
        }

        cell.store(open);

        Some(())
    }

    pub fn add_window<F>(&mut self, id: GuiId, title: &str, f: F)
    where
        F: FnMut(&App, &mut egui::Ui, &[Node]) + Send + Sync + 'static,
    {
        let wrap_win = WrapWin {
            id: egui::Id::new(id),
            title: title.to_string(),
            show: Box::new(f) as _,
        };

        self.windows.insert(id, Arc::new(Mutex::new(wrap_win)));
        self.open_windows.insert(id, Arc::new(false.into()));
    }

    pub fn is_open(&self, id: GuiId) -> bool {
        self.open_windows
            .get(&id)
            .map(|c| c.load())
            .unwrap_or(false)
    }

    pub fn get_open_arc(&self, id: GuiId) -> Option<&Arc<AtomicCell<bool>>> {
        self.open_windows.get(&id)
    }

    pub fn set_open(&self, id: GuiId, open: bool) {
        if let Some(o) = self.open_windows.get(&id) {
            o.store(open);
        }
    }

    pub fn toggle_open(&self, id: GuiId) {
        if let Some(o) = self.open_windows.get(&id) {
            o.fetch_xor(true);
        }
    }
}

/*
pub struct AppViewState {
    settings: SettingsWindow,
    fps: ViewStateChannel<FrameRate, FrameRateMsg>,

    graph_stats: ViewStateChannel<GraphStats, GraphStatsMsg>,

    node_list: ViewStateChannel<NodeList, NodeListMsg>,
    node_details: ViewStateChannel<NodeDetails, NodeDetailsMsg>,

    path_list: ViewStateChannel<PathList, PathListMsg>,
    path_details: ViewStateChannel<PathDetails, ()>,

    // theme_editor: ThemeEditor,
    // theme_list: ThemeList,
    overlay_creator: ViewStateChannel<OverlayCreator, OverlayCreatorMsg>,
    overlay_list: ViewStateChannel<OverlayList, OverlayListMsg>,
}
*/

// impl GuiWindows {
//     pub fn (
// }

/*


*/
