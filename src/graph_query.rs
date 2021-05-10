#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use handlegraph::{
    packedgraph::{paths::StepPtr, PackedGraph},
    path_position::PathPositionMap,
};

use crossbeam::{
    atomic::AtomicCell,
    channel::{self, Receiver},
};

use std::{collections::VecDeque, sync::Arc};

use anyhow::Result;

pub struct AsyncQueryResult<T: 'static + Send + Sync> {
    result: parking_lot::Mutex<T>,
    ready: AtomicCell<bool>,
}

impl<T: 'static + Send + Sync> AsyncQueryResult<T> {
    pub fn is_ready(&self) -> bool {
        self.ready.load()
    }

    pub fn take_result_blocking(self) -> T {
        self.result.into_inner()
    }

    /// If the provided `&mut Option<AsyncQueryResult<T>>` contains an
    /// async result, and that result is ready, replace `result_opt`
    /// with `None` and return the result.
    ///
    /// If the provided value is `None`, or the async result is not
    /// yet ready, returns `None`.
    pub fn take_result_option(result_opt: &mut Option<Self>) -> Option<T> {
        let is_ready = result_opt.as_ref().map(|r| r.is_ready()) == Some(true);

        if is_ready {
            if let Some(result) = result_opt.take() {
                return Some(result.take_result_blocking());
            }
        }

        None
    }
}

// pub struct GraphQueryWorker<'a> {
pub struct GraphQueryWorker {
    _join_handle: std::thread::JoinHandle<()>,
    graph_query: Arc<GraphQuery>,

    work_tx: channel::Sender<Box<dyn FnOnce() + Send + Sync>>,
    work_rx: channel::Receiver<Box<dyn FnOnce() + Send + Sync>>,
}

impl GraphQueryWorker {
    pub fn new(graph_query: Arc<GraphQuery>) -> Self {
        // let (work_tx, work_rx) = channel::unbounded::<Box<dyn Fn(Arc<GraphQuery>) + Send + Sync>>();
        let (work_tx, work_rx) = channel::unbounded::<Box<dyn FnOnce() + Send + Sync>>();

        // let graph_query_ = graph_query.clone();
        let work_rx_ = work_rx.clone();

        let _join_handle = std::thread::spawn(move || {
            // let work_queue: VecDeque<Box<dyn Fn(Arc<GraphQuery>) + Send + Sync>> = VecDeque::new();
            // let work_queue: VecDeque<Box<dyn FnOnce() + Send + Sync>> = VecDeque::new();

            // let graph_query = graph_query_;
            let work_rx = work_rx_;

            while let Ok(work) = work_rx.recv() {
                work();
            }
        });

        Self {
            _join_handle,

            graph_query,
            // work_queue: VecDeque::new(),
            work_tx,
            work_rx,
        }
    }

    pub fn run_query<T, F>(&self, query: F) -> Receiver<T>
    where
        T: 'static + Send + Sync,
        F: Fn(Arc<GraphQuery>) -> T + 'static + Send + Sync,
    {
        let (tx, rx) = channel::bounded::<T>(1);
        let graph_query = self.graph_query.clone();
        let boxed = Box::new(move || {
            let result = query(graph_query);
            let send_result = tx.send(result);
            if let Err(err) = send_result {
                eprintln!("async graph query error: {:?}", err);
            }
        });

        self.work_tx.send(boxed).unwrap();

        rx
    }
}

pub struct GraphQuery {
    graph: Arc<PackedGraph>,
    path_positions: Arc<PathPositionMap>,
    query_thread: QueryThread,
}

