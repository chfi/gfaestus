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


vec3 edge_ver(vec4 fc, vec2 uv) {

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

  vec3 result = texture(u_color_sampler, uv).rgb * row1[1];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, -1.0))).rgb * row0[0];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, 0.0))).rgb * row0[1];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, 1.0))).rgb * row0[2];

  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(0.0, -1.0))).rgb * row1[0];

  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(0.0, 1.0))).rgb * row1[2];

  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, -1.0))).rgb * row2[0];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, 0.0))).rgb * row2[1];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, 1.0))).rgb * row2[2];

  return result;
}

vec3 edge_hor(vec4 fc, vec2 uv) {

  float row0[3];
  row0[0] = 1.0;
  row0[1] = 2.0;
  row0[2] = 1.0;

  float row1[3];
  row1[0] = 0.0;
  row1[1] = 0.0;
  row1[2] = 0.0;

  float row2[3];
  row2[0] = -1.0;
  row2[1] = -2.0;
  row2[2] = -1.0;

  vec3 result = texture(u_color_sampler, uv).rgb * row1[1];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, -1.0))).rgb * row0[0];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, 0.0))).rgb * row0[1];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(-1.0, 1.0))).rgb * row0[2];

  // result += texture(u_color_sampler, uv_coord(fc.xy + vec2(0.0, -1.0))).rgb * row1[0];

  // result += texture(u_color_sampler, uv_coord(fc.xy + vec2(0.0, 1.0))).rgb * row1[2];

  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, -1.0))).rgb * row2[0];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, 0.0))).rgb * row2[1];
  result += texture(u_color_sampler, uv_coord(fc.xy + vec2(1.0, 1.0))).rgb * row2[2];

  return result;
}

void main() {

  vec2 uv = gl_FragCoord.xy / vec2(dims.width, dims.height);
  vec4 fc = gl_FragCoord;

  vec4 color = texture(u_color_sampler, uv);

  if (dims.enabled) {
    vec3 ver = edge_ver(fc, uv);
    vec3 hor = edge_hor(fc, uv);

    vec3 result = ver + hor;
    // vec3 result = ver;
    // vec3 result = hor;

    float alpha = max(ver.r, max(ver.g, max(ver.b, max(hor.r, max(hor.g, hor.b)))));

    f_color = vec4(result, 1.0);

  } else {
    vec3 result = texture(u_color_sampler, uv).rgb;
    f_color = vec4(result, color.a);
  }
}
