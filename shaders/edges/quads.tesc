#version 450

layout (vertices = 4) out;

layout (std140, set = 0, binding = 0) uniform UBO
{
  vec4 edge_color;
  float edge_width;

  float tess_levels[5];

  float curve_offset;
} ubo;


layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;


float tess_level(float len) {
  if (len < 0.001) {
    return 0.0;
  } else if (len < 0.01) {
    return 4.0;
  } else if (len < 0.05) {
    return 8.0;
  } else if (len < 0.1) {
    return 16.0;
  } else if (len < 0.4) {
    return 24.0;
  } else {
    return 32.0;
  }
}

void main() {

  float len = length(gl_in[0].gl_Position - gl_in[1].gl_Position);

  float tess = tess_level(len);

  gl_TessLevelInner[0] = tess;
  gl_TessLevelInner[1] = tess;

  gl_TessLevelOuter[0] = tess;
  gl_TessLevelOuter[1] = tess;
  gl_TessLevelOuter[2] = tess;
  gl_TessLevelOuter[3] = tess;

  gl_out[gl_InvocationID].gl_Position = gl_in[gl_InvocationID % 2].gl_Position;
}
