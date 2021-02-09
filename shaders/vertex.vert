#version 450

layout (location = 0) in vec2 position;
layout (location = 1) in vec3 color;

layout (location = 0) out vec3 vs_color;

layout (set = 0, binding = 0) uniform View {
  float x;
  float y;
  float zoom;
} vo;

void main() {
  vec2 pos_offset = position + vec2(vo.x, vo.y);
	gl_Position = vec4(pos_offset, 0.0, 1.0);
  vs_color = color;
  // vs_color = vec3(1.0);
	// gl_Position = vec4(pos_offset, 0.0 + vo.zoom * 0.01, 1.0);
	// gl_Position = vec4(position, 0.0 + vo.zoom, 1.0);
	// gl_Position = vec4(position, 0.0, 1.0);
  // vs_color = vec3(1.0);
// layout (location = 3) out vec3 vs_color;
  // vs_color = color;
}
