use std::path::Path;

use bstr::{ByteSlice, ByteVec};

use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub struct BedRecord {
    chrom: Vec<u8>,
    chrom_start: usize,
    chrom_end: usize,

    name: Option<Vec<u8>>,
    score: Option<f32>,
    strand: Option<Strand>,

    thick_start: Option<usize>,
    thick_end: Option<usize>,

    item_rgb: Option<rgb::RGB<f32>>,
    /*
    block_count: Option<usize>,
    block_sizes: Option<Vec<usize>>,
    block_starts: Option<Vec<usize>>,
    */
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

fn parse_rgb(field: &[u8]) -> Option<rgb::RGB<f32>> {
    let mut fields = field.split_str(",");

    let ru = parse_next::<u8, _>(&mut fields)?;
    let gu = parse_next::<u8, _>(&mut fields)?;
    let bu = parse_next::<u8, _>(&mut fields)?;

    let rgb_u8 = rgb::RGB::new(ru, gu, bu);

    Some(rgb_u8.into())
}

impl BedRecord {
    pub fn parse_fields<'a, I>(mut fields: I) -> Option<Self>
    where
        I: Iterator<Item = &'a [u8]> + 'a,
    {
        let chrom = fields.next()?;

        let chrom_start = parse_next(&mut fields)?;
        let chrom_end = parse_next(&mut fields)?;

        let name = fields.next().map(|f| f.to_owned());

        let score = parse_next(&mut fields);

        let strand = parse_next(&mut fields);

        let thick_start = parse_next(&mut fields);
        let thick_end = parse_next(&mut fields);

        let item_rgb = {
            let field = fields.next();
            field.and_then(parse_rgb)
        };

        Some(Self {
            chrom: chrom.to_owned(),
            chrom_start,
            chrom_end,

            name,
            score,
            strand,

            thick_start,
            thick_end,

            item_rgb,
        })
    }

    pub fn parse_bed_file<P: AsRef<Path>>(path: P) -> Result<Vec<Self>> {
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

            if let Some(record) = Self::parse_fields(fields) {
                result.push(record);
            }
        }

        Ok(result)
    }
}
