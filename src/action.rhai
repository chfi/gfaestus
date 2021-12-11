export const name = "Copy node ID - rhai!!";
export const context_types = ["NodeId"];

fn action(context) {
  let node = context.get("NodeId");
  set_clipboard_contents(node.to_string());
}
