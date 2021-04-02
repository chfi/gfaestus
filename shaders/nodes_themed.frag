#version 450

layout (early_fragment_tests) in;

flat layout (location = 0) in int node_id;

layout (location = 0) out vec4 f_color;
layout (location = 1) out uint f_id;
// layout (location = 1) out vec4 f_mask;

layout (set = 0, binding = 0) uniform sampler1D theme_sampler;

// layout (set = 1, binding = 0) buffer Data {
//   uint data[];
// } data;

// layout (set = 1, binding = 1) readonly buffer Selection {
//   int flag[];
// } selection;


layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;

void main() {
  f_id = uint(node_id);

  float color_u = float((node_id - 1) % node_uniform.texture_period) / node_uniform.texture_period;
  f_color = texture(theme_sampler, color_u);
}
