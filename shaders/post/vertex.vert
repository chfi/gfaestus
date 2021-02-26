#version 450

layout (location = 0) in vec2 position;

layout (push_constant) uniform Dims {
  float width;
  float height;
  bool enabled;
} dims;

void main() {
  gl_Position = vec4(position, 0.0, 1.0);
}
