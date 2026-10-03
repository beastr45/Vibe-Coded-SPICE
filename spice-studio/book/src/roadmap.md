# Extending Toward Wider Xyce Coverage

Modern Xyce supports far more than the subset implemented here: controlled sources, subcircuits, parameter expressions, behavioral sources, BJT and MOS compact models, noise analysis, advanced timestep control, parallel linear algebra backends, and much more. Reaching that breadth is a serious multi-phase engineering program, not a small refactor.

The good news is that the current architecture points in the right direction. The parser already separates commands and elements. The simulator already separates domain-specific system construction from solving. The GUI already stores a vector schematic separately from the solver input. To expand the simulator, add new element variants, write their DC/AC/transient stamps, extend the parser, and create targeted regression tests.

Some particularly natural next extensions from the current codebase are deeper MOSFET and BJT compact-model coverage beyond the current introductory MOS support, expression evaluators for parameters, richer schematic-net inference for junctions and symbol variants, more advanced waveform cursors and measurement tools, and adaptive timestep control. Each of those features would deepen realism without requiring an architectural reset.

When you do that work, keep one rule in mind: every new device should come with both theory documentation and executable tests. That habit is what turns a toy simulator into trustworthy engineering software.
