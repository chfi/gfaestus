use std::collections::{HashMap, HashSet};

use bstr::ByteSlice;

use anyhow::Result;

use super::{AnnotationCollection, AnnotationColumn, AnnotationRecord, Strand};

#[derive(Debug, Clone, Default)]
pub struct BedRecords {
    file_name: String,

    pub records: Vec<BedRecord>,

    column_keys: Vec<BedColumn>,
    // TODO add header support
    // pub column_header: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct BedRecord {
    pub chr: Vec<u8>,
    pub start: usize,
    pub end: usize,

    // TODO add header support
    pub rest: Vec<Vec<u8>>,
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

impl BedRecords {
    pub fn parse_bed_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        use std::fs::File;

        use std::io::{BufRead, BufReader};

        let file_name = path.as_ref().file_name().unwrap();
        let file_name = file_name.to_str().unwrap().to_string();

        let file = File::open(path)?;

        let mut reader = BufReader::new(file);

        let mut buf: Vec<u8> = Vec::new();

        let mut records = Vec::new();

        let mut column_count = 0;

        loop {
            buf.clear();

            let read = reader.read_until(b'\n', &mut buf)?;

            if read == 0 {
                break;
            }

            let line = &buf[0..read];

            if line[0] == b'#' {
                continue;
            }

            let fields = line.fields();

            if let Some(record) = BedRecord::parse_row(fields) {
                column_count = record.rest.len();
            }
        }

        let mut column_keys: Vec<BedColumn> =
            vec![BedColumn::Chr, BedColumn::Start, BedColumn::End];

        column_keys.extend((0..column_count).map(|ix| BedColumn::Index(ix)));

        Ok(Self {
            file_name,
            records,
            column_keys,
        })
    }
}

fn parse_next<'a, T, I>(fields: &mut I) -> Option<T>
where
    T: std::str::FromStr,
    I: Iterator<Item = &'a [u8]> + 'a,
{
    let field = fields.next()?;
    let field = field.as_bstr().to_str().ok()?;
    field.parse().ok()
}

impl BedRecord {
    fn parse_row<'a, I>(mut fields: I) -> Option<Self>
    where
        I: Iterator<Item = &'a [u8]> + 'a,
    {
        let chr = fields.next()?;

        let start: usize = parse_next(&mut fields)?;
        let end: usize = parse_next(&mut fields)?;

        let mut rest: Vec<Vec<u8>> = Vec::new();

        while let Some(field) = fields.next() {
            rest.push(field.to_owned());
        }

        Some(Self {
            chr: chr.to_owned(),
            start,
            end,
            rest,
        })
    }
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

impl AnnotationCollection for BedRecords {
    type ColumnKey = BedColumn;
    type Record = BedRecord;

    fn file_name(&self) -> &str {
        &self.file_name
    }

    fn len(&self) -> usize {
        self.records.len()
    }

    fn all_columns(&self) -> Vec<Self::ColumnKey> {
        self.column_keys.clone()
    }

    fn mandatory_columns(&self) -> Vec<Self::ColumnKey> {
        let slice = &self.column_keys[0..=2];
        Vec::from(slice)
    }

    fn optional_columns(&self) -> Vec<Self::ColumnKey> {
        let mut res = Vec::new();
        res.extend(self.column_keys.iter().cloned().skip(3));
        res
    }

    fn records(&self) -> &[Self::Record] {
        &self.records
    }

    fn wrap_column(column: Self::ColumnKey) -> AnnotationColumn {
        AnnotationColumn::Bed(column)
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
            columns.push(Index(i));
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
            BedColumn::Index(i) => self.rest.get(*i).map(|v| v.as_bytes()),
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
                .get(*i)
                .map(|v| v.as_bytes())
                .into_iter()
                .collect(), // BedColumn::Header(h) => todo!(),
        }
    }

    fn is_column_optional(key: &Self::ColumnKey) -> bool {
        use BedColumn::*;
        match key {
            Chr | Start | End => false,
            _ => true,
        }
    }
}
