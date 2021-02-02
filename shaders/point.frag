#version 450

layout(location = 0) out vec4 f_color;

float circ_dist(vec2 pos) {
  float dist = length(pos);
  float radius = dist - 0.3;
  return radius;
}

void main() {
  vec2 local = vec2(gl_PointCoord) + vec2(-0.5, -0.5);

  // f_color = vec4(1.0, 0.0, 0.0, smoothstep(0.02, 0.0, circ_dist(local)));

  float dist = circ_dist(local);

  f_color = vec4(1.0, 0.0, 0.0, smoothstep(0.02, 0.0, abs(dist - 0.025)));

}
