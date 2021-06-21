#define TILE_F 16.0
#define TILE_I 16
#define LINE_WIDTH 2.0

ivec2 tile_coords(in vec2 pixel_c) {
    return ivec2(pixel_c / TILE_F);
}

vec3 tile_hor_line_above(in vec2 pixel_c) {
    ivec2 tile = tile_coords(pixel_c);

    return vec3(0.0, 1.0, float(tile.y) * TILE_F);
}

vec3 tile_ver_line_left(in vec2 pixel_c) {
    ivec2 tile = tile_coords(pixel_c);

    return vec3(1.0, 0.0, float(tile.x) * TILE_F);
}


mat4x3 tile_lines(in vec2 pixel_c) {
    vec3 hor_above = tile_hor_line_above(pixel_c);
    vec3 hor_below = hor_above + vec3(0.0, 0.0, TILE_F);

    vec3 ver_left = tile_ver_line_left(pixel_c);
    vec3 ver_right = ver_left + vec3(0.0, 0.0, TILE_F);

    mat4x3 res;

    res[0] = hor_above;
    res[1] = hor_below;
    res[2] = ver_left;
    res[3] = ver_right;

    return res;
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

float line_sdf(in vec2 pos, in vec3 line) {
  float x0 = 0.0;
  float y0 = line.y * x0 + line.z;

  float x1 = iResolution.x;
  float y1 = line.y * x1 + line.z;

  return line_segment_sdf(pos, vec2(x0, y0), vec2(x1, y1));
}

float line_sdf2(in vec2 pos, in vec3 line) {
  return abs(pos.x * line.x + pos.y * line.y - line.z) * 0.5;
}

vec3 points_line(in vec2 p0, in vec2 p1) {
   float slope = (p1.y - p0.y) / (p1.x - p0.x);
   float intercept = ((p1.x * p0.y) - (p0.x * p1.y)) / (p1.x - p0.x);

   return vec3(1.0, slope, intercept);
}


vec2 hor_intersect(in uint grid_i, in vec3 line) {
   vec3 grid_line = vec3(0.0, 1.0, -TILE_F * float(grid_i));

   vec3 intersect = cross(line, grid_line);

   if (intersect.z == 0.0) {
       return vec2(0.0);
   } else {
       return vec2(intersect.xy / intersect.z);
   }

}

vec2 line_line_intersect(in vec3 l0, in vec3 l1) {
   vec3 intersect = cross(l0, l1);

   if (intersect.z == 0.0) {
       return vec2(0.0);
   } else {
       return vec2(-intersect.x / intersect.z, -intersect.y / intersect.z);
   }

}

vec2 intersect2(in vec3 l0, in vec3 l1) {
   vec3 intersect = cross(l0, l1);

   float den = intersect.z == 0.0 ? 1.0 : intersect.z;

   return vec2(intersect.xy / den);
}
