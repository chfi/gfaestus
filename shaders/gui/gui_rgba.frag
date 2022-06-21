#version 450

layout (set = 0, binding = 0) uniform sampler2D u_sampler;

layout (location = 0) in vec4 vs_color;
layout (location = 1) in vec2 vs_uv;

layout (location = 0) out vec4 f_color;

void main() {
  // vec2 uv = vec2(vs_uv.x, vs_uv.y);
  vec4 color = texture(u_sampler, vs_uv);

  f_color = vs_color * color;
}
