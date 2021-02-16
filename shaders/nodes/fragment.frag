#version 450

flat layout (location = 0) in int node_id;

layout (location = 0) out vec4 f_color;

void main() {
  int color_id = node_id % 7;

  switch (color_id) {
    case 0:
      f_color = vec4(1.0, 0.0, 0.0, 1.0);
      break;
    case 1:
      f_color = vec4(1.0, 0.65, 0.0, 1.0);
      break;
    case 2:
      f_color = vec4(1.0, 1.0, 0.0, 1.0);
      break;
    case 3:
      f_color = vec4(0.0, 0.5, 0.0, 1.0);
      break;
    case 4:
      f_color = vec4(0.0, 0.0, 1.0, 1.0);
      break;
    case 5:
      f_color = vec4(0.3, 0.0, 0.51, 1.0);
      break;
    default:
      f_color = vec4(0.93, 0.51, 0.93, 1.0);
      break;

  }
}
