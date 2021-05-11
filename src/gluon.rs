use std::{path::Path, sync::Arc};

use bstr::ByteVec;
use gluon_codegen::*;

use gluon::vm::{
    api::{FunctionRef, Hole, OpaqueValue},
    ExternModule,
};
use gluon::*;
use gluon::{base::types::ArcType, import::add_extern_module};

use anyhow::Result;

use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    packedgraph::index::OneBasedIndex,
    pathhandlegraph::*,
};

use handlegraph::{
    packedgraph::{paths::StepPtr, PackedGraph},
    path_position::PathPositionMap,
};

use crate::vulkan::draw_system::nodes::overlay::NodeOverlay;

pub mod bed;

pub struct GluonVM {
    vm: RootedThread,
}

pub type RGBTuple = (f32, f32, f32, f32);

impl GluonVM {
    pub fn new() -> Result<Self> {
        let vm = new_vm();
        gluon::import::add_extern_module(&vm, "gfaestus", packedgraph_module);
        gluon::import::add_extern_module(&vm, "bed", bed::bed_module);

        vm.run_expr::<OpaqueValue<&Thread, Hole>>("", "import! gfaestus")?;

        Ok(Self { vm })
    }

    pub fn run_overlay_expr(&self, expr_str: &str) -> Result<Vec<RGBTuple>> {
        self.vm.run_io(true);
        let (res, _arc) = self.vm.run_expr("overlay_expr", expr_str)?;
        self.vm.run_io(false);
        match res {
            vm::api::IO::Value(v) => Ok(v),
            vm::api::IO::Exception(err) => {
                anyhow::bail!(err)
            }
        }
    }

    pub fn load_overlay_expr(
        &self,
        script_path: &Path,
    ) -> Result<Vec<RGBTuple>> {
        use std::{fs::File, io::Read};

        let mut file = File::open(script_path)?;
        let mut source = String::new();
        file.read_to_string(&mut source)?;

        self.vm.run_io(true);
        let (res, _arc) = self.vm.run_expr("overlay_expr", &source)?;
        self.vm.run_io(false);
        match res {
            vm::api::IO::Value(v) => Ok(v),
            vm::api::IO::Exception(err) => {
                anyhow::bail!(err)
            }
        }
    }

    pub fn load_overlay_per_node_expr(
        &self,
        graph: &GraphHandle,
        script_path: &Path,
    ) -> Result<Vec<rgb::RGB<f32>>> {
        use std::{fs::File, io::Read};

        let mut file = File::open(script_path)?;
        let mut source = String::new();
        file.read_to_string(&mut source)?;

        let node_count = graph.graph.node_count();

        let (mut node_color, _): (
            FunctionRef<fn(GraphHandle, u64) -> (f32, f32, f32)>,
            _,
        ) = self.vm.run_expr("node_color_fun", &source)?;

        let mut colors: Vec<rgb::RGB<f32>> = Vec::with_capacity(node_count);

        for node_id in 0..node_count {
            let node_id = (node_id + 1) as u64;
            let (r, g, b) = node_color.call(graph.clone(), node_id)?;

            colors.push(rgb::RGB::new(r, g, b));
        }

        Ok(colors)
    }

    pub fn load_overlay_per_node_expr_io(
        &self,
        graph: &GraphHandle,
        script_path: &Path,
    ) -> Result<Vec<rgb::RGB<f32>>> {
        use std::{fs::File, io::Read};

        let mut file = File::open(script_path)?;
        let mut source = String::new();
        file.read_to_string(&mut source)?;

        let node_count = graph.graph.node_count();

        dbg!();

        self.vm.run_io(true);
        let (node_color, _): (
            vm::api::IO<FunctionRef<fn(GraphHandle, u64) -> (f32, f32, f32)>>,
            _,
        ) = self.vm.run_expr("node_color_fun", &source)?;

        dbg!();
        let mut colors: Vec<rgb::RGB<f32>> = Vec::with_capacity(node_count);
        dbg!();

        let mut node_color = match node_color {
            vm::api::IO::Value(v) => v,
            vm::api::IO::Exception(err) => anyhow::bail!(err),
        };

        for node_id in 0..node_count {
            let node_id = (node_id + 1) as u64;
            let (r, g, b) = node_color.call(graph.clone(), node_id)?;

            colors.push(rgb::RGB::new(r, g, b));
        }

        self.vm.run_io(false);

        Ok(colors)
    }

    pub fn test_graph_handle(&self, graph: &GraphHandle) {
        let script = r#"
let gfaestus = import! gfaestus
gfaestus.node_count
"#;

        let (mut node_count_fn, _) = self
            .vm
            .run_expr::<FunctionRef<fn(GraphHandle) -> usize>>(
                "node_count_test",
                script,
            )
            .unwrap();

        let node_count = node_count_fn.call(graph.clone()).unwrap();

        println!("gluon node count: {}", node_count);
    }

    pub fn example_overlay(
        &self,
        graph: &GraphHandle,
    ) -> gluon::Result<Vec<rgb::RGB<f32>>> {
        let script = r#"
let gfaestus = import! gfaestus
let node_color g x = gfaestus.hash_node_color (gfaestus.hash_node_paths g x)
node_color
"#;

        let node_count = graph.graph.node_count();

        let (mut node_color, _): (
            FunctionRef<fn(GraphHandle, u64) -> (f32, f32, f32)>,
            _,
        ) = self.vm.run_expr("node_color_fun", script)?;

        let mut colors: Vec<rgb::RGB<f32>> = Vec::with_capacity(node_count);

        for node_id in 0..node_count {
            let node_id = (node_id + 1) as u64;
            let (r, g, b) = node_color.call(graph.clone(), node_id)?;

            colors.push(rgb::RGB::new(r, g, b));
        }

        Ok(colors)
    }
}

