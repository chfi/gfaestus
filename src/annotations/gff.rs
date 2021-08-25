use std::collections::{HashMap, HashSet};

use bstr::ByteSlice;

use anyhow::Result;

use log::error;

use super::{
    AnnotationCollection, AnnotationColumn, AnnotationRecord, ColumnKey, Strand,
};

#[derive(Debug, Clone, Default)]
pub struct Gff3Records {
    file_name: String,

    pub records: Vec<Gff3Record>,

    pub attribute_keys: HashSet<Vec<u8>>,
}

impl AnnotationCollection for Gff3Records {
    type ColumnKey = Gff3Column;
    type Record = Gff3Record;

    fn all_columns(&self) -> Vec<Gff3Column> {
        let mut columns = Vec::with_capacity(8 + self.attribute_keys.len());

        use Gff3Column::*;
        columns.push(SeqId);
        columns.push(Source);
        columns.push(Type);
        columns.push(Start);
        columns.push(End);
        columns.push(Score);
        columns.push(Strand);
        columns.push(Frame);

        let mut attr_keys =
            self.attribute_keys.iter().cloned().collect::<Vec<_>>();
        attr_keys.sort();
        columns.extend(attr_keys.into_iter().map(|k| Attribute(k)));

        columns
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }

    fn len(&self) -> usize {
        self.records.len()
    }

    fn mandatory_columns(&self) -> Vec<Gff3Column> {
        let mut columns = Vec::with_capacity(8);

        use Gff3Column::*;
        columns.push(SeqId);
        columns.push(Source);
        columns.push(Type);
        columns.push(Start);
        columns.push(End);
        columns.push(Score);
        columns.push(Strand);
        columns.push(Frame);

        columns
    }

    fn optional_columns(&self) -> Vec<Gff3Column> {
        let mut columns = Vec::with_capacity(self.attribute_keys.len());

        let mut attr_keys =
            self.attribute_keys.iter().cloned().collect::<Vec<_>>();
        attr_keys.sort();
        columns.extend(attr_keys.into_iter().map(|k| Gff3Column::Attribute(k)));

        columns
    }

    fn records(&self) -> &[Gff3Record] {
        &self.records
    }

    fn wrap_column(column: Gff3Column) -> AnnotationColumn {
        AnnotationColumn::Gff3(column)
    }
}

#[derive(Debug, Clone)]
pub struct Gff3Record {
    seq_id: Vec<u8>,
    source: Vec<u8>,
    type_: Vec<u8>,

    start: usize,
    end: usize,

    score: Option<f64>,

    strand: Strand,

    frame: Vec<u8>,

    attributes: HashMap<Vec<u8>, Vec<Vec<u8>>>,
}

impl AnnotationRecord for Gff3Record {
    type ColumnKey = Gff3Column;

    fn columns(&self) -> Vec<Gff3Column> {
        let mut columns = Vec::with_capacity(8 + self.attributes.len());

        use Gff3Column::*;
        columns.push(SeqId);
        columns.push(Source);
        columns.push(Type);
        columns.push(Start);
        columns.push(End);
        columns.push(Score);
        columns.push(Strand);
        columns.push(Frame);

        let mut attr_keys = self.attributes.keys().cloned().collect::<Vec<_>>();
        attr_keys.sort();
        columns.extend(attr_keys.into_iter().map(|k| Attribute(k)));

        columns
    }

    fn seq_id(&self) -> &[u8] {
        &self.seq_id
    }

    fn start(&self) -> usize {
        self.start
    }

    fn end(&self) -> usize {
        self.end
    }

    fn score(&self) -> Option<f64> {
        self.score
    }

    fn get_first(&self, key: &Self::ColumnKey) -> Option<&[u8]> {
        match key {
            Gff3Column::SeqId => Some(&self.seq_id),
            Gff3Column::Source => Some(&self.source),
            Gff3Column::Type => Some(&self.type_),
            Gff3Column::Strand => match self.strand {
                Strand::Pos => Some(b"+"),
                Strand::Neg => Some(b"-"),
                Strand::None => Some(b"."),
            },
            Gff3Column::Frame => Some(&self.frame),
            Gff3Column::Attribute(key) => self
                .attributes
                .get(key)
                .and_then(|a| a.first())
                .map(|a| a.as_slice()),
            Gff3Column::Start | Gff3Column::End | Gff3Column::Score => None,
        }
    }

