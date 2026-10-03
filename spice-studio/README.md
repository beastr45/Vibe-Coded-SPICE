# Spice Studio

Spice Studio is a Rust-first circuit simulation workspace with four coordinated parts:

- `crates/spice-core`: parser, netlist model, simulator core, and schematic/vector storage helpers.
- `crates/spice-cli`: standalone CLI frontend for batch simulation.
- `apps/spice-studio`: Tauri desktop GUI with an LTspice-inspired workflow.
- `book`: an mdBook that explains how to build a SPICE simulator step by step.

## Scope

This repository provides a working Xyce-inspired architecture and a substantial classical SPICE subset:

- linear passive devices (R, C, L)
- independent voltage/current sources with DC, AC, SIN, and PULSE support
- voltage-controlled current sources (G / VCCS)
- voltage-controlled voltage sources (E / VCVS)
- current-controlled current sources (F / CCCS)
- current-controlled voltage sources (H / CCVS)
- diode models (`.model ... D ...`)
- basic level-1-style NMOS/PMOS parsing and operating-point/small-signal stamping
- hierarchical `.subckt` expansion for reusable circuit blocks
- `.op`, `.dc`, `.ac`, and `.tran`
- SVG-backed schematic export paired with a SPICE netlist
- desktop GUI shell plus standalone CLI

The current GUI phase also adds:

- click-to-place interactive editing on a snapped grid
- two-click wire creation
- editable wire net labels
- automatic net inference from wire geometry and symbol terminal positions
- source-value templates for common stimulus definitions
- inline waveform plotting for transient and AC results
- selectable plotted traces in the waveform pane
- single- and dual-cursor waveform readout for point, delta, slope, average, and 1/Δx measurements on plotted traces
- inspector fields for controlled-source control nodes

Modern Xyce itself supports a much larger device and analysis surface area. This codebase is structured so that additional devices, parser directives, and solver strategies can be layered in systematically.

## Quick start

```bash
source "$HOME/.cargo/env"
cargo test -p spice-core
cargo run -p spice-cli -- examples/rc_lowpass.cir
```

For the GUI:

```bash
cd apps/spice-studio
npm install
npm run tauri:dev
```
