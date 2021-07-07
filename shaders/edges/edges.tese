#version 450

layout (isolines, fractional_odd_spacing, cw) in;

void main() {

  // vec4 modulation = vec4(0.0, sin(gl_TessCoord.x) * 0.05, 0.0, 0.0);

  gl_Position = (gl_TessCoord.x * gl_in[0].gl_Position) +
    (gl_TessCoord.y * gl_in[1].gl_Position) +
    (gl_TessCoord.z * gl_in[2].gl_Position);
    // modulation;
}
