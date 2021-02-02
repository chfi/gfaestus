#version 450

layout (points) in;
layout (triangle_strip, max_vertices = 256) out;

void build_polygon(vec4 position) {
  for (int i=0; i<20; i++) {


    float angle = (i * 3.141592 * 2.0) / 20;
    float x = sin(angle) * 0.1;
    float y = cos(angle) * 0.1;

    gl_Position = position;
    EmitVertex();

    gl_Position = position + vec4(x, y, 0.0, 0.0);
    EmitVertex();


    angle = (((i % 20) + 1) * 3.141592 * 2.0) / 20;
    x = sin(angle) * 0.1;
    y = cos(angle) * 0.1;

    gl_Position = position + vec4(x, y, 0.0, 0.0);
    EmitVertex();

    // if i == 4 {
    //   float angle = ((i + 1) * 3.141592 * 2.0) / 5;
    //   float x = sin(angle) * 0.1;
    //   float y = cos(angle) * 0.1;
    // } else {
    //   float angle = ((i + 1) * 3.141592 * 2.0) / 5;
    //   float x = sin(angle) * 0.1;
    //   float y = cos(angle) * 0.1;
    // }
  }
  EndPrimitive();
}

void main() {
  build_polygon(gl_in[0].gl_Position);
  // build_house(gl_in[0].gl_Position);
  // gl_Position = gl_in[0].gl_Position + vec4(-0.1, 0.0, 0.0, 0.0);
  // EmitVertex();

  // gl_Position = gl_in[0].gl_Position + vec4( 0.1, 0.0, 0.0, 0.0);
  // EmitVertex();

  // EndPrimitive();
}
