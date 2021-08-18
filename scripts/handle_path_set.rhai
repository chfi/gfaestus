fn node_color(id) {
  let h = handle(id, false);

  let steps = graph.steps_on_handle(h);

  let paths = [];
  for step in steps {
    let path = step.path_id;
    paths.push(path);
  }

  paths.sort(|x, y| {
    unwrap_path_id(x) - unwrap_path_id(y)
  });

  let hasher = create_hasher();

  for path in paths {
    hasher.hash(path);
  }

  let hash = hasher.finish();
  let color = hash_color(hash);

  color
}
