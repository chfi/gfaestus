#version 450

layout (isolines, equal_spacing, ccw) in;

void main() {
  float u = gl_TessCoord.x;
  float v = gl_TessCoord.y;

  float modulation = sin(u * 3.141592) * 0.03;

  gl_Position = (u * gl_in[0].gl_Position) + ((1.0 - u) * gl_in[1].gl_Position) + vec4(0.0, modulation, 0.0, 0.0);
}
