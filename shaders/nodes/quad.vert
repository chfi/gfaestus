#version 450

#define VERTICES_PER_NODE 6

layout (location = 0) in vec2 position;
layout (location = 0) out int node_id;

layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;

void main() {

  int id = 1 + (gl_VertexIndex / VERTICES_PER_NODE);
  node_id = id;

  int vx_mod = gl_VertexIndex % VERTICES_PER_NODE;

  // assuming the node is seen lying horizontally from left to right,
  // 0 -> bottom left
  // 1 -> top right
  // 2 -> top left
  //
  // 3 -> bottom left
  // 4 -> bottom right
  // 5 -> top right

  vec2 offset;

  switch (vx_mod) {
    case 0:
      offset = vec2(-0.05, -0.05);
      break;
    case 1:
      offset = vec2(0.05, 0.05);
      break;
    case 2:
      offset = vec2(-0.05, 0.05);
      break;
    case 3:
      offset = vec2(-0.05, -0.05);
      break;
    case 4:
      offset = vec2(0.05, -0.05);
      break;
    case 5:
      offset = vec2(0.05, 0.05);
      break;
    default:
      offset = vec2(-0.05, -0.05);
      break;
  }

  gl_Position = vec4(position.xy + offset, 0.0, 1.0);
}
