fn path_steps(path) {
  let graph = get_graph();
  let nodes = [];

  let step = graph.path_first_step(path);

  loop {
    let handle = graph.path_handle_at_step(path, step);
    nodes.push(handle.id());
    if !graph.has_next_step(path, step) {
      break;
    }
    step = graph.next_step(path, step);
  }

  return nodes;
}