impl GraphQuery {
    pub fn load_gfa(gfa_path: &str) -> Result<Self> {
        let mut mmap = gfa::mmap::MmapGFA::new(gfa_path)?;
        let graph = crate::gfa::load::packed_graph_from_mmap(&mut mmap)?;
        let path_positions = PathPositionMap::index_paths(&graph);
        Ok(Self::new(graph, path_positions))
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn new(graph: PackedGraph, path_positions: PathPositionMap) -> Self {
        let graph = Arc::new(graph);
        let path_positions = Arc::new(path_positions);
        let query_thread = QueryThread::new(graph.clone());
        Self {
            graph,
            path_positions,
            query_thread,
        }
    }

    pub fn query_request_blocking(&self, request: GraphQueryRequest) -> GraphQueryResp {
        self.query_thread.request_blocking(request)
    }

    pub fn graph_arc(&self) -> &Arc<PackedGraph> {
        &self.graph
    }

    pub fn graph(&self) -> &PackedGraph {
        &self.graph
    }

    pub fn build_overlay_colors<F>(&self, mut f: F) -> Vec<rgb::RGB<f32>>
    where
        F: FnMut(&PackedGraph, Handle) -> rgb::RGB<f32>,
    {
        let mut result = Vec::with_capacity(self.graph.node_count());

        let mut handles = self.graph.handles().collect::<Vec<_>>();
        handles.sort();

        for handle in handles {
            let color = f(&self.graph, handle);
            result.push(color);
        }

        result
    }

    pub fn handle_positions(&self, handle: Handle) -> Option<Vec<(PathId, StepPtr, usize)>> {
        self.path_positions.handle_positions(&self.graph, handle)
    }
}

struct QueryThread {
    resp_rx: channel::Receiver<GraphQueryResp>,
    req_tx: channel::Sender<GraphQueryRequest>,
    _thread_handle: std::thread::JoinHandle<()>,
}

impl QueryThread {
    fn request_blocking(&self, request: GraphQueryRequest) -> GraphQueryResp {
        self.req_tx.send(request).unwrap();
        self.resp_rx.recv().unwrap()
    }

    fn new(graph: Arc<PackedGraph>) -> Self {
        let (resp_tx, resp_rx) = channel::bounded::<GraphQueryResp>(0);
        let (req_tx, req_rx) = channel::bounded::<GraphQueryRequest>(0);

        let _thread_handle = std::thread::spawn(move || {
            use GraphQueryRequest as Req;
            use GraphQueryResp as Resp;

            use Direction as Dir;

            while let Ok(request) = req_rx.recv() {
                let resp: Resp = match request {
                    Req::GraphStats => Resp::GraphStats {
                        node_count: graph.node_count(),
                        edge_count: graph.edge_count(),
                        path_count: graph.path_count(),
                        total_len: graph.total_length(),
                    },
                    Req::NodeStats(node_id) => {
                        let handle = Handle::pack(node_id, false);

                        let deg_l = graph.degree(handle, Dir::Left);
                        let deg_r = graph.degree(handle, Dir::Right);

                        let coverage: usize = graph
                            .steps_on_handle(handle)
                            .map(|occurs| occurs.count())
                            .unwrap_or(0);

                        Resp::NodeStats {
                            node_id,
                            len: graph.node_len(handle),
                            degree: (deg_l, deg_r),
                            coverage,
                        }
                    }
                    Req::PathStats(path_id) => {
                        let step_count = graph.path_len(path_id).unwrap_or(0);
                        Resp::PathStats {
                            path_id,
                            step_count,
                        }
                    }
                    Req::NodeSeq(node_id) => {
                        let seq = graph.sequence_vec(Handle::pack(node_id, false));
                        let len = seq.len();

                        Resp::NodeSeq { node_id, seq, len }
                    }
                };

                resp_tx.send(resp).unwrap();
            }
        });

        Self {
            resp_rx,
            req_tx,
            _thread_handle,
        }
    }
}

#[derive(Debug, Clone)]
pub enum GraphQueryRequest {
    GraphStats,
    NodeStats(NodeId),
    PathStats(PathId),
    NodeSeq(NodeId),
    // Neighbors(NodeId),
}

#[derive(Debug, Clone)]
pub enum GraphQueryResp {
    GraphStats {
        node_count: usize,
        edge_count: usize,
        path_count: usize,
        total_len: usize,
    },
    NodeStats {
        node_id: NodeId,
        len: usize,
        degree: (usize, usize),
        coverage: usize,
    },
    PathStats {
        path_id: PathId,
        step_count: usize,
    },
    NodeSeq {
        node_id: NodeId,
        seq: Vec<u8>,
        len: usize,
    },
    // Neighbors {
    //     node_id: NodeId,
    //     left: Vec<NodeId>,
    //     right: Vec<NodeId>,
    // },
}
