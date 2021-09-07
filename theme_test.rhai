// initializes a global counter variable, and uses it to step through
// three different background colors

fn next_theme() {

  let bg_index = 0;
  let bg_colors = [rgb(0.9, 0.7, 0.7), rgb(0.7, 0.9, 0.7), rgb(0.7, 0.7, 0.9)];

  try {
    bg_index = get_var("bg_index");
  }
  catch {
  }

  let new_index = if bg_index < bg_colors.len() - 1 {
    bg_index + 1
  } else {
    0
  };

  set_var("bg_index", new_index);

  let cur_color = bg_colors[bg_index];
  set("background_color_light", cur_color);
}
