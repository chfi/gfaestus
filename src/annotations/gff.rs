use std::collections::HashMap;

use bstr::{ByteSlice, ByteVec};

use anyhow::Result;

use super::Strand;

pub struct Gff3Record {
    seq_id: Vec<u8>,
    source: Vec<u8>,
    type_: Vec<u8>,

    start: usize,
    end: usize,

    score: f64,

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

        let score: f64 = parse_next(&mut fields)?;

        let strand: Strand = parse_next(&mut fields)?;

        let frame = fields.next()?;

        let attributes_raw = fields.next()?;

        let attributes_split = attributes_raw.split_str(";");

        let mut attributes: HashMap<Vec<u8>, Vec<Vec<u8>>> = HashMap::default();

        for attribute in attributes_split {
            let attr_fields = attribute.split_str("=");
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

            let fields = line.fields();

            if let Some(record) = Self::parse_row(fields) {
                result.push(record);
            }
        }

        Ok(result)
    }
}
