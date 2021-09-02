fn animate(delay_ms, paths) {
  let graph = get_graph();

  for path in paths {
    let steps = path_steps(path);
    let selection = build_selection(steps);
    set_selection(selection);
    pan_to_active_selection();

    thread_sleep(delay_ms);
  }
}