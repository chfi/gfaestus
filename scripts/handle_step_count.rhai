fn node_color(id) {
  let h = handle(id, false);

  let steps = graph.steps_on_handle(h);
  let count = 0.0;

  for step in steps {
    count += 1.0;
  }

  count
}