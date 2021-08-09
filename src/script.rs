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
use rustc_hash::FxHashMap;

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

#[derive(Debug, Clone)]
pub enum ScriptTarget {
    Nodes,
    Path { name: String },
}

#[derive(Debug, Clone, Copy)]
pub struct ScriptConfig {
    pub default_color: rgb::RGBA<f32>,
}

pub fn check_overlay_kind(data: rhai::Dynamic) -> Option<OverlayKind> {
    if let Some(_rgb) = data.clone().try_cast::<rgb::RGBA<f32>>() {
        Some(OverlayKind::RGB)
    } else if let Some(_val) = data.clone().try_cast::<f32>() {
        Some(OverlayKind::Value)
    } else {
        None
    }
}

// pub fn check_overlay_kind(data: &Vec<rhai::Dynamic>)
pub fn cast_overlay_data(data: Vec<rhai::Dynamic>) -> Option<OverlayData> {
    let first = data.first()?.clone();

    if let Some(_rgb) = first.clone().try_cast::<rgb::RGBA<f32>>() {
        let data = data
            .into_iter()
            // .filter_map(|v| v.try_cast::<rgb::RGBA<f32>>())
            .map(|v| v.try_cast::<rgb::RGBA<f32>>().unwrap())
            .collect::<Vec<_>>();

        return Some(OverlayData::RGB(data));
    } else if let Some(_val) = first.try_cast::<f32>() {
        let data = data
            .into_iter()
            // .filter_map(|v| v.try_cast::<f32>())
            .map(|v| v.try_cast::<f32>().unwrap())
            .collect::<Vec<_>>();

        return Some(OverlayData::Value(data));
    }

    None
}

pub fn overlay_colors_tgt(
    rayon_pool: &rayon::ThreadPool,
    config: ScriptConfig,
    target: &ScriptTarget,
    graph: &GraphQuery,
    script: &str,
) -> std::result::Result<OverlayData, Box<EvalAltResult>> {
    use rhai::Scope;

    let mut scope = Scope::new();
    scope
        .push("graph", graph.graph.clone())
        .push("path_pos", graph.path_positions.clone());

    let engine = create_engine();

    let node_color_ast = engine.compile(script)?;

    match target {
        ScriptTarget::Nodes => {
            let mut node_ids =
                graph.graph().handles().map(|h| h.id()).collect::<Vec<_>>();
            node_ids.sort();

            let values = rayon_pool.install(|| {
                let mut values: Vec<rhai::Dynamic> =
                    Vec::with_capacity(node_ids.len());
                node_ids
                    .into_par_iter()
                    .map_with(scope, |mut thread_scope, node_id| {
                        let value = engine
                            .call_fn(
                                &mut thread_scope,
                                &node_color_ast,
                                "node_color",
                                (node_id,),
                            )
                            .unwrap();

                        value
                    })
                    .collect_into_vec(&mut values);

                values
            });

            let data = cast_overlay_data(values)
                .ok_or("Couldn't process overlay data")?;

            Ok(data)
        }
        ScriptTarget::Path { name } => {
            let path_id = graph
                .graph()
                .get_path_id(name.as_bytes())
                .ok_or("Path not found")?;

            let steps =
                graph.path_pos_steps(path_id).ok_or("Path not found")?;

            let node_value_map = rayon_pool.install(|| {
                let mut id_values: Vec<(NodeId, rhai::Dynamic)> =
                    Vec::with_capacity(steps.len());

                steps
                    .into_par_iter()
                    .map_with(scope, |mut thread_scope, step| {
                        let (handle, _, pos) = step;
                        let node_id = handle.id();

                        let value = engine
                            .call_fn(
                                &mut thread_scope,
                                &node_color_ast,
                                "node_color",
                                (node_id,),
                            )
                            .unwrap();

                        value
                    })
                    .collect_into_vec(&mut id_values);

                id_values
            });

            let (nodes, values): (Vec<_>, Vec<_>) =
                node_value_map.into_iter().unzip();

            let data = cast_overlay_data(values)
                .ok_or("Couldn't process overlay data")?;

            let mut node_ids =
                graph.graph().handles().map(|h| h.id()).collect::<Vec<_>>();
            node_ids.sort();

            match data {
                OverlayData::RGB(rgb) => {
                    let node_rgb_map: FxHashMap<NodeId, rgb::RGBA<f32>> = nodes
                        .into_iter()
                        .zip(rgb.into_iter())
                        .collect::<FxHashMap<_, _>>();

                    let data = node_ids
                        .into_iter()
                        .map(|node_id| {
                            node_rgb_map
                                .get(&node_id)
                                .copied()
                                .unwrap_or(config.default_color)
                        })
                        .collect();

                    Ok(OverlayData::RGB(data))
                }
                OverlayData::Value(val) => {
                    let node_val_map: FxHashMap<NodeId, f32> = nodes
                        .into_iter()
                        .zip(val.into_iter())
                        .collect::<FxHashMap<_, _>>();

                    let data = node_ids
                        .into_iter()
                        .map(|node_id| {
                            node_val_map
                                .get(&node_id)
                                .copied()
                                .unwrap_or_default()
                        })
                        .collect();

                    Ok(OverlayData::Value(data))
                }
            }
        }
    }
}

