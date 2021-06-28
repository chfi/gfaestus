vec2 rotate_90_degrees(in vec2 v) {
  return mat2(0.0, 1.0,
              1.0, 0.0) * v;
}

vec2 midpoint(in vec2 p0, in vec2 p1) {
  vec2 diff = p1 - p0;
  return p0 + (diff / 2.0);
}

vec2 edge_control_point(in vec2 p0, in vec2 p1) {
  vec2 mid = midpoint(p0, p1);
  vec2 diff = p1 - p0;

  return mid + rotate_90_degrees(diff / 4.0);
}

vec2 bezier_quad(in vec2 p0, in vec2 ctrl, in vec2 p1, in float t) {
  t = clamp(t, 0.0, 1.0);

  return (1 - t) * ((1 - t) * p0 + t * ctrl)
        + t * ((1 - t) * ctrl + t * p1);

}

int bezier_interval(in vec2 p0, in vec2 ctrl, in vec2 p1) {
  float p0_ctrl = length(ctrl - p0);
  float ctrl_p1 = length(p1 - ctrl);

  return int((p0_ctrl / 8.0) + (ctrl_p1 / 8.0));
}


/*
uint tile_border_index(in vec2 p) {
  uint x = p.x <= 7.5 ? 0 : 15;
  uint y = p.y <= 7.5 ? 0 : 15;

  if (x == 0) {
    return y / 2;
  } else if (x == 15) {
  } else {
  }
}
*/


uint tile_border_index(in vec2 p) {
  uint x = uint(clamp(p.x, 0.0, 15.0));
  uint y = uint(clamp(p.y, 0.0, 15.0));

  if (y == 0) {
    return x / 2;
  } else if (y == 15) {
    return (x / 2) + 16;
  } else if (x == 15) {
    return (y / 2) + 8;
  } else if (x == 0) {
    return (y / 2) + 24;
  } else {
    return 0;
  }
}

ivec2 slot_pixel(in uint slot_ix) {
  if (slot_ix < 8) {
    return ivec2(slot_ix * 2, 0);
  } else if (slot_ix < 16) {
    return ivec2(15, (slot_ix - 8) * 2);
  } else if (slot_ix < 24) {
    return ivec2((slot_ix - 16) * 2, 15);
  } else {
    return ivec2(0, (slot_ix - 24) * 2);
  }
}
