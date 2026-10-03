# SVG + SPICE Project Storage

Many schematic editors use opaque binary files. This project goes the other direction. The circuit drawing is exported as SVG, which is both human-readable and a genuine vector format. The simulator source is exported as a SPICE netlist. A small JSON manifest points to both. This satisfies a practical engineering goal and a teaching goal at the same time: the files are easy to inspect, diff, version, and transform.

The related code paths are `crates/spice-core/src/schematic.rs`, which defines a small in-memory schematic model and SVG emitter, and `apps/spice-studio/src-tauri/src/main.rs`, which writes the exported bundle. Even if you later move to a richer internal schematic model, keeping SVG as an interchange or archival format is a strong idea because it is open and durable.
