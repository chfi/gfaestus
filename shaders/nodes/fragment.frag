#version 450

flat layout (location = 0) in int node_id;

layout (location = 0) out vec4 f_color;

// layout (set = 0, binding = 0, r32)
// layout (set = 0, binding = 0) uniform writeonly image2D img;
layout (set = 0, binding = 0) buffer Data {
  uint data[];
} data;

layout (push_constant) uniform View {
  float node_width;
  float scale;
  vec2 viewport_dims;
  mat4 view;
} vo;

void main() {
  int color_id = node_id % 7;

  // uint x = uint(vo.viewport_dims.x * gl_FragCoord.x);
  // uint y = uint(vo.viewport_dims.y * gl_FragCoord.y);
  // uint x = uint(gl_FragCoord.x);
  // uint y = uint(gl_FragCoord.y);
  uint w = uint(vo.viewport_dims.x);
  uint h = uint(vo.viewport_dims.y);

  float x = gl_FragCoord.x - (vo.viewport_dims.x / 2.0);
  float y = gl_FragCoord.y - (vo.viewport_dims.y / 2.0);

  uint ix = uint((y * vo.viewport_dims.x) + x);
  data.data[ix] = uint(node_id);

  // data.data[(y * w) + x] = uint(node_id);
  // data.data[(y * w) + h] = 123;
  // data.data[y] = 123;
  // data

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
