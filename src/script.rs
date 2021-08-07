use rhai::{Engine, EvalAltResult, INT};

use anyhow::Result;

use rayon::prelude::*;

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

use std::{path::Path, sync::Arc};

use bstr::ByteVec;
use futures::Future;

use bytemuck::{Contiguous, Pod, Zeroable};

use crate::vulkan::draw_system::nodes::overlay::NodeOverlay;

use crate::graph_query::GraphQuery;
use crate::overlays::{OverlayData, OverlayKind};

pub fn create_engine() -> Engine {
    let mut engine = Engine::new();

    engine.register_type::<NodeId>();
    engine.register_type::<Handle>();

    engine.register_fn("node_count", |g: &mut Arc<PackedGraph>| {
        g.node_count() as i64
    });

    engine.register_fn("edge_count", |g: &mut Arc<PackedGraph>| {
        g.edge_count() as i64
    });

    engine.register_fn(
        "get_path_id",
        |g: &mut Arc<PackedGraph>, path_name: &str| {
            g.get_path_id(path_name.as_bytes())
        },
    );

    engine.register_fn(
        "sequence",
        |g: &mut Arc<PackedGraph>, node_id: NodeId| {
            g.sequence_vec(Handle::new(node_id, gfa::gfa::Orientation::Forward))
        },
    );

    engine.register_fn("node_id", |id: i64| NodeId::from(id as u64));

    engine.register_fn("hash_bytes", |bytes: &mut Vec<u8>| {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::default();
        bytes.hash(&mut hasher);
        let hash = hasher.finish();
        let result: i64 = bytemuck::cast(hash);
        result
    });

    engine.register_fn("hash_color", |hash: i64| {
        let hash: u64 = bytemuck::cast(hash);

        let r_u16 = ((hash >> 32) & 0xFFFFFFFF) as u16;
        let g_u16 = ((hash >> 16) & 0xFFFFFFFF) as u16;
        let b_u16 = (hash & 0xFFFFFFFF) as u16;

        let max = r_u16.max(g_u16).max(b_u16) as f32;
        let r = (r_u16 as f32) / max;
        let g = (g_u16 as f32) / max;
        let b = (b_u16 as f32) / max;
        rgb::RGBA::new(r, g, b, 1.0)
    });

    engine
}

pub fn overlay_colors(
    graph: &GraphQuery,
    script: &str,
) -> std::result::Result<Vec<rgb::RGBA<f32>>, Box<EvalAltResult>> {
    use rhai::Scope;

    let mut scope = Scope::new();
    scope
        .push("graph", graph.graph.clone())
        .push("path_pos", graph.path_positions.clone());

    let engine = create_engine();

    let node_color_script = "
fn node_color(id) {
  let seq = graph.sequence(id);
  let hash = hash_bytes(seq);
  let color = hash_color(hash);
  color
}
";

    let node_color_ast = engine.compile(node_color_script)?;

    let mut colors: Vec<rgb::RGBA<f32>> =
        Vec::with_capacity(graph.node_count());

    let mut node_ids =
        graph.graph().handles().map(|h| h.id()).collect::<Vec<_>>();
    node_ids.sort();

    for id in node_ids {
        let color =
            engine.call_fn(&mut scope, &node_color_ast, "node_color", (id,))?;
        colors.push(color);
    }
    /*

        let node_count = engine.eval_with_scope::<Vec<u8>>(
            &mut scope,
            "
    let seq = graph.sequence(node_id(5));
    seq
    ",
        );

        println!("eval results: {:?}", node_count);

        let color = engine.eval_with_scope::<rgb::RGBA<f32>>(
            &mut scope,
            "
    let seq = graph.sequence(node_id(5));
    let hash = hash_bytes(seq);
    let color = hash_color(hash);
    color
    ",
        );

        println!("eval results: {:?}", color);
        */

    Ok(colors)
}

pub fn hash_node_seq(graph: &GraphQuery, node_id: NodeId) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::default();
    let seq = graph.graph().sequence_vec(Handle::pack(node_id, false));
    seq.hash(&mut hasher);
    hasher.finish()
}

pub fn hash_node_paths(graph: &GraphQuery, node_id: NodeId) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    if let Some(steps) =
        graph.graph().steps_on_handle(Handle::pack(node_id, false))
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
