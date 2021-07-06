#version 450

#define VERTICES_PER_NODE 2

layout (location = 0) in vec2 position;

layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;

void main() {
  vec4 pos = node_uniform.view_transform * vec4(position, 0.0, 1.0);
  // vec4 pos = vo.view * vec4(position, 0.0, 1.0);
  // gl_Position = vo.view * vec4(position, 0.0, 1.0);

  // NodeIds are 1-indexed
  // int id = 1 + (gl_VertexIndex / VERTICES_PER_NODE);
  // node_id = id;

  // float z = float(node_id) / 1500.0;
  gl_Position = vec4(pos.x, pos.y, 0.0, pos.w);
  // gl_Position = vec4(pos.x, pos.y, 0.6, pos.w);
}
