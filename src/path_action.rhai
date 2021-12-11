export const name = "Copy path ID";
export const context_types = ["PathId"];

fn action(context) {
  let path = context.get("PathId");
  set_clipboard_contents(path.to_string());
}
