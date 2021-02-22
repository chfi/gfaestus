# gfaestus - Vulkan-accelerated GFA visualization


`gfaestus` is a tool for visualizing and interacting with genome graphs
in the [GFA format](http://gfa-spec.github.io/GFA-spec/GFA1.html).

It can display GFA graphs using a provided 2D layout (produced with
[odgi's](https://github.com/vgteam/odgi) `layout` command), and is
intended to deliver an interactive visual interface for exploring
genome graphs that is fast, powerful, and easy to use.


In addition to the 2D layout, a
[handlegraph](https://github.com/chfi/rs-handlegraph) representation
of the GFA is created, which will enable visualizations and
interactivity that take advantage of the graph topology, paths, and
more.


`gfaestus` uses Vulkan for hardware-acceleration, via the
[`vulkano`](https://crates.io/crates/vulkano) crate.
