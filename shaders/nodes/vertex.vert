#version 450

#define VERTICES_PER_NODE 2

layout (location = 0) in vec2 position;
layout (location = 0) out int node_id;
layout (location = 1) out float node_width;

layout (push_constant) uniform View {
  float node_width;
  mat4 view;
} vo;

void main() {
  gl_Position = vo.view * vec4(position, 0.0, 1.0);

  vec4 node_width_vec = vec4(vo.node_width, vo.node_width, 0.0, 0.0);
  node_width_vec = vo.view * node_width_vec;

  node_width = length(node_width_vec.xy);

  int id = gl_VertexIndex / VERTICES_PER_NODE;
  node_id = id;
}
