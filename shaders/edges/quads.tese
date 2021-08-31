#version 450

layout (quads, fractional_odd_spacing, ccw) in;

layout (std140, set = 0, binding = 0) uniform UBO
{
  // UBO ubo;
  vec4 edge_color;
  float edge_width;

  float tess_levels[5];

  float curve_offset;
} ubo;

layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;

float curve_modulation(float x) {
  return -0.8 * (x * x - x);
}

vec2 norm_diff(vec2 v0, vec2 v1) {
  vec2 diff = v1 - v0;
  return mat2x2(0.0, 1.0, -1.0, 0.0) * diff;
}

void main() {
  float u = gl_TessCoord.x;
  float v = gl_TessCoord.y;

  vec2 p = gl_in[0].gl_Position.xy;
  vec2 q = gl_in[1].gl_Position.xy;

  vec4 p_ = node_uniform.view_transform * gl_in[0].gl_Position;
  vec4 q_ = node_uniform.view_transform * gl_in[1].gl_Position;

  vec2 diff = q_.xy - p_.xy;
  vec2 n_diff = normalize(diff);

  vec2 rn_diff = vec2(-n_diff.y, n_diff.x);
  vec4 rot_diff = vec4(rn_diff.xy, 0.0, 0.0);

  // float edge_width = ubo.edge_width / (node_uniform.scale * max(node_uniform.viewport_dims.x,
  //                                                               node_uniform.viewport_dims.y));

  // float edge_width = ubo.edge_width / (node_uniform.scale * 100.0);
  // float edge_width = ubo.edge_width / 1000.0;
  float edge_width = ubo.edge_width / max(node_uniform.viewport_dims.x,
                                          node_uniform.viewport_dims.y);

  vec4 tl = p_ + rot_diff * edge_width;
  vec4 tr = p_ - rot_diff * edge_width;
  vec4 bl = q_ + rot_diff * edge_width;
  vec4 br = q_ - rot_diff * edge_width;

  vec4 pos1 = mix(tl, tr, gl_TessCoord.x);
  vec4 pos2 = mix(bl, br, gl_TessCoord.x);
  vec4 pos = mix(pos1, pos2, gl_TessCoord.y);

  vec2 curvature = curve_modulation(v) *
                   norm_diff(p_.xy, q_.xy);


  gl_Position = pos + vec4(curvature, 0.0, 0.0);
}
