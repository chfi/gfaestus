use std::{collections::HashMap, io::BufReader, path::PathBuf, sync::Arc};

use crossbeam::atomic::AtomicCell;

use futures::{task::SpawnExt, Future, FutureExt, StreamExt};
#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use rustc_hash::FxHashMap;

use bstr::ByteSlice;

use crate::{
    annotations::{
        path_name_range, AnnotationCollection, AnnotationRecord, BedColumn,
        BedRecord, BedRecords, LabelSet,
    },
    app::{AppChannels, AppMsg, OverlayCreatorMsg, SharedState},
    graph_query::GraphQuery,
    overlays::OverlayData,
    reactor::{ModalError, ModalHandler, ModalSuccess},
    script::plugins::colors::hash_color,
};

use super::ConsoleShared;

pub(super) fn tsv_wizard_impl(
    console: &ConsoleShared,
    tsv_path: Option<&str>,
) -> bool {
    let graph = console.graph.clone();

    let thread_pool = &console.thread_pool;
    let rayon_pool = &console.rayon_pool;

    let channels = &console.channels;
    let shared_state = &console.shared_state;

    let modal_tx = &channels.modal_tx;
    let app_msg_tx = channels.app_tx.clone();
    let overlay_tx = channels.new_overlay_tx.clone();

    let show_modal = shared_state.show_modal.clone();

    let path_future = if let Some(path) = tsv_path {
        let path = PathBuf::from(path);
        let path_future = async move { Some(path) };
        path_future.boxed()
    } else {
        let path_future = crate::reactor::file_picker_modal(
            modal_tx.clone(),
            &show_modal,
            &[],
            None,
        );

        path_future.boxed()
    };

    enum Line {
        Color { r: u8, g: u8, b: u8, a: u8 },
        Label(String),
    }

    let table_future = async move {
        if let Some(path) = path_future.await {
            use std::io::prelude::*;

            let table_path = path.clone();

            let file = std::fs::File::open(path).ok()?;
            let reader = BufReader::new(file);

            let table = reader
                .lines()
                .filter_map(|line| {
                    let line = line.ok()?;

                    let mut fields = line.split("\t");
                    let node_str = fields.next()?;

                    let node_id = node_str.parse::<usize>().ok()?;
                    let node_id = NodeId::from(node_id as u64);

                    let next = fields.next()?;

                    let line_val =
                        if let Some(color_str) = next.strip_prefix("#") {
                            let ri = 0..=1;
                            let gi = 2..=3;
                            let bi = 4..=5;
                            let ai = 6..=7;

                            let col = |s: &str| u8::from_str_radix(s, 16).ok();

                            match color_str.len() {
                                6 => {
                                    let r = col(&color_str[ri])?;
                                    let g = col(&color_str[gi])?;
                                    let b = col(&color_str[bi])?;

                                    Some(Line::Color { r, g, b, a: 255 })
                                }
                                8 => {
                                    let r = col(&color_str[ri])?;
                                    let g = col(&color_str[gi])?;
                                    let b = col(&color_str[bi])?;
                                    let a = col(&color_str[ai])?;

                                    Some(Line::Color { r, g, b, a })
                                }
                                _ => None,
                            }
                        } else {
                            // set label to rest of line
                            Some(Line::Label(next.trim().to_string()))
                        }?;

                    Some((node_id, line_val))
                })
                .collect::<Vec<_>>();

            Some((table, table_path))
        } else {
            None
        }
    };

    let graph = console.graph.clone();
    let rayon_pool = console.rayon_pool.clone();

    let result = thread_pool.spawn(async move {
        if let Some((table, table_path)) = table_future.await {
            let mut node_color_map: FxHashMap<NodeId, rgb::RGBA<f32>> =
                FxHashMap::default();

            let mut label_set = LabelSet::default();

            let mut label_id = 0;

            for (node_id, line) in table.iter() {
                match line {
                    Line::Color { r, g, b, a } => {
                        let r = (*r as f32) / 255.0;
                        let g = (*g as f32) / 255.0;
                        let b = (*b as f32) / 255.0;
                        let a = (*a as f32) / 255.0;

                        node_color_map
                            .insert(*node_id, rgb::RGBA::new_alpha(r, g, b, a));
                    }
                    Line::Label(label_text) => {
                        let handle = Handle::pack(*node_id, false);
                        label_set.add_at_handle(handle, label_id, &label_text);
                        label_id += 1;
                    }
                }
            }

            let name = table_path.as_os_str().to_str().unwrap();

            if !node_color_map.is_empty() {
                let data = {
                    use rayon::prelude::*;

                    let mut nodes = graph
                        .graph
                        .handles()
                        .map(|h| h.id())
                        .collect::<Vec<_>>();
                    nodes.sort();

                    // TODO gradient support (or rewrite gradient
                    // logic entirely)

                    let colors = rayon_pool.install(|| {
                        nodes
                            .par_iter()
                            .map(|node| {
                                node_color_map.get(&node).copied().unwrap_or(
                                    rgb::RGBA::new(0.5, 0.5, 0.5, 0.4),
                                )
                            })
                            .collect()
                    });

                    OverlayData::RGB(colors)
                };

                let msg = OverlayCreatorMsg::NewOverlay {
                    name: name.to_string(),
                    data,
                };
                overlay_tx.send(msg).unwrap();
            }

            if !label_set.is_empty() {
                app_msg_tx
                    .send(AppMsg::new_label_set(name.to_string(), label_set))
                    .unwrap();
            }
        }
    });

    match result {
        Ok(_) => true,
        Err(_) => false,
    }
}

