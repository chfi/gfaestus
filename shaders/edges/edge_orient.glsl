uvec2 oriented_edge_ixs(uvec2 handles) {

  uint left_id = handles.x >> 1;
  uint right_id = handles.y >> 1;

  bool left_fwd = (handles.x & 1) != 1;
  bool right_fwd = (handles.y & 1) != 1;

  uint left_ix;
  uint right_ix;

  if (left_fwd && right_fwd) {
    left_ix = handles.x + 1;
    right_ix = handles.y;
  } else if (!left_fwd && right_fwd) {
    left_ix = handles.x;
    right_ix = handles.y - 1;
  } else if (left_fwd && !right_fwd) {
    left_ix = handles.x;
    right_ix = handles.y;
  } else if (!left_fwd && !right_fwd) {
    left_ix = handles.x;
    right_ix = handles.y + 1;
  }

  return uvec2(left_ix, right_ix);
}
