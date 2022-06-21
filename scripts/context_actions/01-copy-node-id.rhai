export const name = "Copy node ID";
export const context_types = ["NodeId"];

fn action(context) {
  let node = context.get("NodeId");
  app::set_clipboard_contents(node.to_string());
}