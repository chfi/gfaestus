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

  vec2 p = gl_in[0].gl_Position.xy;
  vec2 q = gl_in[1].gl_Position.xy;

  vec2 diff = q - p;

  vec2 n_diff = normalize(diff);
  vec2 rn_diff = vec2(-n_diff.y, n_diff.x);

  vec2 len = (u * p) + (1.0 - u) * q;

  float v_ = mix(-1.0, 1.0, v);

  vec2 offset = rn_diff * v_ * 10.0;

  vec2 pos = len + vec2(0.0, 0.1 * (v - 0.5));

  gl_Position = vec4(pos.xy, 0.0, 1.0);

  // vec4 pos_ = vec4(pos.xy, 0.0, 1.0);

  // gl_Position = vec4(u, v, 0.0, 1.0);

  // gl_Position = vec4(0.0, 0.0, 0.0, 1.0) +


  // gl_Position = len * offset;

  // gl_Position = (u * gl_in[0].gl_Position) + (1.0 - u) * gl_in[1].gl_Position;

  node_id = in_node_id[0];
}
