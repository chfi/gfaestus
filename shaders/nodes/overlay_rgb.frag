#version 450

layout (early_fragment_tests) in;

flat layout (location = 0) in int node_id;

layout (location = 0) out vec4 f_color;
layout (location = 1) out uint f_id;
layout (location = 2) out vec4 f_mask;

layout (set = 0, binding = 0) uniform samplerBuffer overlay;

layout (set = 1, binding = 0) readonly buffer Selection {
  uint flag[];
} selection;

layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;

void main() {

  uint is_selected = selection.flag[node_id - 1];

  f_id = uint(node_id);

  if ((is_selected & 1) == 1) {
    f_mask = vec4(1.0, 1.0, 1.0, 1.0);
  } else {
    f_mask = vec4(0.0, 0.0, 0.0, 0.0);
  }


  int color_u = node_id - 1;
  f_color = texelFetch(overlay, color_u);
}
