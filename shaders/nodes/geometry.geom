#version 450

layout (lines) in;

layout (location = 0) in int[] vs_node_id;

layout (triangle_strip, max_vertices = 4) out;

layout (location = 0) out int node_id;

layout (push_constant) uniform View {
  float node_width;
  float scale;
  vec2 viewport_dims;
  mat4 view;
} vo;


void build_rectangle(int id, vec4 pos0, vec4 pos1) {
  vec2 to_pos1 = vec2(pos1.x - pos0.x, pos1.y - pos0.y);
  vec2 norm_to_pos1 = normalize(to_pos1);

  // float nw_over_vs = vo.node_width / vo.scale;

  float screen_w = vo.viewport_dims.x;
  float screen_h = vo.viewport_dims.y;

  // vec2 width_vec = (vo.node_width / vo.scale) * vec2(1.0 / screen_h, 1.0 / screen_w);

  bool wider = screen_w >= screen_h;
  float ratio = wider ? (screen_w / screen_h) : (screen_h / screen_w);

  vec2 to_pos1_orth;

  if (wider) {
      // to_pos1_orth = vec2(-norm_to_pos1.y * ratio, norm_to_pos1.x);
      to_pos1_orth = vec2(-norm_to_pos1.y, norm_to_pos1.x * ratio);
  } else {
      // to_pos1_orth = vec2(-norm_to_pos1.y, norm_to_pos1.x * ratio);
      to_pos1_orth = vec2(-norm_to_pos1.y * ratio, norm_to_pos1.x);
  }

  vec2 to_pos1_orth_op = -to_pos1_orth;

  node_id = id;

  // float node_width = vo.node_width / (vo.scale * ((screen_w + screen_h) / 2.0));
  float node_width = vo.node_width / (vo.scale * max(screen_w, screen_h));

  gl_Position = pos0 + vec4(to_pos1_orth, 0.0, 0.0) * node_width;
  EmitVertex();

  gl_Position = pos0 + vec4(to_pos1_orth_op, 0.0, 0.0) * node_width;
  EmitVertex();

  gl_Position = pos1 + vec4(to_pos1_orth, 0.0, 0.0) * node_width;
  EmitVertex();

  gl_Position = pos1 + vec4(to_pos1_orth_op, 0.0, 0.0) * node_width;
  EmitVertex();

  EndPrimitive();
}

void main() {
  int id = vs_node_id[0];
  build_rectangle(id, gl_in[0].gl_Position, gl_in[1].gl_Position);
}
