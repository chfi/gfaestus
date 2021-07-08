#version 450

layout (isolines, equal_spacing, ccw) in;

void main() {
  float u = gl_TessCoord.x;
  float v = gl_TessCoord.y;

  gl_Position = (u * gl_in[0].gl_Position) + ((1.0 - u) * gl_in[1].gl_Position);
}
