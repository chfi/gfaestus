#define TILE_F 16.0
#define TILE_I 16
#define LINE_WIDTH 2.0

ivec2 tile_coords(in vec2 pixel_c) {
    return ivec2(pixel_c / TILE_F);
}

vec2 pixel_to_local(in ivec2 tile, in vec2 pixel) {
    return pixel - vec2(tile * TILE_I);
}

float line_segment_sdf(in vec2 pos, in vec2 p0, in vec2 p1) {
  vec2 pos_0 = pos - p0;
  vec2 pos_1 = p1 - p0;

  float h = clamp(dot(pos_0, pos_1) / dot(pos_1, pos_1), 0.0, 1.0);
  return length(pos_0 - pos_1*h) - LINE_WIDTH;
}

vec4 tile_sdf(in ivec2 tile) {
    float x0 = 0.02 * float(tile.x) * float(tile.x);

    float v = sin(float(tile.y) * 0.2);
    float y0 = 0.5 + (v * 0.5) * float(tile.y) / 2.0;

    float x1 = 16.0 - (0.02 * float(tile.x) * float(tile.x));


    float v2 = sin(float(tile.y) * 0.2);

    float y1 = TILE_F - (0.5 + (v2 * 0.5) * float(tile.y) / 2.0);

    return vec4(x0, y0, x1, y1);
}

vec4 tile_curve_sdf(in ivec2 tile) {
    float x0 = (TILE_F / 2.0) + cos(float(tile.x) / 10.0) * (TILE_F / 2.0);
    float y0 = (TILE_F / 2.0) + sin(float(tile.y) / 10.0) * (TILE_F / 2.0);

    float x1 = (TILE_F / 2.0) + cos(float(tile.x + 1) / 10.0) * (TILE_F / 2.0);
    float y1 = (TILE_F / 2.0) + sin(float(tile.y + 1) / 10.0) * (TILE_F / 2.0);

    return vec4(x0, y0, x1, y1);
}

vec3 points_line(in vec2 p0, in vec2 p1) {
   float slope = (p1.y - p0.y) / (p1.x - p0.x);
   float intercept = ((p1.x * p0.y) - (p0.x * p1.y)) / (p1.x - p0.x);

   return vec3(1.0, slope, intercept);
}

vec2 hor_intersect(in uint grid_i, in vec3 line) {
   vec3 grid_line = vec3(0.0, 1.0, -16.0 * float(grid_i));

   vec3 intersect = cross(line, grid_line);

   if (intersect.z == 0.0) {
       return vec2(0.0);
   } else {
       return vec2(intersect.xy / intersect.z);
   }
}
