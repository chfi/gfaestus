# gfaestus - Vulkan-accelerated GFA visualization

### Demo: https://youtu.be/TOJZeeCqatk

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
[`ash`](https://crates.io/crates/ash) crate.



## Requirements

Compiling `gfaestus` requires the Vulkan SDK, available here: https://vulkan.lunarg.com/sdk/home

To run `gfaestus`, you must have a GPU with drivers that support

Vulkan. If you're on Windows or Linux, and have an AMD, Nvidia, or
integrated Intel GPU, you should be good to go.

If you're on Mac, you'll need to install [MoltenVK](https://github.com/KhronosGroup/MoltenVK).