pub(super) fn bed_label_wizard_impl(
    console: &ConsoleShared,
    bed_path: Option<&str>,
    arg_path_prefix: Option<&str>,
    arg_column_ix: Option<usize>,
) -> bool {
    lazy_static::lazy_static! {
        static ref PREV_DIR: AtomicCell<Option<PathBuf>> = None.into();
    }

    let graph = console.graph.clone();

    let thread_pool = &console.thread_pool;
    let rayon_pool = &console.rayon_pool;

    let channels = &console.channels;
    let shared_state = &console.shared_state;

    let modal_tx = &channels.modal_tx;
    let app_msg_tx = channels.app_tx.clone();
    let overlay_tx = channels.new_overlay_tx.clone();

    let show_modal = shared_state.show_modal.clone();

    log::trace!("in bed_label_wizard");

    let path_future = if let Some(path) = bed_path {
        let path = PathBuf::from(path);
        let path_future = async move { Some(path) };
        path_future.boxed()
    } else {
        let path = PREV_DIR.take();
        // PREV_DIR.store(path.clone());

        let path_future = crate::reactor::file_picker_modal(
            modal_tx.clone(),
            &show_modal,
            &["bed"],
            path,
        );

        path_future.boxed()
    };

    let path_prefix = arg_path_prefix.unwrap_or_default().to_string();

    #[derive(Debug, Clone)]
    struct WizardCfg {
        column_ix: usize,
        column: BedColumn,
        path_prefix: String,
        numeric: bool,
    }

    let (cfg_tx, mut cfg_rx) =
        futures::channel::mpsc::channel::<Option<WizardCfg>>(1);

    let first_run = AtomicCell::new(true);

    let show_modal = show_modal.clone();
    let modal_tx = modal_tx.clone();

    let path_prefix_id = egui::Id::new("bed_label_wizard_path_prefix");

    let pre_config = if let (Some(prefix), Some(column_ix)) =
        (arg_path_prefix, arg_column_ix)
    {
        Some(WizardCfg {
            column_ix,
            column: BedColumn::Index(column_ix),
            path_prefix: path_prefix.clone(),
            numeric: false,
        })
    } else {
        None
    };

    let config_future = move |records: Arc<BedRecords>| {
        let mut cfg = WizardCfg {
            column_ix: 0,
            column: BedColumn::Index(0),
            path_prefix,
            numeric: false,
        };

        if records.has_headers() {
            let name = records.headers().first().cloned().unwrap_or_default();
            cfg.column = BedColumn::Header { index: 0, name };
        }

        let column_ix = AtomicCell::new(arg_column_ix);

        async move {
            let callback = move |cfg: &mut WizardCfg,
                                 ui: &mut egui::Ui,
                                 force: bool| {
                let columns = records.optional_columns();

                let limit = columns.len() - 1;

                let mut success = false;

                if let Some(ix) = column_ix.load() {
                    let ix = ix - 4;
                    if ix <= limit {
                        cfg.column_ix = ix;
                        cfg.column = BedColumn::Index(ix);
                        success = true;
                    } else {
                        column_ix.store(None);
                    }
                } else if limit <= 1 {
                    cfg.column_ix = 0;
                    cfg.column = BedColumn::Index(0);
                    success = true;
                } else {
                    if records.has_headers() {
                        ui.label("Choose header");

                        for (ix, header) in
                            records.headers().iter().skip(3).enumerate()
                        {
                            let row = ui.selectable_label(
                                ix == cfg.column_ix,
                                format!("{}", header.as_bstr()),
                            );

                            if row.clicked() {
                                cfg.column_ix = ix;
                                cfg.column = BedColumn::Header {
                                    index: ix,
                                    name: header.to_owned(),
                                };
                            }

                            if row.double_clicked() {
                                success = true;
                            }
                        }
                    } else {
                        ui.label("Enter column number");
                        let column = ui.add(
                            egui::DragValue::new::<usize>(&mut cfg.column_ix)
                                .clamp_range(4..=(limit + 4)),
                        );

                        if column.changed() {
                            let ix = cfg.column_ix - 4;
                            cfg.column = BedColumn::Index(ix);
                        }
                        if column.lost_focus()
                            && ui.input().key_pressed(egui::Key::Enter)
                            || force
                        {
                            success = true;
                        }
                    }
                }

                if success {
                    return Ok(ModalSuccess::Success);
                }

                Err(ModalError::Continue)
            };

            let prepared = ModalHandler::prepare_callback(
                &show_modal,
                cfg,
                callback,
                cfg_tx,
            );

            modal_tx.send(prepared).unwrap();

            cfg_rx.next().await.flatten()
        }
    };

    let graph = graph.clone();
    let app_msg_tx = app_msg_tx.clone();
    let overlay_tx = overlay_tx.clone();

    let rayon_pool = rayon_pool.clone();

    let result =
        thread_pool.spawn(async move {
            if let Some(path) = path_future.await {

                let records = BedRecords::parse_bed_file(&path);

                match records {
                    Ok(records) => {

                        let records = Arc::new(records);

                        let config = if let Some(mut config) = pre_config {

                            if let Some(row) = records.records().first() {
                                let val = row.get_all(&config.column);

                                if let &[v] = val.as_slice() {
                                    if let Some(_parsed) =
                                        v.to_str().ok().and_then(|v| v.parse::<f32>().ok())
                                    {
                                        config.numeric = true;
                                    } else {
                                        config.numeric = false;
                                    }
                                }
                            }

                            config
                        } else {
                            config_future(records.clone()).await.unwrap()
                        };

                        let mut path_map: HashMap<
                                Vec<u8>,
                            (PathId, Option<(usize, usize)>),
                            > = HashMap::default();

                        let mut step_caches: FxHashMap<
                                PathId,
                            Vec<(Handle, _, usize)>,
                            > = FxHashMap::default();

                        let prefix = config.path_prefix.as_bytes();

                        let column = &config.column;

                        for path_id in graph.graph.path_ids() {
                            let path_name = graph
                                .graph
                                .get_path_name_vec(path_id)
                                .unwrap();

                            if let Some((name, start, end)) =
                                path_name_range(&path_name)
                            {
                                if let Some(stripped) =
                                    name.strip_prefix(prefix)
                                {
                                    path_map.insert(
                                        stripped.to_owned(),
                                        (path_id, Some((start, end))),
                                    );
                                }
                            } else {
                                if let Some(stripped) =
                                    path_name.strip_prefix(prefix)
                                {
                                    path_map.insert(
                                        stripped.to_owned(),
                                        (path_id, None),
                                    );
                                }
                            };
                        }

                        let mut label_set = LabelSet::default();

                        let mut node_color_map: FxHashMap<
                                NodeId,
                            rgb::RGBA<f32>,
                            > = FxHashMap::default();

                        for (label_id, record) in records.records().iter().enumerate() {

                            if let Some((path_id, range)) =
                                path_map.get(record.chr.as_slice())
                            {
                                //
                                let (path_id, range) = (*path_id, *range);

                                let steps = step_caches
                                    .entry(path_id)
                                    .or_insert_with(|| {
                                        graph
                                            .path_pos_steps(path_id)
                                            .unwrap()
                                    });

                                let offset = range.map(|(s, _)| s);

                                if let Some(step_range) =
                                    crate::annotations::path_step_range(
                                        steps,
                                        offset,
                                        record.start(),
                                        record.end(),
                                    )
                                {
                                    if let Some(value) =
                                        record.get_first(&column)
                                    {
                                        if !step_range.is_empty() {

                                            let value_str = value.to_str().unwrap();
                                            if let Some((label_text, color)) = split_colored_label(value_str) {

                                                    let (mid, _, _) = step_range[step_range.len() / 2];

                                                    label_set.add_at_handle(
                                                        mid, label_id, label_text,
                                                    );

                                                    for &(handle, _, _) in step_range.iter() {
                                                        let node = handle.id();
                                                        node_color_map.insert(node, color);
                                                    }

                                            } else {
                                                let color = if config.numeric {

                                                    let val = value.to_str().ok().and_then(|v| v.parse::<f32>().ok());

                                                    if let Some(v) = val {
                                                        Some(rgb::RGBA::new(v, v, v, v))
                                                    } else {
                                                        None
                                                    }
                                                } else {
                                                    let hash =
                                                    {
                                                        use std::collections::hash_map::DefaultHasher;
                                                        use std::hash::{Hash, Hasher};
                                                        let mut hasher = DefaultHasher::default();
                                                        value.hash(&mut hasher);
                                                        let hash = hasher.finish();
                                                        hash
                                                    };

                                                    let color = hash_color(hash);
                                                    Some(color)
                                                };


                                                if let Some(color) = color {
                                                    let (mid, _, _) = step_range[step_range.len() / 2];

                                                    let label_text = format!(
                                                        "{}",
                                                        value.as_bstr()
                                                    );
                                                    label_set.add_at_handle(
                                                        mid, label_id, &label_text,
                                                    );

                                                    for &(handle, _, _) in step_range.iter() {
                                                        let node = handle.id();
                                                        node_color_map.insert(node, color);
                                                    }
                                                }
                                            }

                                        }
                                    }
                                } else {
                                    // log::warn!(
                                    //     "out of step range: {}, {}",
                                    //     record.start(),
                                    //     record.end()
                                    // );
                                }
                            }

                        }

                        let data = {
                            use rayon::prelude::*;

                            let mut nodes = graph.graph.handles().map(|h| h.id()).collect::<Vec<_>>();
                            nodes.sort();


                            if config.numeric {
                                let values = rayon_pool.install(|| {
                                    nodes.par_iter().map(|node| {

                                        let rgb = node_color_map.get(&node).copied().unwrap_or(rgb::RGBA::new(0.0, 0.0, 0.0, 0.0));
                                        rgb.r
                                    }).collect()
                                });

                                OverlayData::Value(values)
                            } else {

                                let colors = rayon_pool.install(|| {
                                    nodes.par_iter().map(|node| {

                                        node_color_map.get(&node).copied().unwrap_or(rgb::RGBA::new(0.5, 0.5, 0.5, 0.4))
                                    }).collect()
                                });

                                OverlayData::RGB(colors)
                            }

                        };

                        let name = path.file_name().and_then(|s| s.to_str()).unwrap();

                        let name = if matches!(column, BedColumn::Index(_)) {
                            format!("{}:col# {}", name, config.column_ix + 3)
                        } else {
                            format!("{}:{}", name, column)
                        };

                        let msg = OverlayCreatorMsg::NewOverlay {
                            name: name.to_string(),
                            data,
                        };
                        overlay_tx.send(msg).unwrap();

                        let records = records.clone();
                        // let graph = graph.clone();

                        /*
                        let on_label_click = Box::new(move |label_id| {
                        if let Some(record) = records.records.get(label_id) {
                        let record: &BedRecord = record;
                        let chr: &[u8] = &record.chr;
                        log::warn!("clicked record on path {}, range {}-{}", chr.as_bstr(), record.start, record.end);
                    }
                    }) as Box<dyn Fn(usize) + Send + Sync + 'static>;
                         */

                        app_msg_tx
                            .send(AppMsg::new_label_set(
                                name,
                                label_set,
                                // Some(on_label_click),
                            ))
                            .unwrap();

                    }
                    Err(err) => {
                        log::warn!("parse error: {:+}", err);
                    }
                }

                let mut path = path;
                path.pop();
                if path.is_dir() {
                    PREV_DIR.store(Some(path));
                } else {
                    PREV_DIR.store(None);
                }
            }
        });

    match result {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn split_colored_label(raw: &str) -> Option<(&str, rgb::RGBA<f32>)> {
    let (label, color) = raw.rsplit_once('#')?;

    let ri = 0..=1;
    let gi = 2..=3;
    let bi = 4..=5;
    let ai = 6..=7;

    let col = |s: &str| u8::from_str_radix(s, 16).ok();

    let (r, g, b) = match color.len() {
        6 | 8 => {
            let r = col(&color[ri])?;
            let g = col(&color[gi])?;
            let b = col(&color[bi])?;
            Some((r, g, b))
        }
        _ => None,
    }?;

    let a = if color.len() == 8 {
        col(&color[ai])?
    } else {
        255
    };

    let r = (r as f32) / 255.0;
    let g = (g as f32) / 255.0;
    let b = (b as f32) / 255.0;
    let a = (a as f32) / 255.0;

    Some((label, rgb::RGBA::new(r, g, b, a)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_color_split() {
        let l0 = "hello there#112233";
        let l1 = "hello there#11223344";
        let l2 = "hello there#11223344";
        super::split_colored_label(l0);
        super::split_colored_label(l1);
        super::split_colored_label(l2);

        assert!(false);
    }
}
