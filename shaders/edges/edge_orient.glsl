uvec2 oriented_edge_ixs(uvec2 handles) {

  uint left_id   = (handles.x >> 1) - 1;
  uint right_id  = (handles.y >> 1) - 1;

  bool left_rev  = (handles.x & 1) == 1;
  bool right_rev = (handles.x & 1) == 1;


  uint left_l = left_id * 2;
  uint left_r = left_l + 1;

  uint right_l = right_id * 2;
  uint right_r = right_l + 1;


  uint left_ix;
  uint right_ix;

  if (!left_rev && !right_rev) {
    left_ix = left_r;
    right_ix = right_l;

  } else if (left_rev && !right_rev) {
    left_ix = left_l;
    right_ix = right_l;

  } else if (!left_rev && right_rev) {
    left_ix = left_r;
    right_ix = right_r;

  } else if (left_rev && right_rev) {
    left_ix = left_l;
    right_ix = right_r;

  }

  return uvec2(left_ix, right_ix);
}
