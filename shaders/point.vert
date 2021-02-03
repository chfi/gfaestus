#version 450

layout (location = 0) in vec2 position;

layout (set = 0, binding = 0) uniform ViewOffset {
  float x;
  float y;
} vo;

void main() {
  vec2 pos_offset = position + vec2(vo.x, vo.y);
	gl_Position = vec4(pos_offset, 0.0, 1.0);
  gl_PointSize = 100.0;
}
