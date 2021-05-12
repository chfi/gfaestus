# Overlays

In `gfaestus` an "overlay" is a color scheme that gives each node in
the graph a color based on some property, or set of properties, of the
node.

Some examples are coloring each node based on a hash of its sequence,
a hash of the paths on the node, or from an external data source such
as a BED file.

Overlays are added to `gfaestus` at runtime with
[Gluon](https://gluon-lang.org/) scripts.

## Scripting API

The API currently provides two Gluon modules, `gfaestus`, defined in
`src/gluon.rs`, and `bed`, defined in `src/gluon/bed.rs`.

`gfaestus` includes various handlegraph-related types and functions,
`bed` handles BED files.



### Script format

To create an overlay, you must provide a Gluon script that resolves to
the following type:

```gluon
GraphHandle -> IO (Int -> (Float, Float, Float))
```

This is Gluon syntax for a function that takes a `GraphHandle` and
returns an `IO (Int -> (Float, Float, Float))`.

`GraphHandle` (from the `gfaestus` module, so it's usually found as
`gfaestus.GraphHandle` in scripts) is a wrapper over a `PackedGraph`
and path position index from the
[`handlegraph`](https://crates.io/crates/handlegraph) crate.

The `Int -> (Float, Float, Float)` part means a function that takes an
integer and returns a triple of floats -- the integer is the node ID,
and the triple of floats is the RGB color the node will have in the
overlay. Each color channel should be in the range 0.0 to 1.0.

The `IO` part means that the script can perform IO to produce the
function that defines the color for each node -- for example, it can
load and parse a file.

Taken as a whole, `GraphHandle -> IO (Int -> (Float, Float, Float))`
means "a function that takes a graph, maybe loads and parses some
external data, and returns a function that maps node IDs to colors".


### Examples

#### Hash of node sequences

```gluon
let io @ { ? }  = import! std.io
let gfaestus = import! gfaestus

let node_color_io graph : gfaestus.GraphHandle
                       -> IO (Int -> (Float, Float, Float)) =
    let node_color id = gfaestus.hash_node_color
                          (gfaestus.hash_node_seq graph id)
    io.wrap node_color

node_color_io
```

`gfaestus.hash_node_seq` takes a `GraphHandle` and node ID and returns
a hash of the node's sequence. `gfaestus.hash_node_color` takes a hash
and returns a color.

Since this overlay doesn't need to do any IO, we use `io.wrap` to wrap
the pure function `node_color` in the `IO` type.


## Gluon Resources
- [Gluon tutorial](https://gluon-lang.org/doc/crates_io/book/index.html)
- [Gluon API reference](https://gluon-lang.org/doc/crates_io/std/std.html)
