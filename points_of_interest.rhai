fn try_initialize() {
  try {
    let counter = get_var("_count");
    let points = get_var("_points");
  }
  catch {
    set_var("_count", 0);
    let loop_pt = Point(4093620.0, 306494.8);
    let left_pt = Point(1434572.3, 61577.25);
    set_var("_points", [loop_pt, left_pt]);
  }
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