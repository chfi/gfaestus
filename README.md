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


## Usage

With a working Vulkan SDK environment (make sure you have `glslc` on
your path), you can build `gfaestus` using `cargo`.


```sh
cargo build --release
```

Due to technical reasons, `gfaestus` must be run from the repo
directory for shaders and scripts to be found. An easy way to
do this is to use the generated shell script, which can be
copied to your PATH:

```sh
./gfaestus-release <GFA> <layout TSV>

# use whichever directory on your path you like
cp gfaestus-release ~/.local/bin/gfaestus
gfaestus <GFA> <layout TSV>
```


### Instructions

The easiest way to display annotations is via the `BED Label Wizard`
under `Tools` in the top menubar.

You can also load a custom TSV format, which makes it easy to assign
colors and labels to nodes.

Each line assigns either a color, in CSS-style hexadecimal format, or
a text string, to a node. For example, for node 1223:

```
1223 #44BBCC
1223 this line is a label
```

#### Keyboard

* `Space`: Reset view
* `Arrow keys`: Pan view
* `Escape`: Clear selection
* `F9` - Toggle light/dark mode

#### Mouse

* `Left Mouse`: Select node
* `Left Mouse (click and drag)`: Pan view
* `Shift + Left Mouse (click and drag)`: Rectangle select
* `Scroll wheel`: Zoom view

* `Right Mouse`: Context menu
