# AC Small-Signal Analysis

AC analysis is not a time-domain sinusoidal simulation. Instead, it is a linearized frequency-domain solve around the operating point. Capacitors become admittances \(j\omega C\), inductors become \(1/(j\omega L)\), and nonlinear devices contribute the small-signal derivatives implied by their operating-point linearization.

The AC path in `crates/spice-core/src/simulator.rs` therefore runs in two stages. First, it computes the operating point. Second, it builds a complex-valued linear system for each requested frequency. The result is a phasor for each node voltage and branch current. The output reported by the engine stores both magnitude and phase in degrees.

This two-stage structure is exactly why `.ac` depends so heavily on a correct `.op` solution. If the operating point is wrong, every small-signal derivative is wrong too. In other words, AC analysis is only as trustworthy as the bias point underneath it.
