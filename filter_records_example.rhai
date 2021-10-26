let cseqid = gff3_column("SeqId");
let cstart = gff3_column("Start");
let cend = gff3_column("End");
let cname = gff3_column("Name");
let ctype = gff3_column("Type");

let record_indices = [];

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