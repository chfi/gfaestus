fn animate(delay_ms, paths) {
  let graph = get_graph();

  for path in paths {
    let steps = path_steps(path);
    let selection = build_selection(steps);
    set_selection(selection);
    goto_selection();

    thread_sleep(delay_ms);
  }
}

fn anim_follow_path(delay_ms, speed, path) {
  let graph = get_graph();

  let step = graph.path_first_step(path);

  if speed < 100 {
    speed = 100;
  }

  loop {

    let selection = build_selection([]);

    for count in range(0, speed) {
      let handle = graph.path_handle_at_step(path, step);

      selection.add_one(handle.id());

      step = graph.next_step(path, step);
      if !graph.has_next_step(path, step) {
        break;
      }
    }

    set_selection(selection);
    goto_selection();

    thread_sleep(delay_ms);

  }
}