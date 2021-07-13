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

use bstr::ByteSlice;

use rustc_hash::FxHashMap;

use std::collections::hash_map::HashMap;

use crate::{geometry::*, gluon::GraphHandle, view::*};

pub mod gff;

pub use gff::*;

pub enum Strand {
    Pos,
    Neg,
    None,
}

impl std::str::FromStr for Strand {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        if s == "+" {
            Ok(Strand::Pos)
        } else if s == "-" {
            Ok(Strand::Neg)
        } else if s == "." {
            Ok(Strand::None)
        } else {
            Err(())
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationSource {
    name: String,
    map: FxHashMap<NodeId, String>,
}

#[derive(Debug, Clone, Default)]
pub struct Annotations {
    annotations: FxHashMap<usize, AnnotationSource>,
    enabled: FxHashMap<usize, bool>,

    next_id: usize,
}

// pub struct Annotations {
//     annotations: HashMap<String, FxHashMap<NodeId, String>>,
// }

impl AnnotationSource {
    // TODO built in parser for BED files, at least
    // TODO support building multiple sources from a single file at once
    // TODO support building sources via the gluon api
    pub fn from_file<F, P>(path: P, name: &str, parser: F) -> Result<Self>
    where
        P: AsRef<std::path::Path>,
        for<'a> F: Fn(&'a str) -> Result<(NodeId, String)>,
    {
        use std::fs::File;
        use std::io::prelude::*;

        let name = name.to_owned();

        let file = File::open(path)?;

        let lines = std::io::BufReader::new(file).lines();

        let mut map: FxHashMap<NodeId, String> = FxHashMap::default();

        for line in lines {
            let line = line?;
            let (node, val) = parser(&line)?;

            map.insert(node, val);
        }

        Ok(Self { name, map })
    }
}

impl Annotations {
    pub fn insert(&mut self, source: AnnotationSource) {
        let id = self.next_id;
        self.next_id += 1;

        self.annotations.insert(id, source);
        self.enabled.insert(id, true);
    }

    pub fn annotations_for(&self, node: NodeId) -> Vec<(&str, &str)> {
        let mut res: Vec<(&str, &str)> = Vec::new();

        let mut sources = self
            .annotations
            .iter()
            .filter(|(k, _)| self.enabled.get(k).copied() == Some(true))
            .collect::<Vec<_>>();
        sources.sort_by_key(|(k, _)| *k);

        for (_id, source) in sources {
            if let Some(val) = source.map.get(&node) {
                let name = &source.name;
                res.push((name, val));
            }
        }

        res
    }
}

impl Annotations {
    pub fn from_bed_file<P: AsRef<std::path::Path>>(
        graph: &GraphHandle,
        path: P,
    ) -> Result<Self> {
        use crate::gluon::bed::BedRecord;

        use crate::gluon;

        let bed_records = BedRecord::parse_bed_file(path)?;

        // NB: just doing names for now, and assuming that the BED file has names

        let mut record_names: FxHashMap<NodeId, String> = Default::default();

        for record in bed_records {
            let chrom = record.chrom();
            let start = record.chrom_start();
            let end = record.chrom_end();

            let name = record.name().unwrap();
            let name = name.to_str().unwrap().to_owned();

            let path_id = graph.graph.get_path_id(chrom).unwrap();

            let steps =
                gluon::path_base_range(graph, path_id.0, start, end).unwrap();

            for (id, _, _) in steps {
                let node_id = NodeId::from(id);

                record_names.insert(node_id, name.clone());
            }
        }

        let source = AnnotationSource {
            name: "Name".to_string(),
            map: record_names,
        };

        let mut res = Annotations::default();

        res.insert(source);

        Ok(res)
    }
}
