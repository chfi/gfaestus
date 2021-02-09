#version 450

layout (lines) in;
layout (location = 0) in vec3[] vs_color_in;

layout (triangle_strip, max_vertices = 6) out;
layout (location = 0) out vec3 gs_color;

layout (set = 0, binding = 0) uniform View {
  float x;
  float y;
  float zoom;
} vo;

vec2 rotate(vec2 val, float angle) {
  return vec2(val.x * cos(angle) - val.y * sin(angle),
              val.x * sin(angle) + val.y * cos(angle));
}
void build_rectangle(vec3 color, vec4 pos0, vec4 pos1, float width) {
  float angle = atan(pos0.y - pos1.y, pos0.x - pos1.x);

  vec2 pos0_to_pos1 = pos1.xy - pos0.xy;
  vec2 pos0_to_pos1_norm = normalize(pos0_to_pos1);

  vec2 pos1_to_pos0 = pos0.xy - pos1.xy;
  vec2 pos1_to_pos0_norm = normalize(pos1_to_pos0);

  vec2 pos0_orthogonal = rotate(pos0_to_pos1_norm, 3.14159265 / 2.0);
  vec2 pos1_orthogonal = rotate(pos1_to_pos0_norm, 3.14159265 / 2.0);

  vec2 pos0_orthogonal_1 = rotate(pos0_to_pos1_norm, 3.0 * (3.14159265 / 2.0));
  vec2 pos1_orthogonal_1 = rotate(pos1_to_pos0_norm, 3.0 * (3.14159265 / 2.0));

  gs_color = color;

  gl_Position = pos0 + vec4(pos0_orthogonal, 0.0, 0.0) * width / 2.0;
  EmitVertex();

  gl_Position = pos0 + vec4(pos0_orthogonal_1, 0.0, 0.0) * width / 2.0;
  EmitVertex();

  gl_Position = pos1 + vec4(pos1_orthogonal_1, 0.0, 0.0) * width / 2.0;
  EmitVertex();

  gl_Position = pos1 + vec4(pos1_orthogonal, 0.0, 0.0) * width / 2.0;
  EmitVertex();

  EndPrimitive();
}

void main() {
  float width = max(0.01, 0.1 + (1.0 - vo.zoom));
  vec3 color = vs_color_in[0];
  build_rectangle(color, gl_in[0].gl_Position, gl_in[1].gl_Position, width);
}
