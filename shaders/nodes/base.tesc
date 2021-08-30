#version 450

layout (vertices = 4) out;

layout (location = 0) in int[] vs_node_id;

layout (location = 0) out int[] node_id;

layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;


void main() {

  gl_TessLevelInner[0] = 1.0;
  gl_TessLevelInner[1] = 1.0;

  gl_TessLevelOuter[0] = 1.0;
  gl_TessLevelOuter[1] = 1.0;
  gl_TessLevelOuter[2] = 1.0;
  gl_TessLevelOuter[3] = 1.0;

  node_id[gl_InvocationID] = vs_node_id[gl_InvocationID % 2];
  gl_out[gl_InvocationID].gl_Position = gl_in[gl_InvocationID % 2].gl_Position;
}
