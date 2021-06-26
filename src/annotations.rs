use anyhow::Result;

use handlegraph::handle::NodeId;

use rustc_hash::FxHashMap;

use std::collections::hash_map::HashMap;

use crate::{geometry::*, view::*};

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
}
