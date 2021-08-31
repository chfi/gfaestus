#version 450

layout (std140, set = 0, binding = 0) uniform UBO
{
  vec4 edge_color;
  float edge_width;

  float tess_levels[5];

  float curve_offset;
} ubo;

layout (location = 0) out vec4 f_color;

layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;

void main() {
  f_color = ubo.edge_color;
}
