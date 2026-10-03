# Modified Nodal Analysis

Classical nodal analysis writes Kirchhoff’s current law at each node and solves for node voltages. Pure nodal analysis becomes awkward when ideal voltage sources appear because a voltage source constrains a voltage difference directly but does not give a current-voltage relation in admittance form. Modified nodal analysis, or MNA, solves this by augmenting the unknown vector with branch currents for elements such as ideal voltage sources. The resulting linear system is still sparse and structured, but it now handles a much wider set of devices naturally.

The implementation in `crates/spice-core/src/simulator.rs` first builds a `NodeIndex` for all non-ground nodes and an `ExtraVarIndex` for branch currents. A resistor between nodes `a` and `b` stamps a conductance `g = 1/R`. A current source injects current into the right-hand side vector. A voltage source adds one extra equation and one extra unknown current. These are the canonical MNA stamps you will find in SPICE literature, and seeing them directly in code is one of the best ways to make the method feel concrete.

Controlled sources fit naturally into the same framework. A VCCS contributes transconductance terms directly into the nodal matrix, while a VCVS behaves like a voltage source plus a control-dependent constraint row. This is a good example of why MNA remains the standard formulation in SPICE-family tools: once the bookkeeping is set up correctly, adding device types is mostly a matter of writing the right local stamp.

Current-controlled sources add one more twist: they depend on branch current, not node voltage. In practice that means the controlling element has to expose a current unknown that the dependent source can reference. In the present codebase, this is handled by looking up the extra MNA variable associated with a named voltage-source-like branch. A CCCS injects a multiple of that branch current into the destination nodes, and a CCVS adds a voltage-source constraint whose right-hand side depends on the controlling branch current.

For a resistor, the local two-node contribution is

\[
\begin{bmatrix}
g & -g \\
-g & g
\end{bmatrix}
\]

and the code representation is equally compact. The helper `stamp_conductance` adds diagonal terms to each connected node and off-diagonal coupling terms when both nodes are non-ground.

<div class="interactive-box" data-mna-playground>
  <h3>MNA mini-playground</h3>
  <p>Set two resistors in a divider and compute the expected midpoint voltage.</p>
  <label>R1 (Ω) <input id="r1" value="10000" /></label>
  <label>R2 (Ω) <input id="r2" value="10000" /></label>
  <label>Vin (V) <input id="vin" value="10" /></label>
  <button id="solve-divider">Solve divider</button>
  <p id="divider-output"></p>
</div>
