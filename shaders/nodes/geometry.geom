#version 450

layout (lines) in;

layout (location = 0) in int[] vs_node_id;
layout (location = 1) in float[] vs_node_width;

layout (triangle_strip, max_vertices = 4) out;

layout (location = 0) out int node_id;

layout (push_constant) uniform View {
  float node_width;
  mat4 view;
} vo;

vec2 rotate(vec2 val, float angle) {
  return vec2(val.x * cos(angle) - val.y * sin(angle),
              val.x * sin(angle) + val.y * cos(angle));
}

void build_rectangle(float width, int id, vec4 pos0, vec4 pos1) {
  vec2 pos0_to_pos1 = pos0.xy - pos1.xy;
  vec2 pos0_to_pos1_norm = normalize(pos0_to_pos1);

  vec2 pos0_orthogonal = rotate(pos0_to_pos1_norm, 3.14159265 / 2.0);

  node_id = id;

  gl_Position = pos0 + vec4(pos0_orthogonal, 0.0, 0.0) * (width / 2.0);
  EmitVertex();

  gl_Position = pos0 + vec4(pos0_orthogonal, 0.0, 0.0) * (-width / 2.0);
  EmitVertex();

  gl_Position = pos1 + vec4(pos0_orthogonal, 0.0, 0.0) * (width / 2.0);
  EmitVertex();

  gl_Position = pos1 + vec4(pos0_orthogonal, 0.0, 0.0) * (-width / 2.0);
  EmitVertex();

  EndPrimitive();
}

void main() {
  int id = vs_node_id[0];
  float width = vs_node_width[0];
  build_rectangle(width, id, gl_in[0].gl_Position, gl_in[1].gl_Position);
}
