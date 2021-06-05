#version 450

layout (set = 0, binding = 0) readonly buffer Pixels {
  uint pixel[];
} pixels;

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

  // vec2 uv = fc;

  // vec4 color = texture(u_color_sampler, uv);

  uvec2 uv = uvec2(fc);

  uint index = (uv.y * uint(dims.texture_size.x)) + uv.x;

  uint pixel = pixels.pixel[index];

  float value = float(pixel) / 255.0;

  vec4 color = vec4(value);

  f_color = color;
}
