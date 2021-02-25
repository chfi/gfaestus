#version 450

layout (set = 0, binding = 0) uniform sampler2D u_color_sampler;

layout (location = 0) out vec4 f_color;

layout (push_constant) uniform Dims {
  float width;
  float height;
} dims;

void main() {
  vec2 uv = gl_FragCoord.xy / vec2(dims.width, dims.height);
  vec4 color = texture(u_color_sampler, uv);

  f_color = color;
}
