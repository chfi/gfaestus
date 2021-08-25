use handlegraph::{
    handle::{Edge, Handle},
    mutablehandlegraph::*,
    pathhandlegraph::*,
};

use gfa::mmap::MmapGFA;

use handlegraph::packedgraph::PackedGraph;

use gfa::gfa::Line;

use anyhow::Result;

use rustc_hash::FxHashMap;

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

pub fn packed_graph_from_mmap(mmap_gfa: &mut MmapGFA) -> Result<PackedGraph> {
    let indices = mmap_gfa.build_index()?;

    // let mut graph =
    //     PackedGraph::with_expected_node_count(indices.segments.len());

    let mut graph = PackedGraph::default();
    // eprintln!("empty space usage: {} bytes", graph.total_bytes());

    info!(
        "loading GFA with {} nodes, {} edges",
        indices.segments.len(),
        indices.links.len()
    );

    let mut min_id = std::usize::MAX;
    let mut max_id = 0;

    for &offset in indices.segments.iter() {
        let _line = mmap_gfa.read_line_at(offset.0)?;
        let name = mmap_gfa.current_line_name().unwrap();
        let name_str = std::str::from_utf8(name).unwrap();
        let id = name_str.parse::<usize>().unwrap();

        min_id = id.min(min_id);
        max_id = id.max(max_id);
    }

    let id_offset = if min_id == 0 { 1 } else { 0 };

    info!("adding nodes");
    for &offset in indices.segments.iter() {
        let _line = mmap_gfa.read_line_at(offset.0)?;
        let segment = mmap_gfa.parse_current_line()?;

        if let gfa::gfa::Line::Segment(segment) = segment {
            let id = (segment.name + id_offset) as u64;
            graph.create_handle(&segment.sequence, id);
        }
    }
    // eprintln!(
    //     "after segments - space usage: {} bytes",
    //     graph.total_bytes()
    // );

    info!("adding edges");

    let edges_iter = indices.links.iter().filter_map(|&offset| {
        let _line = mmap_gfa.read_line_at(offset).ok()?;
        let link = mmap_gfa.parse_current_line().ok()?;

        if let gfa::gfa::Line::Link(link) = link {
            let from_id = (link.from_segment + id_offset) as u64;
            let to_id = (link.to_segment + id_offset) as u64;

            let from = Handle::new(from_id, link.from_orient);
            let to = Handle::new(to_id, link.to_orient);

            Some(Edge(from, to))
        } else {
            None
        }
    });

    graph.create_edges_iter(edges_iter);

    // eprintln!(
    //     "after edges    - space usage: {} bytes",
    //     graph.total_bytes()
    // );

    let mut path_ids: FxHashMap<PathId, (usize, usize)> = FxHashMap::default();
    path_ids.reserve(indices.paths.len());

    info!("adding paths");
    for &offset in indices.paths.iter() {
        let line = mmap_gfa.read_line_at(offset)?;
        let length = line.len();
        if let Some(path_name) = mmap_gfa.current_line_name() {
            let path_id = graph.create_path(path_name, false).unwrap();
            path_ids.insert(path_id, (offset, length));
        }
    }

    info!("created path handles");

    let mmap_gfa_bytes = mmap_gfa.get_ref();

    let parser = mmap_gfa.get_parser();

    graph.with_all_paths_mut_ctx_chn_new(|path_id, sender, path_ref| {
        let &(offset, length) = path_ids.get(&path_id).unwrap();
        let end = offset + length;
        let line = &mmap_gfa_bytes[offset..end];
        if let Some(Line::Path(path)) = parser.parse_gfa_line(line).ok() {
            path_ref.append_handles_iter_chn(
                sender,
                path.iter().map(|(node, orient)| {
                    let node = node + id_offset;
                    Handle::new(node, orient)
                }),
            );
        }
    });

    /*
    graph.with_all_paths_mut_ctx_chn(|path_id, path_ref| {
        let &(offset, length) = path_ids.get(&path_id).unwrap();
        let end = offset + length;
        let line = &mmap_gfa_bytes[offset..end];

        if let Some(Line::Path(path)) = parser.parse_gfa_line(line).ok() {
            path.iter()
                .map(|(node, orient)| {
                    let node = node + id_offset;
                    let handle = Handle::new(node, orient);
                    path_ref.append_step(handle)
                })
                .collect()

        // path_ref.append_steps_iter(path.iter().map(|(node, orient)| {
        //     let node = node + id_offset;
        //     Handle::new(node, orient)
        // }))
        } else {
            Vec::new()
        }
    });
    */

    // eprintln!(
    //     "after paths    - space usage: {} bytes",
    //     graph.total_bytes()
    // );

    Ok(graph)
}
