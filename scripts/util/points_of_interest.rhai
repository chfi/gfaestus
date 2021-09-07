fn try_initialize() {
  try {
    let counter = get_var("_count");
    let points = get_var("_points");
  }
  catch {
    set_var("_count", 0);
    set_var("_points", []);
  }
}

fn add_point_of_interest() {
  try_initialize();

  let point = get_selection_center();
  let id = label_count().to_string();
  let label_text = id;

  let points = get_var("_points");
  points.push(point);

  set_var("_points", points);

  add_label(id, label_text, point);
}

fn next_point_of_interest() {
  try_initialize();

  let ix = get_var("_count");
  let points = get_var("_points");

  try {
    let pt = points[ix];
    set_view_origin(pt);

    let new_ix = ix + 1;
    if new_ix >= points.len() {
      new_ix = 0;
    }

    set_var("_count", new_ix);
  }
  catch {
  }
}