#[derive(Debug, Clone, Trace, Userdata, VmType)]
#[gluon_userdata(clone)]
#[gluon_trace(skip)]
#[gluon(vm_type = "GraphHandle")]
pub struct GraphHandle {
    graph: Arc<PackedGraph>,
    path_pos: Arc<PathPositionMap>,
}

impl GraphHandle {
    pub fn new(
        graph: Arc<PackedGraph>,
        path_pos: Arc<PathPositionMap>,
    ) -> Self {
        Self { graph, path_pos }
    }
}

/*
impl gluon::vm::api::VmType for GraphHandle {
    type Type = Self;

    fn make_type(thread: &Thread) -> ArcType {
        thread
            .find_type_info("GraphHandle")
            .unwrap_or_else(|err| panic!("{}", err))
            .clone()
            .into_type()
    }
}
*/

fn node_count(graph: &GraphHandle) -> usize {
    graph.graph.node_count()
}

fn edge_count(graph: &GraphHandle) -> usize {
    graph.graph.edge_count()
}

fn path_count(graph: &GraphHandle) -> usize {
    graph.graph.path_count()
}

fn sequence(graph: &GraphHandle, node_id: u64, reverse: bool) -> String {
    let seq = graph.graph.sequence_vec(Handle::pack(node_id, reverse));
    seq.into_string_lossy()
}
fn node_len(graph: &GraphHandle, node_id: u64) -> usize {
    graph.graph.node_len(Handle::pack(node_id, false))
}

fn degree(graph: &GraphHandle, node_id: u64, reverse: bool) -> (usize, usize) {
    let handle = Handle::pack(node_id, reverse);
    let left = graph.graph.degree(handle, Direction::Left);
    let right = graph.graph.degree(handle, Direction::Right);
    (left, right)
}

fn has_node(graph: &GraphHandle, node_id: u64) -> bool {
    graph.graph.has_node(node_id)
}

fn is_path_on_node(graph: &GraphHandle, path_id: u64, node_id: u64) -> bool {
    if let Some(mut steps) =
        graph.graph.steps_on_handle(Handle::pack(node_id, false))
    {
        steps.any(|(path, _)| path.0 == path_id)
    } else {
        false
    }
}

fn path_len(graph: &GraphHandle, path_id: u64) -> Option<usize> {
    graph.graph.path_len(PathId(path_id))
}

fn get_path_id(graph: &GraphHandle, path_name: &[u8]) -> Option<u64> {
    graph.graph.get_path_id(path_name).map(|p| p.0)
}

fn get_path_id_str(graph: &GraphHandle, path_name: &str) -> Option<u64> {
    graph.graph.get_path_id(path_name.as_bytes()).map(|p| p.0)
}

fn path_range(
    graph: &GraphHandle,
    path_id: u64,
    start: u64,
    end: u64,
) -> Option<Vec<(u64, u64, usize)>> {
    let path_steps = graph.graph.path_steps_range(
        PathId(path_id),
        StepPtr::from_zero_based(start as usize),
        StepPtr::from_zero_based(end as usize),
    )?;

    let mut result = Vec::new();

    for step in path_steps {
        let step_ptr = step.0;
        let handle = step.handle();

        let base_pos = graph
            .path_pos
            .path_step_position(PathId(path_id), step_ptr)
            .unwrap();

        result.push((handle.id().0, step_ptr.to_vector_value(), base_pos));
    }

    Some(result)
}

fn hash_node_seq(graph: &GraphHandle, node_id: u64) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::default();
    let seq = graph.graph.sequence_vec(Handle::pack(node_id, false));
    seq.hash(&mut hasher);
    hasher.finish()
}

fn hash_node_paths(graph: &GraphHandle, node_id: u64) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    if let Some(steps) =
        graph.graph.steps_on_handle(Handle::pack(node_id, false))
    {
        let mut hasher = DefaultHasher::default();

        for (path, _) in steps {
            path.hash(&mut hasher);
        }

        hasher.finish()
    } else {
        0
    }
}

fn hash_node_color(hash: u64) -> (f32, f32, f32) {
    let r_u16 = ((hash >> 32) & 0xFFFFFFFF) as u16;
    let g_u16 = ((hash >> 16) & 0xFFFFFFFF) as u16;
    let b_u16 = (hash & 0xFFFFFFFF) as u16;

    let max = r_u16.max(g_u16).max(b_u16) as f32;
    let r = (r_u16 as f32) / max;
    let g = (g_u16 as f32) / max;
    let b = (b_u16 as f32) / max;
    (r, g, b)
}

fn packedgraph_module(thread: &Thread) -> vm::Result<ExternModule> {
    thread.register_type::<GraphHandle>("GraphHandle", &[])?;

    let module = record! {
        type GraphHandle => GraphHandle,

        get_path_id => primitive!(2, get_path_id),
        get_path_id_str => primitive!(2, get_path_id_str),
        path_range => primitive!(4, path_range),

        node_count => primitive!(1, node_count),
        edge_count => primitive!(1, edge_count),
        path_count => primitive!(1, path_count),
        has_node => primitive!(2, has_node),

        sequence => primitive!(3, sequence),
        node_len => primitive!(2, node_len),
        degree => primitive!(3, degree),
        is_path_on_node => primitive!(3, is_path_on_node),

        path_len => primitive!(2, path_len),

        hash_node_seq => primitive!(2, hash_node_seq),
        hash_node_paths => primitive!(2, hash_node_paths),

        hash_node_color => primitive!(1, hash_node_color),
    };

    ExternModule::new(thread, module)
}
