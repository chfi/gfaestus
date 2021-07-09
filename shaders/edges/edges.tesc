#version 450

layout (vertices = 2) out;


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

void main() {

  float len = length(gl_in[0].gl_Position - gl_in[1].gl_Position);
  float tess = tess_level(len);

  if (gl_InvocationID == 0) {
    gl_TessLevelInner[0] = 1.0;
    gl_TessLevelOuter[0] = 2.0;
    gl_TessLevelOuter[1] = tess;
  }

  gl_out[gl_InvocationID].gl_Position = gl_in[gl_InvocationID].gl_Position;
}
