#version 450
// #extension GL_EXT_debug_printf : enable

layout (location = 0) in vec2 pos;
layout (location = 1) in vec2 uv;
layout (location = 2) in vec4 color;

layout (location = 0) out vec4 vs_color;
layout (location = 1) out vec2 vs_uv;

layout (push_constant) uniform ScreenSize {
  float width;
  float height;
} screen_size;

// layout (set = 0, binding = 0) uniform View {
//   mat4 view;
// } vo;

// taken from the egui glium example
    // 0-1 linear  from  0-255 sRGB
vec3 linear_from_srgb(vec3 srgb) {
  bvec3 cutoff = lessThan(srgb, vec3(10.31475));
  vec3 lower = srgb / vec3(3294.6);
  vec3 higher = pow((srgb + vec3(14.025)) / vec3(269.025), vec3(2.4));
  return mix(higher, lower, cutoff);
}

vec4 linear_from_srgba(vec4 srgba) {
  vec3 srgb = srgba.xyz * 255.0;
  return vec4(linear_from_srgb(srgb), srgba.a);
}

void main() {
  // gl_Position = vec4(position, 0.0, 1.0);

  // gl_Position = vec4(4.0 * (pos.x / u_screen_size.width),
  // gl_Position = vec4(4.0 * (pos.x / u_screen_size.width),
  //                    4.0 * (pos.y / u_screen_size.height),
  // gl_Position = vec4(pos.x / 10000.0,
  //                    pos.y / 10000.0,
                     // 1.0 - 2.0 * pos.y / u_screen_size.height,
                     // 2.0 * pos.y / u_screen_size.height,
                     // 0.0,
                     // 1.0);

  // gl_Position = vec4(pos.x, pos.y, 0.0, 1.0);

  gl_Position = vec4(
                     2.0 * pos.x / screen_size.width - 1.0,
                     2.0 * pos.y / screen_size.height - 1.0,
                     0.0,
                     1.0);

  // vs_color = linear_from_

  // vs_color = color;
  // vs_color = color;
  vs_color = linear_from_srgba(color);
  vs_uv = uv;
}
