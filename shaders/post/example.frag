#version 450

layout (set = 0, binding = 0) uniform sampler2D u_color_sampler;

layout (location = 0) out vec4 f_color;

layout (push_constant) uniform Dims {
  vec2 texture_size;
  vec2 screen_size;
  bool enabled;
} dims;

// vec2 uv_coord(vec2 coord) {
//   return (coord / vec2(dims.width, dims.height));
// }

void main() {
  vec2 fc = gl_FragCoord.xy / dims.texture_size;

  vec2 uv = fc;

  vec4 color = texture(u_color_sampler, uv);

  f_color = color;
}
