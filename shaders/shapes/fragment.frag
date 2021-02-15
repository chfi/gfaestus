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

float circle(float dist, float radius, float border) {
  return 1.0 - smoothstep(radius - (radius * border),
                          radius + (radius * border),
                          dist);
}


float circ_dist(vec2 pos, float border) {
  float dist = length(pos);
  float radius = dist - border;
  return radius;
}

void main() {
  float x = (gl_FragCoord.x / pc.screen_dims.x) - 0.5;
  float y = (gl_FragCoord.y / pc.screen_dims.y) - 0.5;

  f_color = vec4(1.0, 1.0, 1.0, 0.0);

  if ((pc.draw_flags & 1) == 1) {
    // TODO take viewport ratio into account
    float radius = pc.radius / pc.screen_dims.x;

    vec2 circle_center = (pc.circle / pc.screen_dims);

    float dist = distance(circle_center, vec2(x, y));

    float border_dist = dist - radius;

    // float alpha = smoothstep(0.02, 0.0, abs(dist - pc.border));
    // vec3 color = vec3(circle(dist, pc.radius, pc.border));
    // vec3 border_dist = vec3(circle(dist, pc.radius, pc.border));

    float color = 1.0 - smoothstep(0.0, pc.border, abs(border_dist));

    // if ((pc.draw_flags & (1 << 2)) != 0) {
    //   color = 1.0 - color;
    // }
    f_color = vec4(1.0, 1.0, 1.0, color);
  }

  if ((pc.draw_flags & (1 << 1)) != 0) {
    vec2 top_left = pc.rect.yx / pc.screen_dims;
    vec2 bottom_right = pc.rect.wz / pc.screen_dims;

    vec2 diff = bottom_right - top_left;

    vec2 local = vec2(x, y) - top_left;

    vec2 d = abs(local) - diff;

    float dist = length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);

    float border_dist = dist;

    float color = 1.0 - smoothstep(0.0, pc.border, abs(border_dist));

    f_color = vec4(1.0, 1.0, 1.0, color);
  }

}
