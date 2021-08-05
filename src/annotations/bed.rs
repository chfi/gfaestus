use std::collections::{HashMap, HashSet};

use bstr::ByteSlice;

use anyhow::Result;

use super::{AnnotationCollection, AnnotationColumn, AnnotationRecord, Strand};

#[derive(Debug, Clone, Default)]
pub struct BedRecords {
    file_name: String,

    pub records: Vec<BedRecord>,
    // TODO add header support
    // pub column_header: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct BedRecord {
    chr: Vec<u8>,
    start: usize,
    end: usize,

    // TODO add header support
    rest: Vec<Vec<u8>>,
    // headers: FxHashMap<Vec<u8>, usize>
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum BedColumn {
    Chr,
    Start,
    End,
    Name,
    Index(usize),
    // Header(Vec<u8>),
}

impl std::fmt::Display for BedColumn {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::result::Result<(), std::fmt::Error> {
        match self {
            BedColumn::Chr => write!(f, "chr"),
            BedColumn::Start => write!(f, "start"),
            BedColumn::End => write!(f, "end"),
            BedColumn::Name => write!(f, "name"),
            BedColumn::Index(i) => write!(f, "{}", i),
            // BedColumn::Header(h) => write!(f, "{}", h.as_bstr()),
        }
    }
}

impl AnnotationRecord for BedRecord {
    type ColumnKey = BedColumn;

    fn columns(&self) -> Vec<Self::ColumnKey> {
        let mut columns = Vec::with_capacity(3 + self.rest.len());

        use BedColumn::*;
        columns.push(Chr);
        columns.push(Start);
        columns.push(End);
        for i in 0..self.rest.len() {
            columns.push(Index(i + 3));
        }

        columns
    }

    fn seq_id(&self) -> &[u8] {
        &self.chr
    }

    fn start(&self) -> usize {
        self.start
    }

    fn end(&self) -> usize {
        self.end
    }

    // TODO handle this more intelligently... somehow
    fn score(&self) -> Option<f64> {
        let field = self.rest.get(1)?;
        let field_str = field.to_str().ok()?;
        field_str.parse().ok()
    }

    fn get_first(&self, key: &Self::ColumnKey) -> Option<&[u8]> {
        match key {
            BedColumn::Chr => Some(&self.chr),
            BedColumn::Start => None,
            BedColumn::End => None,
            BedColumn::Name => self.rest.get(0).map(|v| v.as_bytes()),
            BedColumn::Index(i) => self.rest.get(i - 3).map(|v| v.as_bytes()),
            // BedColumn::Header(h) => todo!(),
        }
    }

    fn get_all(&self, key: &Self::ColumnKey) -> Vec<&[u8]> {
        match key {
            BedColumn::Chr => vec![&self.chr],
            BedColumn::Start => vec![],
            BedColumn::End => vec![],
            BedColumn::Name => {
                self.rest.get(0).map(|v| v.as_bytes()).into_iter().collect()
            }
            BedColumn::Index(i) => self
                .rest
                .get(i - 3)
                .map(|v| v.as_bytes())
                .into_iter()
                .collect(),
            // BedColumn::Header(h) => todo!(),
        }
    }

    fn is_column_optional(key: &Self::ColumnKey) -> bool {
        use BedColumn::*;
        match key {
            Chr | Start | End | Index(0 | 1 | 2) => false,
            _ => true,
        }
    }
}
