# Overlays

In `gfaestus` an "overlay" is a color scheme that gives each node in
the graph a color based on some property, or set of properties, of the
node.

Some examples are coloring each node based on a hash of its sequence,
a hash of the paths on the node, or from an external data source such
as a BED file.

Overlays are added to `gfaestus` at runtime with
[Rhai](https://rhai.rs/) scripts.
