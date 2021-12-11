export const name = "Copy node ID - rhai!!";
export const context_types = ["NodeId"];

print("in action.rhai!");

fn lol() {
  5
}


fn action(context) {
  let node = context.get("NodeId");
  set_clipboard_contents(node.to_string());
}
