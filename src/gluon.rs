use std::sync::Arc;

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
    pathhandlegraph::*,
};

use handlegraph::{
    packedgraph::{paths::StepPtr, PackedGraph},
    path_position::PathPositionMap,
};

use crate::vulkan::draw_system::nodes::overlay::NodeOverlay;

pub struct GluonVM {
    vm: RootedThread,
}

pub type RGBTuple = (f32, f32, f32, f32);

impl GluonVM {
    pub fn new() -> Result<Self> {
        let vm = new_vm();
        gluon::import::add_extern_module(&vm, "gfaestus", packedgraph_module);

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

    pub fn test_graph_handle(&self, graph: &GraphHandle) {
        let script = r#"
let gfaestus = import! gfaestus
gfaestus.node_count
"#;

        let (mut node_count_fn, _) = self
            .vm
            .run_expr::<FunctionRef<fn(GraphHandle) -> usize>>("node_count_test", script)
            .unwrap();

        let node_count = node_count_fn.call(graph.clone()).unwrap();

        println!("gluon node count: {}", node_count);
    }

    pub fn example_overlay(&self, graph: &GraphHandle) -> gluon::Result<Vec<rgb::RGB<f32>>> {
        //         let script = r#"
        // let int = import! std.int

        // let color_fn node_id =
        //     if int.rem node_id 2 == 0 then (0.5, 0.5, 0.5) else (0.8, 0.3, 0.3)
        // color_fn
        // "#;
        let script = r#"
let gfaestus = import! gfaestus
gfaestus.hash_node_color
"#;

        let node_count = graph.graph.node_count();

        let (mut node_color, _): (FunctionRef<fn(GraphHandle, u64) -> (f32, f32, f32)>, _) =
            self.vm.run_expr("node_color_fun", script)?;

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
}

impl GraphHandle {
    pub fn new(graph: Arc<PackedGraph>) -> Self {
        Self { graph }
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

fn sequence_len(graph: &GraphHandle, node_id: u64) -> usize {
    graph.graph.node_len(Handle::pack(node_id, false))
}

fn hash_node_seq(graph: &GraphHandle, node_id: u64) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::default();
    let seq = graph.graph.sequence_vec(Handle::pack(node_id, false));
    seq.hash(&mut hasher);
    hasher.finish()
}

fn hash_node_color(graph: &GraphHandle, node_id: u64) -> (f32, f32, f32) {
    let hash = hash_node_seq(graph, node_id);
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
        node_count => primitive!(1, node_count),
        sequence_len => primitive!(2, sequence_len),
        hash_node => primitive!(2, hash_node_seq),
        hash_node_color => primitive!(2, hash_node_color),
    };

    ExternModule::new(thread, module)
}
