use rhai::{Engine, EvalAltResult};

use rayon::prelude::*;

use handlegraph::{
    handle::{Handle, NodeId},
    handlegraph::*,
    pathhandlegraph::*,
};

use rustc_hash::FxHashMap;

use crate::{app::selection::NodeSelection, graph_query::GraphQuery};
use crate::{
    app::AppMsg,
    overlays::{OverlayData, OverlayKind},
};

use rhai::plugin::*;

pub mod plugins;

pub fn create_engine() -> Engine {
    let mut engine = Engine::new();

    engine.register_type::<NodeId>();
    engine.register_type::<Handle>();
    engine.register_type::<PathId>();
    engine.register_type::<NodeSelection>();

    engine.register_fn("to_string", |i: &mut NodeId| i.0.to_string());
    engine.register_fn("to_string", |i: &mut PathId| i.0.to_string());
    engine.register_fn("to_string", |i: &mut Handle| format!("{:?}", i));
    engine.register_fn("to_string", |i: &mut Vec<u8>| {
        use bstr::ByteSlice;
        format!("{}", i.as_bstr())
    });

    let handle = exported_module!(plugins::handle_plugin);
    let graph = exported_module!(plugins::graph_plugin);
    let paths = exported_module!(plugins::paths_plugin);
    let graph_iters = exported_module!(plugins::graph_iters);
    let colors = exported_module!(plugins::colors);
    let selection = exported_module!(plugins::selection);

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    engine.register_fn("finish", |hasher: &mut DefaultHasher| hasher.finish());

    engine.register_fn("hash_array", |a: rhai::Array| {
        let mut hasher = DefaultHasher::default();
        Hash::hash_slice(a.as_slice(), &mut hasher);
        hasher.finish()
    });
    engine.register_fn("hash_dynamic", |d: rhai::Dynamic| {
        let mut hasher = DefaultHasher::default();
        d.hash(&mut hasher);
        hasher.finish()
    });

    engine.register_global_module(handle.into());
    engine.register_global_module(graph.into());
    engine.register_global_module(paths.into());
    engine.register_global_module(graph_iters.into());
    engine.register_global_module(colors.into());

    engine.register_global_module(selection.into());

    engine.register_iterator::<plugins::HandlesIter>();
    engine.register_iterator::<plugins::OccursIter>();
    engine.register_iterator::<plugins::NeighborsIter>();

    macro_rules! unwrap_opt {
        ($type:ty, $default:expr) => {
            |opt: Option<$type>| opt.unwrap_or($default)
        };
    }

    macro_rules! unwrap_opt_with {
        ($type:ty) => {
            |opt: Option<$type>, or: $type| opt.unwrap_or(or)
        };
    }

    engine.register_fn("unwrap_opt", unwrap_opt!(i32, 0));
    engine.register_fn("unwrap_opt", unwrap_opt!(i64, 0));
    engine.register_fn("unwrap_opt", unwrap_opt!(f32, 0.0));
    engine.register_fn("unwrap_opt", unwrap_opt!(f64, 0.0));

    engine.register_fn("unwrap_opt_with", unwrap_opt_with!(i32));
    engine.register_fn("unwrap_opt_with", unwrap_opt_with!(i64));
    engine.register_fn("unwrap_opt_with", unwrap_opt_with!(f32));
    engine.register_fn("unwrap_opt_with", unwrap_opt_with!(f64));
    engine.register_fn("unwrap_opt_with", unwrap_opt_with!(rhai::Dynamic));

    engine.register_fn("print_handle", |h: Handle| {
        let suffix = if h.is_reverse() { "-" } else { "+" };
        println!("Handle {}{}", h.id().0, suffix);
    });

    engine.register_fn("thread_sleep", |ms: i64| {
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
    });

    engine
}

#[derive(Debug, Clone)]
pub enum ScriptTarget {
    Nodes,
    Path { name: String },
}

#[derive(Debug, Clone)]
pub struct ScriptConfig {
    pub default_color: rgb::RGBA<f32>,
    pub target: ScriptTarget,
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
        let mut min = std::f32::MAX;
        let mut max = std::f32::MIN;

        let mut data = data
            .into_iter()
            // .filter_map(|v| v.try_cast::<f32>())
            .map(|v| {
                let x = v.try_cast::<f32>().unwrap();
                min = min.min(x);
                max = max.max(x);
                x
            })
            .collect::<Vec<_>>();

        let range = max - min;

        for val in data.iter_mut() {
            *val = (*val - min) / range;
        }

        log::debug!("Overlay values, min: {}, max: {}", min, max);

        return Some(OverlayData::Value(data));
    }

    None
}

pub fn overlay_colors_tgt_ast(
    rayon_pool: &rayon::ThreadPool,
    config: &ScriptConfig,
    graph: &GraphQuery,
    engine: &rhai::Engine,
    scope: rhai::Scope<'_>,
    node_color_ast: rhai::AST,
) -> std::result::Result<OverlayData, Box<EvalAltResult>> {
    match config.target.clone() {
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
                        let (handle, _, _pos) = step;
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

            let default_color = config.default_color;

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
                                .unwrap_or(default_color)
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

pub fn overlay_colors_tgt(
    rayon_pool: &rayon::ThreadPool,
    config: &ScriptConfig,
    graph: &GraphQuery,
    script: &str,
) -> std::result::Result<OverlayData, Box<EvalAltResult>> {
    use rhai::Scope;

    let mut scope = Scope::new();
    scope
        .push("graph", graph.graph.clone())
        .push("path_pos", graph.path_positions.clone());

    let mut engine = create_engine();

    let graph_ = graph.graph.clone();

    engine.register_fn("get_graph", move || graph_.clone());

    let node_color_ast = engine.compile(script)?;

    match config.target.clone() {
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
                        let (handle, _, _pos) = step;
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

            let default_color = config.default_color;

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
                                .unwrap_or(default_color)
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
