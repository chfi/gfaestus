#version 450

layout (isolines, equal_spacing, ccw) in;

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

  vec2 curvature = curve_modulation(u) *
                   norm_diff(gl_in[0].gl_Position.xy,
                             gl_in[1].gl_Position.xy);


  gl_Position = (u * gl_in[0].gl_Position) +
                ((1.0 - u) * gl_in[1].gl_Position) +
                vec4(curvature, 0.0, 0.0);
}
