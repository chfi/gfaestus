#version 450

#define VERTICES_PER_NODE 6

layout (location = 0) in vec2 position;
layout (location = 0) out int node_id;

layout (push_constant) uniform View {
  mat4 view;
} vo;

void main() {
  gl_Position = vo.view * vec4(position, 0.0, 1.0);

  int id = gl_VertexIndex / VERTICES_PER_NODE;
  node_id = id;
}
