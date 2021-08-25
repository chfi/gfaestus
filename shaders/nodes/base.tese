#version 450

layout (quads, fractional_odd_spacing, ccw) in;

layout (location = 0) in int[] in_node_id;

layout (location = 0) out int node_id;

layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;

void main() {
  float u = gl_TessCoord.x;
  float v = gl_TessCoord.y;

  gl_Position = (u * gl_in[0].gl_Position) + (1.0 - u) * gl_in[1].gl_Position;

  node_id = in_node_id[0];
}
