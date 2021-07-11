#version 450

// #include "ubo.glsl"

layout (vertices = 2) out;


// struct UBO {
//   vec4 edge_color;
//   float edge_width;

//   float tess_levels[5];

//   float curve_offset;
// };

layout (std140, set = 0, binding = 0) uniform UBO
{
  // UBO ubo;
  vec4 edge_color;
  // float edge_width;

  // float tess_levels[5];

  // float curve_offset;
} ubo;


layout (push_constant) uniform NodePC {
  mat4 view_transform;
  float node_width;
  float scale;
  vec2 viewport_dims;
  uint texture_period;
} node_uniform;


// TODO make this configurable via a UBO
float tess_level(float len) {
  if (len < 0.01) {
    return 2.0;
  } else if (len < 0.05) {
    return 3.0;
  } else if (len < 0.1) {
    return 5.0;
  } else if (len < 0.4) {
    return 8.0;
  } else {
    return 16.0;
  }
}

uint tess_level_ix(float len) {
  if (len < 0.01) {
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

  // float tess = ubo.tess_levels[tess_level_ix(len)];
  // float tess = tess_level(len);

  float tess = ubo.edge_color.r;

  if (gl_InvocationID == 0) {
    gl_TessLevelInner[0] = 1.0;
    gl_TessLevelOuter[0] = 2.0;
    gl_TessLevelOuter[1] = tess;
  }

  gl_out[gl_InvocationID].gl_Position = gl_in[gl_InvocationID].gl_Position;
}
