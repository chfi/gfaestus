#version 450

layout (location = 0) in vec3 vs_color;

layout (location = 0) out vec4 f_color;

void main() {
  f_color = vec4(1.0, 1.0, 1.0, 1.0);
  // f_color = vec4(gs_color, 1.0);
}
