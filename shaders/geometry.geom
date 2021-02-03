#version 450

layout (lines) in;
layout (triangle_strip, max_vertices = 6) out;

layout (set = 0, binding = 0) uniform View {
  float x;
  float y;
  float zoom;
} vo;

vec2 rotate(vec2 val, float angle) {
  return vec2(val.x * cos(angle) - val.y * sin(angle),
              val.x * sin(angle) + val.y * cos(angle));
}
void build_rectangle(vec4 pos0, vec4 pos1, float width) {
  float angle = atan(pos0.y - pos1.y, pos0.x - pos1.x);

  vec2 pos0_to_pos1 = pos1.xy - pos0.xy;
  vec2 pos0_to_pos1_norm = normalize(pos0_to_pos1);

  vec2 pos1_to_pos0 = pos0.xy - pos1.xy;
  vec2 pos1_to_pos0_norm = normalize(pos1_to_pos0);

  vec2 pos0_orthogonal = rotate(pos0_to_pos1_norm, 3.14159265 / 2.0);
  vec2 pos1_orthogonal = rotate(pos1_to_pos0_norm, 3.14159265 / 2.0);

  vec2 pos0_orthogonal_1 = rotate(pos0_to_pos1_norm, 3.0 * (3.14159265 / 2.0));
  vec2 pos1_orthogonal_1 = rotate(pos1_to_pos0_norm, 3.0 * (3.14159265 / 2.0));

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
  // float width = 1.0 - vo.zoom;
  float width = max(0.01, 0.1 + (1.0 - vo.zoom));
  build_rectangle(gl_in[0].gl_Position, gl_in[1].gl_Position, width);
}



void build_polygon(vec4 position) {
  for (int i=0; i<20; i++) {


    float angle = (i * 3.141592 * 2.0) / 20;
    float x = sin(angle) * 0.1;
    float y = cos(angle) * 0.1;

    gl_Position = position;
    EmitVertex();

    gl_Position = position + vec4(x, y, 0.0, 0.0);
    EmitVertex();


    angle = (((i % 20) + 1) * 3.141592 * 2.0) / 20;
    x = sin(angle) * 0.1;
    y = cos(angle) * 0.1;

    gl_Position = position + vec4(x, y, 0.0, 0.0);
    EmitVertex();
  }
  EndPrimitive();
}
