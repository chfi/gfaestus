#version 450

layout (early_fragment_tests) in;

flat layout (location = 0) in int node_id;

layout (location = 0) out vec4 f_color;
layout (location = 1) out vec4 f_mask;

layout (set = 0, binding = 0) buffer Data {
  uint data[];
} data;

layout (set = 0, binding = 1) readonly buffer Selection {
  int flag[];
} selection;

layout (push_constant) uniform View {
  float node_width;
  float scale;
  vec2 viewport_dims;
  mat4 view;
} vo;

void main() {
  int color_id = (node_id - 1) % 7;

  int is_selected = selection.flag[node_id - 1];

  if ((is_selected & 1) == 1) {
    f_mask = vec4(1.0, 1.0, 1.0, 1.0);
  } else {
    f_mask = vec4(0.0, 0.0, 0.0, 0.0);
  }

  uint w = uint(vo.viewport_dims.x);
  uint h = uint(vo.viewport_dims.y);

  float x = floor(gl_FragCoord.x);
  float y = floor(gl_FragCoord.y);

  uint ix = uint((y * vo.viewport_dims.x) + x);
  data.data[ix] = uint(node_id);

  switch (color_id) {
    case 0:
      f_color = vec4(1.0, 0.0, 0.0, 1.0);
      break;
    case 1:
      f_color = vec4(1.0, 0.65, 0.0, 1.0);
      break;
    case 2:
      f_color = vec4(1.0, 1.0, 0.0, 1.0);
      break;
    case 3:
      f_color = vec4(0.0, 0.5, 0.0, 1.0);
      break;
    case 4:
      f_color = vec4(0.0, 0.0, 1.0, 1.0);
      break;
    case 5:
      f_color = vec4(0.3, 0.0, 0.51, 1.0);
      break;
    default:
      f_color = vec4(0.93, 0.51, 0.93, 1.0);
      break;

  }
}
