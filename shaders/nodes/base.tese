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

  vec2 diff = q - p;
  vec2 n_diff = normalize(diff);

  vec2 rn_diff = vec2(-n_diff.y, n_diff.x);
  vec4 rot_diff = vec4(rn_diff.xy, 0.0, 0.0);

  vec2 len = (u * p) + (1.0 - u) * q;

  float scaling_ = node_uniform.node_width / node_uniform.scale;
  float scaling = scaling_;
  // float scaling = 1.0 / scaling_;

  rot_diff.x = rot_diff.x * node_uniform.viewport_dims.x;
  rot_diff.y = rot_diff.y * node_uniform.viewport_dims.y;

  vec4 tl = vec4(p.xy, 0.0, 1.0) + rot_diff * scaling;
  vec4 tr = vec4(q.xy, 0.0, 1.0) + rot_diff * scaling;

  vec4 bl = vec4(p.xy, 0.0, 1.0) - rot_diff * scaling;
  vec4 br = vec4(q.xy, 0.0, 1.0) - rot_diff * scaling;

  tl = node_uniform.view_transform * tl;
  tr = node_uniform.view_transform * tr;

  bl = node_uniform.view_transform * bl;
  br = node_uniform.view_transform * br;


  vec4 pos1 = mix(tl, tr, gl_TessCoord.x);
  vec4 pos2 = mix(bl, br, gl_TessCoord.x);
  vec4 pos = mix(pos1, pos2, gl_TessCoord.y);

  gl_Position = pos;

  // gl_Position = node_uniform.view_transform * pos;

  node_id = in_node_id[0];
}
