#version 450

layout (location = 0) out vec4 f_color;

layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  // uint flags;
} node_uniform;


void main() {
  f_color = vec4(0.7, 0.7, 0.7, 1.0);
}
