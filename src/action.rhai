export const name = "Copy node ID - rhai!!";
export const context_types = ["NodeId"];

fn action(context) {
  let node = context.get("NodeId");
  print("in action.rhai!!!!!!!!!!");
  print(node.to_string());
  set_clipboard_contents(node.to_string());
  print("end of action");
  // clipboard.set_text(node.to_string());
}
