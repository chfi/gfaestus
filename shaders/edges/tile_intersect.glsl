#define TILE_F 16.0
#define TILE_I 16
#define LINE_WIDTH 2.0

#include "geometry.glsl"

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

/*
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
*/

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

vec2 bezier_grid_intersect(in vec2 pixel,
                           in vec2 p0,
                           in vec2 ctrl,
                           in vec2 p1,
                           in float t0,
                           in float t1) {

  vec2 b0 = bezier_quad(p0, ctrl, p1, t0);
  vec2 b1 = bezier_quad(p0, ctrl, p1, t1);

  vec3 b_line;

  if (b0.x == b1.x) {
    b_line = vec3(1.0, 0.0, -b0.x);
  } else {

    float slope = (b1.y - b0.y) / (b1.x - b1.y);
    float intercept = (b1.x * b0.y - b0.x * b1.y) / slope;

    b_line = vec3(1.0, slope, -intercept);
  }

  mat4x3 grid_lines = tile_lines(pixel);

  for (int i = 0; i < 4; i++) {
    vec2 grid_intersect = line_line_intersect(b_line, grid_lines[i]);

    if (grid_intersect != vec2(0.0)) {
      return grid_intersect;
    }
  }

  return vec2(0.0);
}

vec2 tile_line_intersect(in vec2 pixel,
                         in vec3 line) {

  mat4x3 grid_lines = tile_lines(pixel);

  for (int i = 0; i < 4; i++) {
    vec2 grid_intersect = line_line_intersect(line, grid_lines[i]);

    if (grid_intersect != vec2(0.0)) {
      return grid_intersect;
    }
  }

  return vec2(0.0);
}

vec4 tile_line_intersect2(in vec2 pixel,
                          in vec3 line) {
  mat4x3 grid_lines = tile_lines(pixel);

  vec2 intersect0;
  vec2 intersect1;

  float left = float(uint(pixel.x) % 16);
  float right = left + 16;

  float top = float(uint(pixel.y) % 16);
  float bottom = top + 16;

  uint count = 0;

  for (int i = 0; i < 4; i++) {
    vec2 grid_intersect = line_line_intersect(line, grid_lines[i]);

    if (count > 1) {
      break;
    }

    if (grid_intersect != vec2(0.0))
      if (grid_intersect.x >= left && grid_intersect.x <= right
          && grid_intersect.y >= top && grid_intersect.y <= bottom) {
        count += 1;

        if (count == 0) {
          intersect0 = grid_intersect;
        } else {
          intersect1 = grid_intersect;
        }
      }
  }

  return vec4(intersect0.xy, intersect1.xy);

}

/*
vec4 tile_line_intersect2(in vec2 pixel,
                          in vec3 line) {

  mat4x3 grid_lines = tile_lines(pixel);


  vec2 intersect0;
  vec2 intersect1;

  uint count = 0;

  for (int i = 0; i < 4; i++) {
    vec2 grid_intersect = line_line_intersect(line, grid_lines[i]);

    if (grid_intersect != vec2(0.0) &&
        grid_intersect.x >=
      // return grid_intersect;
    }
  }

  return vec2(0.0);
}
*/

/*
void eval_lines_test(out vec4 color, in vec2 pixel_coord) {
    vec3 l0 = vec3(0.0, 1.0, 100.0);
    vec3 l1 = vec3(1.0, -0.45 + (5.5 * sin(iTime)), 200.0);

    vec3 l2 = tile_hor_line_above(fragCoord);

    mat4x3 grid_lines = tile_lines(fragCoord);

    vec2 g0 = line_line_intersect(l1, grid_lines[0]);
    vec2 g1 = line_line_intersect(l1, grid_lines[1]);
    vec2 g2 = line_line_intersect(l1, grid_lines[2]);
    vec2 g3 = line_line_intersect(l1, grid_lines[3]);

    vec2 pt1 = line_line_intersect(l2, l1);
    vec2 pt2 = line_line_intersect(l0, l1);

    vec2 fc = pixel_coord;

    float dl0 = line_sdf2(fc, l0);
    float dl1 = line_sdf2(fc, l1);
    float dl2 = line_sdf2(fc, l2);

    float gl0 = line_sdf2(fc, grid_lines[0]);
    float gl1 = line_sdf2(fc, grid_lines[1]);
    float gl2 = line_sdf2(fc, grid_lines[2]);
    float gl3 = line_sdf2(fc, grid_lines[3]);

    float d1 = length(pt1 - fc) / 10.0;
    float d2 = length(pt2 - fc) / 10.0;

    float dg0 = length(g0 - fc) / 8.0;
    float dg1 = length(g1 - fc) / 8.0;
    float dg2 = length(g2 - fc) / 8.0;
    float dg3 = length(g3 - fc) / 8.0;

    float v = min(dl0, dl1);
    v = min(v, dl2);

    v = min(v, min(dg0, min(dg1, min(dg2, dg3))));
    v = min(v, min(gl0, min(gl1, min(gl2, gl3))));

    fragColor = vec4(v, v, v, 1.0);
}
*/
