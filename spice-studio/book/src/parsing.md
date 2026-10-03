# Parsing Xyce-style Netlists

Netlist parsing is where circuit simulation becomes a language problem. SPICE syntax looks simple because each device starts with a letter, but that simplicity hides several tricky details: engineering suffixes such as `10k` and `22u`, analysis commands like `.ac dec 20 10 100k`, source waveforms such as `PULSE(...)` and `SIN(...)`, optional initial conditions, and model declarations like `.model DFAST D IS=1e-12 N=1.2`.

In this project, `parse_netlist` in `crates/spice-core/src/netlist.rs` reads the title, strips comments, tokenizes while respecting parentheses, and turns each device into a typed enum variant. A resistor becomes `Element::Resistor`, a voltage source becomes `Element::VoltageSource`, and so on. This typed representation means later code does not have to keep re-parsing strings when stamping the matrix.

The next major parser step is hierarchy. Real SPICE workflows rely heavily on `.subckt` blocks so users can package a circuit motif once and instantiate it many times. The current implementation preprocesses the source text, records subcircuit definitions, and expands `X...` instances into flattened element lines with instance-qualified internal names. That keeps the numeric layer simple while still enabling reusable circuit structure.

One useful teaching example is engineering notation. The helper `parse_number` in `crates/spice-core/src/netlist.rs` resolves suffixes such as `meg`, `k`, `m`, `u`, `n`, and `p` into floating-point scale factors. The logic is tiny, but it is critical: if your parser mishandles `m` as mega instead of milli, every result after that point is wrong.

Try reading the source and then answer this question: why does the tokenizer avoid splitting on spaces while it is inside parentheses? The answer is that `SIN(0 1 1k)` must remain one semantic token, otherwise the waveform parser would have to reconstruct it from fragments.
