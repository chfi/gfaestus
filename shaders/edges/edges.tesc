#version 450

layout (vertices = 2) out;

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


int tess_level_ix(float len) {
  if (len < 0.001) {
    return -1;
  } else if (len < 0.01) {
    return 0;
  } else if (len < 0.05) {
    return 1;
  } else if (len < 0.1) {
    return 2;
  } else if (len < 0.4) {
    return 3;
  } else {
    return 4;
  }
}

void main() {

  float len = length(gl_in[0].gl_Position - gl_in[1].gl_Position);

  int index = tess_level_ix(len);

  if (index == -1) {
    gl_TessLevelOuter[0] = 0.0;
  } else {
    float tess = ubo.tess_levels[tess_level_ix(len)];
    if (gl_InvocationID == 0) {
      gl_TessLevelInner[0] = 1.0;
      gl_TessLevelOuter[0] = 2.0;
      gl_TessLevelOuter[1] = tess;
    }
  }

  gl_out[gl_InvocationID].gl_Position = gl_in[gl_InvocationID].gl_Position;
}
