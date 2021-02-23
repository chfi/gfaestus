#version 450

layout (location = 0) in vec4 color_in;
layout (location = 1) in vec4 mask;

layout (location = 0) out vec4 f_color;



void main() {
  f_color = color_in;
}
