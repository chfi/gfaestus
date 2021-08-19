use std::{any::TypeId, collections::VecDeque, path::PathBuf, sync::Arc};

use crate::{
    graph_query::GraphQuery, gui::windows::OverlayCreatorMsg,
    overlays::OverlayData, script::ScriptConfig,
};

use crossbeam::channel::{Receiver, Sender, TryRecvError};
use handlegraph::packedgraph::PackedGraph;
use rustc_hash::FxHashMap;

mod paired;

pub use paired::{create_host_pair, Host, Inbox, Outbox, Processor};

use paired::*;

pub struct Reactor {
    thread_pool: futures::executor::ThreadPool,

    processors: Vec<Box<dyn ProcTrait>>,
}

impl Reactor {
    pub fn init(thread_pool: futures::executor::ThreadPool) -> Self {
        Self {
            thread_pool,
            processors: Vec::new(),
        }
    }

    pub fn create_host<F, I, T>(&mut self, func: F) -> Host<I, T>
    where
        T: 'static,
        I: Send + Sync + 'static,
        F: Fn(I) -> T + 'static,
    {
        let boxed_func = Box::new(func) as Box<dyn Fn(I) -> T>;

        let (host, proc) = create_host_pair(boxed_func);

        let processor = Box::new(proc) as Box<dyn ProcTrait>;

        self.processors.push(processor);

        host
    }
}

/*
pub struct ReactorOutput {
    sender: Box<dyn std::any::Any>, // should always be a sender, but this part won't be exposed in the API,
    output_type: TypeId,
}

// pub struct ReactorOutputReceiver<T: Send + Sync + std::any::Any> {
pub struct ReactorOutputReceiver<T: Send + Sync> {
    receiver: Receiver<T>,
    id: ReactorOutputId,
    results: Option<T>,
}

impl<T: Send + Sync> ReactorOutputReceiver<T> {
    pub fn try_recv(&mut self) -> Result<&T, TryRecvError> {
        let result = self.receiver.try_recv()?;
        self.results = Some(result);
        let v = self.results.as_ref().unwrap();
        Ok(v)
    }

    pub fn try_results(&mut self) -> Option<&T> {
        if self.results.is_some() {
            return self.results.as_ref();
        }

        let result = self.receiver.try_recv().ok()?;
        self.results = Some(result);
        self.results.as_ref()
    }

    pub fn results(&self) -> Option<&T> {
        self.results.as_ref()
    }
}

pub struct ReactorOutputTyped<T: Send + Sync + 'static> {
    sender: Box<Sender<T>>,
    output_type: T,
}

pub struct Reactor {
    queue: VecDeque<Message>,
    rayon_pool: rayon::ThreadPool,
    futures_pool: futures::executor::ThreadPool,

    message_in: crossbeam::channel::Receiver<Message>,

    graph: Arc<GraphQuery>,

    outputs: Vec<ReactorOutput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReactorOutputId(pub usize, TypeId);
// pub struct ReactorOutputId(pub usize, pub TypeId);

impl Reactor {
    pub fn add_output_chan<T: Send + Sync + 'static>(
        &mut self,
    ) -> ReactorOutputReceiver<T> {
        use std::any::Any;

        let (sender, receiver) = crossbeam::channel::unbounded::<T>();

        let sender = Box::new(sender) as Box<dyn std::any::Any>;
        let output_type = sender.type_id();

        let next_id = ReactorOutputId(self.outputs.len(), output_type);

        let output = ReactorOutput {
            sender,
            output_type,
        };

        self.outputs.push(output);

        ReactorOutputReceiver {
            receiver,
            id: next_id,
            results: None,
        }
    }

    pub fn verify_output_id<T: Send + Sync + 'static>(
        &self,
        id: ReactorOutputId,
    ) -> bool {
        let ReactorOutputId(ix, type_id) = id;

        let output = &self.outputs[ix];

        output.output_type == type_id
    }
}

#[derive(Clone)]
pub struct Message {
    out: crossbeam::channel::Sender<Box<dyn Package>>,
}

pub struct OverlayMsg {
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

        match overlay_data {
            Ok(data) => {
                ok_chan
                    .send(OverlayCreatorMsg::NewOverlay {
                        name: self.name.to_owned(),
                        data,
                    })
                    .unwrap();
                gui_chan.send("Overlay created".to_string()).unwrap();
            }
            Err(err) => {
                gui_chan.send(format!("Script error: {:?}", err)).unwrap();
            }
        }
    }
}

pub trait Package: Send + Sync {}

*/
