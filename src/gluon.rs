use std::sync::Arc;

use gluon_codegen::*;

use gluon::vm::{api::FunctionRef, ExternModule};
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
        packedgraph_module(&vm)?;
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

    pub fn example_overlay(&self, node_count: usize) -> gluon::Result<Vec<rgb::RGB<f32>>> {
        let script = r#"
let int = import! std.int

let color_fn node_id =
    if int.rem node_id 2 == 0 then (0.5, 0.5, 0.5) else (0.8, 0.3, 0.3)
color_fn
"#;

        let (mut node_color, _): (FunctionRef<fn(u32) -> (f32, f32, f32)>, _) =
            self.vm.run_expr("node_color_fun", script)?;

        let mut colors: Vec<rgb::RGB<f32>> = Vec::with_capacity(node_count);

        for node_id in 0..node_count {
            let node_id = node_id as u32;
            let (r, g, b) = node_color.call(node_id)?;

            colors.push(rgb::RGB::new(r, g, b));
        }

        Ok(colors)
    }
}

#[derive(Debug, Trace, Userdata)]
#[gluon_trace(skip)]
pub struct GraphHandle {
    graph: Arc<PackedGraph>,
}

impl gluon::vm::api::VmType for GraphHandle {
    type Type = Self;

    fn make_type(thread: &Thread) -> ArcType {
        thread
            .find_type_info("packedgraph.PackedGraph")
            .unwrap_or_else(|err| panic!("{}", err))
            .clone()
            .into_type()
    }
}

fn node_count(graph: &GraphHandle) -> usize {
    graph.graph.node_count()
}

fn sequence_len(graph: &GraphHandle, node_id: u64) -> usize {
    graph.graph.node_len(Handle::pack(node_id, false))
}

fn packedgraph_module(thread: &Thread) -> vm::Result<ExternModule> {
    // thread.register_type::<PackedGraph>("PackedGraph", &[])?;

    // type PackedGraph => PackedGraph,
    let module = record! {
        type GraphHandle => GraphHandle,
        node_count => primitive!(1, node_count),
        sequence_len => primitive!(2, sequence_len),
    };

    ExternModule::new(thread, module)
}
