#version 450

layout (set = 0, binding = 0) uniform sampler2D u_color_sampler;

layout (location = 0) out vec4 f_color;

layout (push_constant) uniform Dims {
  float width;
  float height;
  bool enabled;
} dims;

vec2 uv_coord(vec2 coord) {
  return (coord / vec2(dims.width, dims.height));
}


float edge_ver(vec4 fc, vec2 uv) {

  float row0[3];
  row0[0] = 1.0;
  row0[1] = 0.0;
  row0[2] = -1.0;

  float row1[3];
  row1[0] = 2.0;
  row1[1] = 0.0;
  row1[2] = -2.0;

  float row2[3];
  row2[0] = 1.0;
  row2[1] = 0.0;
  row2[2] = -1.0;

  float result = texture(u_color_sampler, uv).r * row1[1];

  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, -1.0))).r * row0[0];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, 1.0))).r * row0[2];

  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(0.0, -1.0))).r * row1[0];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(0.0, 1.0))).r * row1[2];

  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, -1.0))).r * row2[0];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, 1.0))).r * row2[2];

  return result;
}

float edge_hor(vec4 fc, vec2 uv) {

  float row0[3];
  row0[0] = 1.0;
  row0[1] = 2.0;
  row0[2] = 1.0;

  float row2[3];
  row2[0] = -1.0;
  row2[1] = -2.0;
  row2[2] = -1.0;

  float result = 0.0;

  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, -1.0))).r * row0[0];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, 0.0))).r * row0[1];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, 1.0))).r * row0[2];

  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, -1.0))).r * row2[0];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, 0.0))).r * row2[1];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, 1.0))).r * row2[2];

  return result;
}

void main() {

  vec2 uv = gl_FragCoord.xy / vec2(dims.width, dims.height);
  vec4 fc = gl_FragCoord;

  vec4 color = texture(u_color_sampler, uv);

  if (dims.enabled) {
    float ver = abs(edge_ver(fc, uv));
    float hor = abs(edge_hor(fc, uv));

    float result = max(hor, ver);

    f_color = vec4(result, result, result, result);

  } else {
    f_color = color;
  }
}
