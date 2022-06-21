export const name = "Pan to node";
export const context_types = [];

fn action(context) {
  let node = modal::get_node_id();
  let msg = msg::goto_node(node);
  app::send_msg(msg);
}
