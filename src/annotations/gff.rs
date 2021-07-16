use std::collections::{HashMap, HashSet};

use bstr::{ByteSlice, ByteVec};

use anyhow::Result;

use super::Strand;

pub struct Gff3Records {
    records: Vec<Gff3Record>,

    attribute_keys: HashSet<Vec<u8>>,
}

impl Gff3Records {
    pub fn parse_gff3_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

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
                eprintln!("failed to parse row:");
                eprintln!("\"{}\"", line.as_bstr());

                std::process::exit(1);
            }
        }

        Ok(Self {
            records,
            attribute_keys,
        })
    }
}

#[derive(Clone)]
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
                eprintln!("failed to parse row:");
                eprintln!("\"{}\"", line.as_bstr());

                std::process::exit(1);
            }
        }

        Ok(result)
    }
}
