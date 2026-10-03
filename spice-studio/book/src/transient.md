# Transient Integration

Transient analysis asks how the circuit evolves over time. Capacitors and inductors introduce state, so a single DC solve is no longer enough. We replace differential equations with algebraic companion models at each time step. In this repository, transient simulation uses backward Euler because it is simple, robust, and easy to explain.

For a capacitor, \(i = C dv/dt\). Backward Euler approximates the derivative at the new step as

\[
i_k = C\frac{v_k - v_{k-1}}{\Delta t}
= \frac{C}{\Delta t}v_k - \frac{C}{\Delta t}v_{k-1}.
\]

That expression looks exactly like a conductance plus a history current source. The code in `crates/spice-core/src/simulator.rs` stamps that conductance and injects the history term into the right-hand side. The inductor uses a dual form in terms of its branch current variable.

This is one of the main reasons SPICE solvers use MNA: dynamic elements become systematic stamp updates rather than special-case hacks. Once you understand the companion model idea, extending to trapezoidal integration or Gear methods becomes a manageable refactor instead of a conceptual leap.
