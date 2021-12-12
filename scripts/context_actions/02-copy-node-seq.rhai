export const name = "Copy node sequence";
export const context_types = ["NodeId"];

fn action(context) {
  let node = context.get("NodeId");
  let seq = app::graph.sequence(handle(node, false));
  app::set_clipboard_contents(seq.to_string());
}
