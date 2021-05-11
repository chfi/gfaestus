use gluon_codegen::*;

use gluon::vm::{
    api::{FunctionRef, Hole, OpaqueValue},
    ExternModule,
};
use gluon::*;
use gluon::{base::types::ArcType, import::add_extern_module};

use std::path::Path;

use bstr::{ByteSlice, ByteVec};

use anyhow::Result;

/// Need a newtype wrapper for RGB to be able to derive the gluon traits
#[derive(Debug, Clone, Copy, PartialEq, Trace, VmType, Userdata)]
#[gluon_trace(skip)]
#[gluon(vm_type = "RGB")]
pub struct RGB(pub rgb::RGB<f32>);

impl RGB {
    pub fn r(&self) -> f32 {
        self.0.r
    }

    pub fn g(&self) -> f32 {
        self.0.g
    }

    pub fn b(&self) -> f32 {
        self.0.b
    }

    pub fn rgb(&self) -> (f32, f32, f32) {
        (self.0.r, self.0.g, self.0.b)
    }
}

impl From<rgb::RGB<f32>> for RGB {
    fn from(rgb: rgb::RGB<f32>) -> Self {
        RGB(rgb)
    }
}

impl Into<rgb::RGB<f32>> for RGB {
    fn into(self) -> rgb::RGB<f32> {
        self.0
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Trace, VmType, Getable, Pushable,
)]
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

#[derive(Debug, Clone, Trace, VmType, Userdata)]
pub struct BedRecord {
    chrom: Vec<u8>,
    chrom_start: usize,
    chrom_end: usize,

    name: Option<Vec<u8>>,
    score: Option<f32>,
    strand: Option<Strand>,

    thick_start: Option<usize>,
    thick_end: Option<usize>,

    item_rgb: Option<RGB>,
    /*
    block_count: Option<usize>,
    block_sizes: Option<Vec<usize>>,
    block_starts: Option<Vec<usize>>,
    */
}

impl BedRecord {
    pub fn chrom(&self) -> &[u8] {
        &self.chrom
    }

    pub fn chrom_start(&self) -> usize {
        self.chrom_start
    }

    pub fn chrom_end(&self) -> usize {
        self.chrom_end
    }

    pub fn name(&self) -> Option<&[u8]> {
        self.name.as_ref().map(|n| n.as_slice())
    }

    pub fn score(&self) -> Option<f32> {
        self.score
    }

    pub fn item_rgb(&self) -> Option<(f32, f32, f32)> {
        self.item_rgb.map(|rgb| rgb.rgb())
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

fn parse_rgb(field: &[u8]) -> Option<RGB> {
    let mut fields = field.split_str(",");

    let ru = parse_next::<u8, _>(&mut fields)?;
    let gu = parse_next::<u8, _>(&mut fields)?;
    let bu = parse_next::<u8, _>(&mut fields)?;

    let rgb_u8 = rgb::RGB::new(ru, gu, bu);

    Some(RGB(rgb_u8.into()))
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

fn parse_bed_file(path: &str) -> Option<Vec<BedRecord>> {
    BedRecord::parse_bed_file(path).ok()
}

fn parse_bed_file_unwrap(path: &str) -> Vec<BedRecord> {
    BedRecord::parse_bed_file(path).unwrap()
}

pub(super) fn bed_module(thread: &Thread) -> vm::Result<ExternModule> {
    thread.register_type::<BedRecord>("BedRecord", &[])?;
    thread.register_type::<RGB>("RGB", &[])?;

    let module = record! {
        type BedRecord => BedRecord,

        parse_bed_file => primitive!(1, parse_bed_file),
        parse_bed_file_unwrap => primitive!(1, parse_bed_file_unwrap),

        rgb => primitive!(1, RGB::rgb),

        chrom => primitive!(1, BedRecord::chrom),
        chrom_start => primitive!(1, BedRecord::chrom_start),
        chrom_end => primitive!(1, BedRecord::chrom_end),

        name => primitive!(1, BedRecord::name),
        score => primitive!(1, BedRecord::score),
        item_rgb => primitive!(1, BedRecord::item_rgb),
    };

    ExternModule::new(thread, module)
}
