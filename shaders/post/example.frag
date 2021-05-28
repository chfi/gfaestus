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

void main() {
  vec2 uv = gl_FragCoord.xy / vec2(dims.width, dims.height);

  vec2 uv_flip_x = vec2(1.0 - uv.x, uv.y);

  vec4 color = texture(u_color_sampler, uv_flip_x);

  float max_v = max(color.r, max(color.g, color.g));

  f_color = vec4(0.0, 0.0, 0.0, max_v);
}
