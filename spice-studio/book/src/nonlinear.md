# Nonlinear Devices and Newton Iteration

Linear circuits are only the beginning. The moment we add a diode, the matrix coefficients depend on the solution itself. The diode law

\[
I_D = I_S\left(e^{V_D/(nV_T)} - 1\right)
\]

is nonlinear, so we cannot solve the circuit in one matrix factorization. Instead we linearize around a current guess, solve the linearized system, update the guess, and repeat. That iterative procedure is Newton’s method.

The helper `diode_linearization` in `crates/spice-core/src/simulator.rs` computes a tangent conductance and an equivalent current source. If the guessed diode voltage is \(V_D^{(k)}\), then the current is approximated as

\[
I_D(V) \approx I_D(V_D^{(k)}) + G_D\left(V - V_D^{(k)}\right)
\]

where \(G_D = dI_D/dV\). In matrix terms, that means we stamp a conductance in parallel with a current source. The nonlinear solver `solve_nonlinear_real` repeats this process until successive iterates are within absolute and relative tolerance.

This chapter is where simulator design starts to feel like numerical analysis rather than just circuit theory. You are not only representing the circuit; you are also choosing a robust strategy for solving it. In production SPICE engines, this area grows to include damping, source stepping, gmin stepping, homotopy strategies, and specialized model limiting. The code here keeps the structure simple so you can understand the essential loop first.

The same pattern can be extended beyond diodes. In the current codebase, a first MOSFET step has been added using a very small Shichman-Hodges-like large-signal approximation for NMOS and PMOS devices. The parser accepts lines of the form `M1 d g s b MODEL` together with `.model ... NMOS ...` or `.model ... PMOS ...` cards carrying parameters such as `VTO`, `KP`, and `LAMBDA`. During DC and transient solves, the simulator linearizes the MOSFET around the current bias point into an output conductance, a gate-controlled transconductance, a body-effect term, and an equivalent current source. That is still far from production compact-model fidelity, but it shows how the diode-style Newton framework naturally generalizes to larger nonlinear devices.
