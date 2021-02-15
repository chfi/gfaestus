#version 450

layout (location = 0) out vec4 f_color;

layout (push_constant) uniform PushConstantData {
  vec4 color;
  uint draw_flags;
  vec4 rect;
  float radius;
  vec2 circle;
  float border;
  vec2 screen_dims;
} pc;

float circ_dist(vec2 pos, float border) {
  float dist = length(pos);
  float radius = dist - border;
  return radius;
}

void main() {
  float x = (gl_FragCoord.x / pc.screen_dims.x) - 0.5;
  float y = (gl_FragCoord.y / pc.screen_dims.y) - 0.5;

  float dist = length(vec2(x, y));

  float alpha = 1.0;

  if (dist > pc.radius) {
    alpha = 0.0;
  }

  f_color = vec4(x, 1.0, 1.0, alpha);
}
