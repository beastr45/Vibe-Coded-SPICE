# CLI and Automation

The standalone command-line tool is the cleanest way to prove that the simulator backend is independent of the GUI. `crates/spice-cli/src/main.rs` reads a netlist from disk, parses it, simulates it, and emits JSON or CSV. That design makes the backend suitable for scripting, CI regression tests, and future integration into larger automation flows.

The CLI exists for an architectural reason as much as a user-facing one. A simulator backend should be usable headlessly. If the only way to access your solver is through a GUI, you make automated testing and reproducibility unnecessarily difficult. With the CLI in place, the Tauri app becomes just one client of the same solver.

Example usage:

```bash
cargo run -p spice-cli -- examples/rc_lowpass.cir
cargo run -p spice-cli -- examples/rc_lowpass.cir --format csv --output rc.csv
```
