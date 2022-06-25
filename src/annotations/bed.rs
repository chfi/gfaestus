use bstr::ByteSlice;

use anyhow::Result;

use super::{
    AnnotationCollection, AnnotationColumn, AnnotationRecord, ColumnKey,
};

#[derive(Debug, Clone, Default)]
pub struct BedRecords {
    file_name: String,

    pub records: Vec<BedRecord>,

    column_keys: Vec<BedColumn>,
    // TODO add header support
    // pub column_header: Vec<Vec<u8>>,
    headers: Vec<Vec<u8>>,
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
    Header { index: usize, name: Vec<u8> },
}

impl ColumnKey for BedColumn {
    fn is_column_optional(key: &Self) -> bool {
        use BedColumn::*;
        match key {
            Chr | Start | End => false,
            _ => true,
        }
    }

    fn seq_id() -> Self {
        Self::Chr
    }

    fn start() -> Self {
        Self::Start
    }

    fn end() -> Self {
        Self::End
    }
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

        let mut headers = Vec::new();

        let mut line_num = 0;

        loop {
            buf.clear();

            let read = reader.read_until(b'\n', &mut buf)?;

            if read == 0 {
                break;
            }

            let line = &buf[0..read];

            line_num += 1;

            if line[0] == b'#' {
                if line_num == 1 && line.len() > 1 {
                    let fields = (&line[1..]).fields();
                    headers.extend(fields.map(|field| field.trim().to_owned()));
                }
                continue;
            }
            let fields = line.split_str("\t").map(ByteSlice::trim);

            if let Some(record) = BedRecord::parse_row(fields) {
                column_count = record.rest.len().max(column_count);
                records.push(record);
            }
        }

        let mut column_keys: Vec<BedColumn> =
            vec![BedColumn::Chr, BedColumn::Start, BedColumn::End];

        if headers.is_empty() {
            column_keys
                .extend((0..column_count).map(|ix| BedColumn::Index(ix)));
        } else {
            column_keys.extend(headers.iter().skip(3).enumerate().map(
                |(ix, h)| BedColumn::Header {
                    index: ix,
                    name: h.to_owned(),
                },
            ));
        }

        Ok(Self {
            file_name,
            records,
            column_keys,

            headers,
        })
    }

    pub fn has_headers(&self) -> bool {
        !self.headers.is_empty()
    }

    pub fn headers(&self) -> &[Vec<u8>] {
        &self.headers
    }

    pub fn header_to_column(&self, header: &[u8]) -> Option<BedColumn> {
        let raw_index = self
            .headers
            .iter()
            .enumerate()
            .find(|(_, h)| h == &header)
            .map(|(ix, _)| ix)?;

        use BedColumn as Bed;

        let column = match raw_index {
            0 => Bed::Chr,
            1 => Bed::Start,
            2 => Bed::End,
            // 3 => Bed::Name,
            ix => Bed::Header {
                index: ix,
                name: header.to_owned(),
            },
            // ix => Bed::Index(ix - 3),
        };

        Some(column)
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

        let rest: Vec<Vec<u8>> = fields.map(|field| field.to_owned()).collect();
        // let mut rest: Vec<Vec<u8>> = Vec::new();

        // let mut count = 0;
        // while let Some(field) = fields.next() {
        //     count += 1;
        //     rest.push(field.to_owned());
        // }
        // println!("adding {} columns", count);

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
            BedColumn::Header { name, .. } => write!(f, "{}", name.as_bstr()),
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
            BedColumn::Header { index, .. } => {
                self.rest.get(*index).map(|v| v.as_bytes())
            } //     let index = self.headers.iter().fi
              //     todo!(),
              // }
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
                .collect(),

            BedColumn::Header { index, .. } => self
                .rest
                .get(*index)
                .map(|v| v.as_bytes())
                .into_iter()
                .collect(),
        }
    }
}
