#version 450

layout (location = 0) in vec2 position;
layout (location = 1) in vec3 color;

layout (location = 3) out vec3 vs_color;

void main() {
	gl_Position = vec4(position, 0.0, 1.0);
  vs_color = vec3(1.0);
// layout (location = 3) out vec3 vs_color;
  // vs_color = color;
}
