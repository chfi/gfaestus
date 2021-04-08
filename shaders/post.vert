#version 450

out gl_PerVertex
{
	vec4 gl_Position;
};

layout (push_constant) uniform Dims {
  float width;
  float height;
  uint enabled;
} dims;

void main() {
  vec2 pos = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);

	gl_Position = vec4(pos * 2.0f - 1.0f, 0.0f, 1.0f);
}
