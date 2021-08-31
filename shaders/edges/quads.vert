#version 450

layout (location = 0) in vec2 position;

layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;

void main() {
  gl_Position = vec4(position.xy, 0.0, 1.0);
}