/*
pub fn overlay_colors_tgt(
    rayon_pool: &rayon::ThreadPool,
    config: ScriptConfig,
    target: &ScriptTarget,
    graph: &GraphQuery,
    script: &str,
) -> std::result::Result<Vec<rgb::RGBA<f32>>, Box<EvalAltResult>> {
    use rhai::Scope;

    let mut scope = Scope::new();
    scope
        .push("graph", graph.graph.clone())
        .push("path_pos", graph.path_positions.clone());

    let engine = create_engine();

    let node_color_ast = engine.compile(script)?;

    match target {
        ScriptTarget::Nodes => {
            let mut node_ids =
                graph.graph().handles().map(|h| h.id()).collect::<Vec<_>>();
            node_ids.sort();

            let colors = rayon_pool.install(|| {
                // let mut colors: Vec<rgb::RGBA<f32>> =
                //     Vec::with_capacity(node_ids.len());
                // let mut colors: Vec<rgb::RGBA<f32>> =
                let mut colors: Vec<rgb::RGBA<f32>> =
                    Vec::with_capacity(node_ids.len());
                node_ids
                    .into_par_iter()
                    .map_with(scope, |mut thread_scope, node_id| {
                        let color = engine
                            .call_fn(
                                &mut thread_scope,
                                &node_color_ast,
                                "node_color",
                                (node_id,),
                            )
                            .unwrap();

                        color
                    })
                    .collect_into_vec(&mut colors);

                colors
            });

            Ok(colors)
        }
        ScriptTarget::Path { name } => {
            let path_id = graph
                .graph()
                .get_path_id(name.as_bytes())
                .ok_or("Path not found")?;

            let steps =
                graph.path_pos_steps(path_id).ok_or("Path not found")?;

            let node_color_map = rayon_pool.install(|| {
                let mut id_colors: Vec<(NodeId, rgb::RGBA<f32>)> =
                    Vec::with_capacity(steps.len());

                steps
                    .into_par_iter()
                    .map_with(scope, |mut thread_scope, step| {
                        let (handle, _, pos) = step;
                        let node_id = handle.id();

                        let color = engine
                            .call_fn(
                                &mut thread_scope,
                                &node_color_ast,
                                "node_color",
                                (node_id,),
                            )
                            .unwrap();

                        color
                    })
                    .collect_into_vec(&mut id_colors);

                id_colors
            });

            let node_color_map =
                node_color_map.into_iter().collect::<FxHashMap<_, _>>();

            let mut node_ids =
                graph.graph().handles().map(|h| h.id()).collect::<Vec<_>>();
            node_ids.sort();

            let node_colors = node_ids
                .into_iter()
                .map(|node_id| {
                    node_color_map
                        .get(&node_id)
                        .copied()
                        .unwrap_or(config.default_color)
                })
                .collect();

            Ok(node_colors)
        }
    }
}
*/

pub fn overlay_colors(
    rayon_pool: &rayon::ThreadPool,
    graph: &GraphQuery,
    script: &str,
) -> std::result::Result<Vec<rgb::RGBA<f32>>, Box<EvalAltResult>> {
    use rhai::{Func, Scope};

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

    let mut node_ids =
        graph.graph().handles().map(|h| h.id()).collect::<Vec<_>>();
    node_ids.sort();

    let node_color_ast = engine.compile(node_color_script)?;

    use std::any::Any;

    println!("node_color_ast type: {:?}", node_color_ast.type_id());

    let colors = rayon_pool.install(|| {
        let mut colors: Vec<rgb::RGBA<f32>> =
            Vec::with_capacity(node_ids.len());
        node_ids
            .into_par_iter()
            .map_with(scope, |mut thread_scope, node_id| {
                let color = engine
                    .call_fn(
                        &mut thread_scope,
                        &node_color_ast,
                        "node_color",
                        (node_id,),
                    )
                    .unwrap();

                color
            })
            .collect_into_vec(&mut colors);

        colors
    });

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
