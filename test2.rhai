let old_color = get("background_color_light");
set("background_color_light", rgb(old_color.r, old_color.g, cos(time_since_start * 0.2)));