export const name = "Copy path name";
export const context_types = ["PathId"];

fn action(context) {
  let path = context.get("PathId");
  let name = app::graph.get_path_name(path);
  app::set_clipboard_contents(name);
}