    fn get_all(&self, key: &Self::ColumnKey) -> Vec<&[u8]> {
        match key {
            Gff3Column::SeqId => vec![&self.seq_id],
            Gff3Column::Source => vec![&self.source],
            Gff3Column::Type => vec![&self.type_],
            Gff3Column::Strand => match self.strand {
                Strand::Pos => vec![b"+"],
                Strand::Neg => vec![b"-"],
                Strand::None => vec![b"."],
            },
            Gff3Column::Frame => vec![&self.frame],
            Gff3Column::Attribute(key) => {
                if let Some(values) = self.attributes.get(key) {
                    values.iter().map(|v| v.as_slice()).collect()
                } else {
                    Vec::new()
                }
            }
            Gff3Column::Start | Gff3Column::End | Gff3Column::Score => vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum Gff3Column {
    SeqId,
    Source,
    Type,
    Start,
    End,
    Score,
    Strand,
    Frame,
    Attribute(Vec<u8>),
}

impl ColumnKey for Gff3Column {
    fn is_column_optional(key: &Self) -> bool {
        match key {
            Self::Attribute(_key) => true,
            _ => false,
        }
    }

    fn seq_id() -> Self {
        Self::SeqId
    }

    fn start() -> Self {
        Self::Start
    }

    fn end() -> Self {
        Self::End
    }
}

impl Gff3Column {
    pub fn attribute_key(&self) -> Option<&[u8]> {
        if let Self::Attribute(attr) = &self {
            Some(attr.as_slice())
        } else {
            None
        }
    }
}

impl std::fmt::Display for Gff3Column {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::result::Result<(), std::fmt::Error> {
        match self {
            Gff3Column::SeqId => write!(f, "seq_id"),
            Gff3Column::Source => write!(f, "source"),
            Gff3Column::Type => write!(f, "type"),
            Gff3Column::Start => write!(f, "start"),
            Gff3Column::End => write!(f, "end"),
            Gff3Column::Score => write!(f, "score"),
            Gff3Column::Strand => write!(f, "strand"),
            Gff3Column::Frame => write!(f, "frame"),
            Gff3Column::Attribute(attr) => write!(f, "{}", attr.as_bstr()),
        }
    }
}

impl Gff3Records {
    pub fn parse_gff3_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file_name = path.as_ref().file_name().unwrap();
        let file_name = file_name.to_str().unwrap().to_string();

        let file = File::open(path)?;

        let mut reader = BufReader::new(file);

        let mut buf: Vec<u8> = Vec::new();

        let mut records = Vec::new();

        let mut attribute_keys: HashSet<Vec<u8>> = HashSet::default();

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

            if let Some(record) = Gff3Record::parse_row(fields) {
                for key in record.attributes.keys() {
                    if !attribute_keys.contains(key) {
                        attribute_keys.insert(key.to_owned());
                    }
                }

                records.push(record);
            } else {
                std::process::exit(1);
            }
        }

        Ok(Self {
            file_name,

            records,
            attribute_keys,
        })
    }
}

impl Gff3Record {
    pub fn seq_id(&self) -> &[u8] {
        &self.seq_id
    }

    pub fn source(&self) -> &[u8] {
        &self.source
    }

    pub fn type_(&self) -> &[u8] {
        &self.type_
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn score(&self) -> Option<f64> {
        self.score
    }

    pub fn frame(&self) -> &[u8] {
        &self.frame
    }

    pub fn strand(&self) -> Strand {
        self.strand
    }

    pub fn attributes(&self) -> &HashMap<Vec<u8>, Vec<Vec<u8>>> {
        &self.attributes
    }

    pub fn get_tag(&self, key: &[u8]) -> Option<&[Vec<u8>]> {
        self.attributes.get(key).map(|s| s.as_slice())
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

impl Gff3Record {
    pub fn parse_row<'a, I>(mut fields: I) -> Option<Self>
    where
        I: Iterator<Item = &'a [u8]> + 'a,
    {
        let seq_id = fields.next()?;
        let source = fields.next()?;
        let type_ = fields.next()?;

        let start: usize = parse_next(&mut fields)?;
        let end: usize = parse_next(&mut fields)?;

        let score_field = fields.next()?;

        let score = if score_field == b"." {
            None
        } else {
            let score = score_field.as_bstr().to_str().ok()?;
            let score: f64 = score.parse().ok()?;
            Some(score)
        };

        let strand: Strand = parse_next(&mut fields)?;

        let frame = fields.next()?;

        let mut attributes: HashMap<Vec<u8>, Vec<Vec<u8>>> = HashMap::default();

        let attributes_raw = fields.next()?;

        let attributes_split = attributes_raw.split_str(";");

        for attribute in attributes_split {
            let mut attr_fields = attribute.split_str("=");
            let tag = attr_fields.next()?;
            let val = attr_fields.next()?;

            attributes
                .entry(tag.to_owned())
                .or_default()
                .push(val.to_owned());
        }

        Some(Self {
            seq_id: seq_id.to_owned(),
            source: source.to_owned(),
            type_: type_.to_owned(),
            start,
            end,
            score,
            strand,
            frame: frame.to_owned(),
            attributes,
        })
    }

    pub fn parse_gff3_file<P: AsRef<std::path::Path>>(
        path: P,
    ) -> Result<Vec<Self>> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open(path)?;

        let mut reader = BufReader::new(file);

        let mut buf: Vec<u8> = Vec::new();

        let mut result = Vec::new();

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

            if let Some(record) = Self::parse_row(fields) {
                result.push(record);
            } else {
                error!("failed to parse row:");
                error!("\"{}\"", line.as_bstr());
                std::process::exit(1);
            }
        }

        Ok(result)
    }
}
