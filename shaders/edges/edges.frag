#version 450

#include "ubo.glsl"

// layout (early_fragment_tests) in;

// flat layout (location = 0) in int node_id;

layout (set = 0, binding = 0) uniform UBOStruct
{
  UBO ubo;
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
  f_color = vec4(0.0, 0.0, 0.0, 1.0);
}
