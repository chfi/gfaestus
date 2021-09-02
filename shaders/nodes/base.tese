#version 450

// layout (quads, equal_spacing, ccw) in;
// layout (isolines, equal_spacing, ccw) in;
layout (quads, equal_spacing, ccw) in;

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

  float node_width = node_uniform.node_width / (node_uniform.scale *
                                                max(node_uniform.viewport_dims.x,
                                                    node_uniform.viewport_dims.y));

  vec4 p_ = node_uniform.view_transform * gl_in[0].gl_Position;
  vec4 q_ = node_uniform.view_transform * gl_in[1].gl_Position;

  vec2 diff = q_.xy - p_.xy;
  vec2 n_diff = normalize(diff);

  vec2 rn_diff = vec2(-n_diff.y, n_diff.x);
  vec4 rot_diff = vec4(rn_diff.xy, 0.0, 0.0);

  vec4 tl = p_ + rot_diff * node_width;
  vec4 tr = p_ - rot_diff * node_width;
  vec4 bl = q_ + rot_diff * node_width;
  vec4 br = q_ - rot_diff * node_width;

  vec4 pos1 = mix(tl, tr, gl_TessCoord.x);
  vec4 pos2 = mix(bl, br, gl_TessCoord.x);
  vec4 pos = mix(pos1, pos2, gl_TessCoord.y);

  gl_Position = pos;
  // gl_Position = node_uniform.view_transform * pos;

  node_id = in_node_id[0];
}
