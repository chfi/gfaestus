use std::{collections::VecDeque, path::PathBuf, sync::Arc};

use crate::{
    graph_query::GraphQuery, gui::windows::OverlayCreatorMsg,
    overlays::OverlayData, script::ScriptConfig,
};

use crossbeam::channel::{Receiver, Sender};
use handlegraph::packedgraph::PackedGraph;

pub struct Reactor {
    queue: VecDeque<Message>,
    rayon_pool: rayon::ThreadPool,
    futures_pool: futures::executor::ThreadPool,

    message_in: crossbeam::channel::Receiver<Message>,
}

#[derive(Clone)]
pub struct Message {
    out: crossbeam::channel::Sender<Box<dyn Package>>,
}

pub struct OverlayMsg {
    data: OverlayData,
    name: String,
    script_path: PathBuf,
    config: ScriptConfig,
    graph: Arc<GraphQuery>,
}

impl OverlayMsg {
    pub async fn eval(
        &self,
        rayon_pool: &rayon::ThreadPool,
        ok_chan: Sender<OverlayCreatorMsg>,
        gui_chan: Sender<String>,
    ) {
        use std::io::prelude::*;

        let file = std::fs::File::open(&self.script_path);

        if let Err(err) = file {
            gui_chan.send(err.to_string()).unwrap();
            return;
        }

        let mut file = file.unwrap();

        let mut script = String::new();

        if let Err(err) = file.read_to_string(&mut script) {
            gui_chan.send(err.to_string()).unwrap();
            return;
        }

        let overlay_data = crate::script::overlay_colors_tgt(
            &rayon_pool,
            &self.config,
            self.graph.as_ref(),
            &script,
        );
    }
}

pub trait Package: Send + Sync {}
