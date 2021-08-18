fn node_color(id) {
  let h = handle(id, false);
  let seq = graph.sequence(h);
  let hash = hash_bytes(seq);
  let color = hash_color(hash);
  color
}
