#version 450

layout (location = 0) in vec2 position;
layout (location = 1) in vec3 color;

layout (location = 0) out vec3 vs_color;

layout (set = 0, binding = 0) uniform View {
  mat4 view;
} vo;

void main() {
  gl_Position = vo.view * vec4(position, 0.0, 1.0);

  vs_color = color;
}
