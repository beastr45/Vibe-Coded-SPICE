# Introduction

This book accompanies the `spice-studio` codebase and shows how to build a SPICE simulator in Rust from first principles. The implementation is intentionally structured as an educational codebase rather than a black box. The parser, matrix assembly, nonlinear solution loop, transient integration, CLI, and desktop GUI are all separated so you can study them independently and recombine them into your own simulator.

Our target style is modern SPICE with Xyce-inspired syntax and workflow. Full Xyce parity spans a very large engineering surface area, including many semiconductor compact models, PDE-driven devices, parallel solvers, and advanced analysis directives. The code in this repository focuses on the essential simulator core: independent sources, passive elements, a nonlinear diode, `.op`, `.dc`, `.ac`, `.tran`, a standalone CLI, and a Tauri desktop application whose workflow mirrors LTspice’s left-palette, central schematic, right-side property editing, and bottom waveform pane.

The best way to read this book is side by side with the source tree. Keep `crates/spice-core/src/netlist.rs`, `crates/spice-core/src/simulator.rs`, `crates/spice-cli/src/main.rs`, and `apps/spice-studio/src/App.tsx` open while you work through the chapters.

<div class="callout">
  <strong>Interactive checkpoint.</strong>
  Before continuing, predict the operating-point voltage of a 10 V source feeding two 10 kΩ resistors in series. Then compare your mental answer with the solver’s output once you reach the CLI chapter.
</div>
