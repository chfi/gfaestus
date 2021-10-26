// replace the path and names as appropriate
// these assignments aren't necessary, just make the code cleaner
load_collection("/home/christian/data/Homo_sapiens.GRCh38.103.chr.gff3");
let gff3_name = "Homo_sapiens.GRCh38.103.chr.gff3";
let ref_path_name = "grch38#chr6:28510128-33480000";

// all these assignments aren't necessary, just make the code cleaner
let graph = get_graph(); let ref_path =
graph.get_path_id(ref_path_name); let coll =
get_collection(gff3_name);

let cseqid = gff3_column("SeqId");
let cstart = gff3_column("Start");
let cend = gff3_column("End");
let cname = gff3_column("Name");
let ctype = gff3_column("Type");

let record_indices = [];

// this loop filters the records by chromosome and position
for ix in range(0, coll.len()) {
  let record = coll.get_record(ix);

  let seq_id = record.get(cseqid);

  if seq_id == ["6"] {
    let start = record.get(cstart);
    let end = record.get(cend);

    if start >= 28510128 && end <= 33480000 {

      let type_ = record.get(ctype);

      if type_ == ["gene"] {
        record_indices.push(ix);
      }
    }
  }
}

// this function takes an annotation collection, a filtered list of
// record indices, a reference path ID, a column to use, and the name of
// the label set

create_label_set(coll, record_indices, ref_path, cname, "genes");