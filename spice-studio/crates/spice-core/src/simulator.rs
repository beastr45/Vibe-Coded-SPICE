use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{anyhow, bail, Context, Result};
use nalgebra::{DMatrix, DVector};
use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::netlist::{AcSweep, Analysis, Circuit, DiodeModel, Element, MosModel, Netlist};

const GMIN: f64 = 1e-12;
const THERMAL_VOLTAGE: f64 = 0.025_852;

/// Solver tolerances and iteration limits shared by every analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub max_iters: usize,
    pub rel_tol: f64,
    pub abs_tol: f64,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            max_iters: 50,
            rel_tol: 1e-8,
            abs_tol: 1e-12,
        }
    }
}

/// Top-level simulation payload returned to the CLI and GUI frontends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub title: String,
    pub analyses: Vec<AnalysisOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalysisOutput {
    OperatingPoint(OperatingPoint),
    DcSweep(DcSweepResult),
    Ac(AcResult),
    Transient(WaveformSet),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatingPoint {
    pub variables: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepPoint {
    pub swept_value: f64,
    pub variables: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcSweepResult {
    pub source: String,
    pub points: Vec<SweepPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcResult {
    pub frequency: Vec<f64>,
    pub magnitude: BTreeMap<String, Vec<f64>>,
    pub phase_deg: BTreeMap<String, Vec<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformSet {
    pub time: Vec<f64>,
    pub traces: BTreeMap<String, Vec<f64>>,
}

#[derive(Debug, Clone)]
struct NodeIndex {
    names: Vec<String>,
    lookup: HashMap<String, usize>,
}

impl NodeIndex {
    fn from_circuit(circuit: &Circuit) -> Self {
        let mut names = BTreeSet::new();
        for element in &circuit.elements {
            match element {
                Element::Resistor { a, b, .. }
                | Element::Capacitor { a, b, .. }
                | Element::Inductor { a, b, .. } => {
                    insert_if_not_ground(&mut names, a);
                    insert_if_not_ground(&mut names, b);
                }
                Element::VoltageSource {
                    positive, negative, ..
                }
                | Element::CurrentSource {
                    positive, negative, ..
                }
                | Element::Cccs {
                    positive, negative, ..
                }
                | Element::Ccvs {
                    positive, negative, ..
                } => {
                    insert_if_not_ground(&mut names, positive);
                    insert_if_not_ground(&mut names, negative);
                }
                Element::Diode {
                    anode, cathode, ..
                } => {
                    insert_if_not_ground(&mut names, anode);
                    insert_if_not_ground(&mut names, cathode);
                }
                Element::Mosfet {
                    drain,
                    gate,
                    source,
                    body,
                    ..
                } => {
                    for node in [drain, gate, source, body] {
                        insert_if_not_ground(&mut names, node);
                    }
                }
                Element::Vccs {
                    positive,
                    negative,
                    control_positive,
                    control_negative,
                    ..
                } => {
                    insert_if_not_ground(&mut names, positive);
                    insert_if_not_ground(&mut names, negative);
                    insert_if_not_ground(&mut names, control_positive);
                    insert_if_not_ground(&mut names, control_negative);
                }
                Element::Vcvs {
                    positive,
                    negative,
                    control_positive,
                    control_negative,
                    ..
                } => {
                    insert_if_not_ground(&mut names, positive);
                    insert_if_not_ground(&mut names, negative);
                    insert_if_not_ground(&mut names, control_positive);
                    insert_if_not_ground(&mut names, control_negative);
                }
            }
        }
        let names = names.into_iter().collect::<Vec<_>>();
        let lookup = names
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.clone(), idx))
            .collect();
        Self { names, lookup }
    }

    fn len(&self) -> usize {
        self.names.len()
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.lookup.get(name).copied()
    }
}

#[derive(Debug, Clone)]
struct ExtraVarIndex {
    names: Vec<String>,
    lookup: HashMap<String, usize>,
    control_branch_lookup: HashMap<String, usize>,
}

impl ExtraVarIndex {
    fn from_names(names: Vec<String>) -> Self {
        let lookup = names
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.clone(), idx))
            .collect::<HashMap<_, _>>();
        let control_branch_lookup = names
            .iter()
            .enumerate()
            .map(|(idx, name)| (normalized_branch_key(name), idx))
            .collect::<HashMap<_, _>>();
        Self {
            names,
            lookup,
            control_branch_lookup,
        }
    }

    fn absolute(&self, nodes: usize, name: &str) -> Option<usize> {
        self.lookup.get(name).map(|idx| nodes + idx)
    }

    fn len(&self) -> usize {
        self.names.len()
    }
}

#[derive(Debug, Clone, Default)]
struct DynamicState {
    capacitor_voltages: HashMap<String, f64>,
    inductor_currents: HashMap<String, f64>,
}

#[derive(Debug, Clone, Copy)]
enum Domain {
    Dc,
    Ac,
    Transient { dt: f64 },
}

/// Run every requested analysis in the netlist and return serializable results.
pub fn simulate(netlist: &Netlist, config: &SimulationConfig) -> Result<SimulationResult> {
    let mut outputs = Vec::new();
    let mut dc_overrides = HashMap::new();
    let mut last_solution: Option<Vec<f64>> = None;

    for analysis in &netlist.analyses {
        let output = match analysis {
            Analysis::OperatingPoint => {
                let (variables, solution_vector) = solve_operating_point(
                    &netlist.circuit,
                    config,
                    &dc_overrides,
                    last_solution.as_deref(),
                )?;
                last_solution = Some(solution_vector);
                AnalysisOutput::OperatingPoint(OperatingPoint { variables })
            }
            Analysis::DcSweep {
                source_name,
                start,
                stop,
                step,
            } => {
                let mut points = Vec::new();
                let mut guess = last_solution.clone();
                let mut value = *start;
                while if *step >= 0.0 {
                    value <= *stop + 1e-18
                } else {
                    value >= *stop - 1e-18
                } {
                    dc_overrides.insert(source_name.clone(), value);
                    let (variables, solution_vector) = solve_operating_point(
                        &netlist.circuit,
                        config,
                        &dc_overrides,
                        guess.as_deref(),
                    )?;
                    guess = Some(solution_vector);
                    points.push(SweepPoint {
                        swept_value: value,
                        variables,
                    });
                    value += *step;
                }
                dc_overrides.remove(source_name);
                AnalysisOutput::DcSweep(DcSweepResult {
                    source: source_name.clone(),
                    points,
                })
            }
            Analysis::Ac {
                sweep,
                points,
                start_hz,
                stop_hz,
            } => AnalysisOutput::Ac(run_ac_analysis(
                &netlist.circuit,
                *sweep,
                *points,
                *start_hz,
                *stop_hz,
                config,
                &dc_overrides,
            )?),
            Analysis::Transient { step, stop, start } => {
                AnalysisOutput::Transient(run_transient_analysis(
                    &netlist.circuit,
                    *step,
                    *stop,
                    *start,
                    config,
                    &dc_overrides,
                )?)
            }
        };
        outputs.push(output);
    }

    Ok(SimulationResult {
        title: netlist.title.clone(),
        analyses: outputs,
    })
}

fn insert_if_not_ground(names: &mut BTreeSet<String>, node: &str) {
    if !is_ground(node) {
        names.insert(node.to_string());
    }
}

fn solve_operating_point(
    circuit: &Circuit,
    config: &SimulationConfig,
    dc_overrides: &HashMap<String, f64>,
    initial_guess: Option<&[f64]>,
) -> Result<(BTreeMap<String, f64>, Vec<f64>)> {
    let nodes = NodeIndex::from_circuit(circuit);
    let extras = build_extra_vars(circuit, Domain::Dc);
    let size = nodes.len() + extras.len();
    let mut guess = initial_guess
        .map(|v| v.to_vec())
        .unwrap_or_else(|| vec![0.0; size]);
    if guess.len() != size {
        guess = vec![0.0; size];
    }

    let state = DynamicState::default();
    let solution = solve_nonlinear_real(
        circuit,
        &nodes,
        &extras,
        Domain::Dc,
        &state,
        0.0,
        dc_overrides,
        config,
        &mut guess,
    )?;
    let variables = collect_real_variables(circuit, &nodes, &extras, &solution);
    Ok((variables, solution))
}

fn run_transient_analysis(
    circuit: &Circuit,
    step: f64,
    stop: f64,
    start: f64,
    config: &SimulationConfig,
    dc_overrides: &HashMap<String, f64>,
) -> Result<WaveformSet> {
    let nodes = NodeIndex::from_circuit(circuit);
    let extras = build_extra_vars(circuit, Domain::Transient { dt: step });
    let size = nodes.len() + extras.len();

    let (_, op_solution) = solve_operating_point(circuit, config, dc_overrides, None)?;
    let mut guess = if op_solution.len() == size {
        op_solution
    } else {
        vec![0.0; size]
    };
    let mut state = update_dynamic_state(circuit, &nodes, &extras, &guess);
    let mut time = Vec::new();
    let mut traces = BTreeMap::<String, Vec<f64>>::new();

    let mut current_time = 0.0;
    loop {
        let solution = solve_nonlinear_real(
            circuit,
            &nodes,
            &extras,
            Domain::Transient { dt: step },
            &state,
            current_time,
            dc_overrides,
            config,
            &mut guess,
        )?;
        guess = solution;
        state = update_dynamic_state(circuit, &nodes, &extras, &guess);
        if current_time >= start - 1e-18 {
            time.push(current_time);
            append_real_traces(&mut traces, &nodes, &extras, &guess);
        }
        if current_time >= stop - 1e-18 {
            break;
        }
        current_time += step;
    }

    Ok(WaveformSet { time, traces })
}

fn run_ac_analysis(
    circuit: &Circuit,
    sweep: AcSweep,
    points: usize,
    start_hz: f64,
    stop_hz: f64,
    config: &SimulationConfig,
    dc_overrides: &HashMap<String, f64>,
) -> Result<AcResult> {
    let (_, op_solution) = solve_operating_point(circuit, config, dc_overrides, None)?;
    let nodes = NodeIndex::from_circuit(circuit);
    let extras = build_extra_vars(circuit, Domain::Ac);
    let size = nodes.len() + extras.len();
    let mut frequency = Vec::new();
    let mut magnitude = BTreeMap::<String, Vec<f64>>::new();
    let mut phase_deg = BTreeMap::<String, Vec<f64>>::new();

    for idx in 0..points.max(1) {
        let freq = match sweep {
            AcSweep::Linear => {
                if points <= 1 {
                    start_hz
                } else {
                    start_hz + (stop_hz - start_hz) * idx as f64 / (points as f64 - 1.0)
                }
            }
            AcSweep::Decade => {
                let log_start = start_hz.log10();
                let log_stop = stop_hz.log10();
                10f64.powf(log_start + (log_stop - log_start) * idx as f64 / (points as f64 - 1.0))
            }
            AcSweep::Octave => {
                let octaves = (stop_hz / start_hz).log2();
                start_hz * 2f64.powf(octaves * idx as f64 / (points as f64 - 1.0))
            }
        };
        let omega = 2.0 * std::f64::consts::PI * freq;
        let (a, z) = build_ac_system(circuit, &nodes, &extras, omega, &op_solution, dc_overrides)?;
        let solution = solve_complex_linear(a, z)?;
        if solution.len() != size {
            bail!("AC solver returned unexpected vector length");
        }
        frequency.push(freq);
        append_ac_traces(&mut magnitude, &mut phase_deg, &nodes, &extras, &solution);
    }

    Ok(AcResult {
        frequency,
        magnitude,
        phase_deg,
    })
}

fn build_extra_vars(circuit: &Circuit, domain: Domain) -> ExtraVarIndex {
    let mut names = Vec::new();
    for element in &circuit.elements {
        match element {
            Element::VoltageSource { name, .. } => names.push(format!("I({name})")),
            Element::Inductor { name, .. } => {
                if matches!(domain, Domain::Dc | Domain::Transient { .. }) {
                    names.push(format!("I({name})"));
                }
            }
            Element::Vcvs { name, .. } => names.push(format!("I({name})")),
            Element::Ccvs { name, .. } => names.push(format!("I({name})")),
            _ => {}
        }
    }
    ExtraVarIndex::from_names(names)
}

fn normalized_branch_key(name: &str) -> String {
    let trimmed = name.trim();
    if let Some(inner) = trimmed.strip_prefix("I(").and_then(|s| s.strip_suffix(')')) {
        inner.to_string()
    } else {
        trimmed.to_string()
    }
}

fn lookup_control_branch_index(extras: &ExtraVarIndex, nodes_len: usize, control_source: &str) -> Option<usize> {
    let target = normalized_branch_key(control_source);
    extras
        .control_branch_lookup
        .get(&target)
        .map(|index| nodes_len + index)
}

#[allow(clippy::too_many_arguments)]
fn solve_nonlinear_real(
    circuit: &Circuit,
    nodes: &NodeIndex,
    extras: &ExtraVarIndex,
    domain: Domain,
    state: &DynamicState,
    time: f64,
    dc_overrides: &HashMap<String, f64>,
    config: &SimulationConfig,
    guess: &mut Vec<f64>,
) -> Result<Vec<f64>> {
    for _ in 0..config.max_iters {
        let (a, z) = build_real_system(circuit, nodes, extras, domain, state, time, dc_overrides, guess)?;
        let next = solve_real_linear(a, z)?;
        let converged = next.iter().zip(guess.iter()).all(|(new, old)| {
            let delta = (new - old).abs();
            delta <= config.abs_tol + config.rel_tol * new.abs().max(old.abs())
        });
        *guess = next;
        if converged {
            return Ok(std::mem::take(guess));
        }
    }
    Err(anyhow!("nonlinear solver failed to converge"))
}

#[allow(clippy::too_many_arguments)]
fn build_real_system(
    circuit: &Circuit,
    nodes: &NodeIndex,
    extras: &ExtraVarIndex,
    domain: Domain,
    state: &DynamicState,
    time: f64,
    dc_overrides: &HashMap<String, f64>,
    guess: &[f64],
) -> Result<(DMatrix<f64>, DVector<f64>)> {
    let size = nodes.len() + extras.len();
    let mut a = DMatrix::<f64>::zeros(size, size);
    let mut z = DVector::<f64>::zeros(size);
    let dt = if let Domain::Transient { dt } = domain {
        Some(dt)
    } else {
        None
    };

    for idx in 0..nodes.len() {
        a[(idx, idx)] += GMIN;
    }

    for element in &circuit.elements {
        match element {
            Element::Resistor { a: n1, b: n2, value, .. } => {
                stamp_conductance(&mut a, nodes, n1, n2, 1.0 / value.max(1e-30));
            }
            Element::Capacitor {
                name,
                a: n1,
                b: n2,
                value,
                ic,
            } => {
                if let Some(dt) = dt {
                    let g = value / dt;
                    let v_prev = state
                        .capacitor_voltages
                        .get(name)
                        .copied()
                        .or(*ic)
                        .unwrap_or(0.0);
                    stamp_conductance(&mut a, nodes, n1, n2, g);
                    stamp_current(&mut z, nodes, n1, n2, -g * v_prev);
                }
            }
            Element::Inductor {
                name,
                a: n1,
                b: n2,
                value,
                ic,
            } => match domain {
                Domain::Ac => unreachable!("inductors are handled in AC builder"),
                Domain::Dc => {
                    let idx = extras
                        .absolute(nodes.len(), &format!("I({name})"))
                        .context("missing inductor current variable")?;
                    stamp_voltage_source_equation(&mut a, nodes, n1, n2, idx);
                }
                Domain::Transient { dt } => {
                    let idx = extras
                        .absolute(nodes.len(), &format!("I({name})"))
                        .context("missing inductor current variable")?;
                    stamp_voltage_source_equation(&mut a, nodes, n1, n2, idx);
                    a[(idx, idx)] += -(value / dt);
                    let i_prev = state
                        .inductor_currents
                        .get(name)
                        .copied()
                        .or(*ic)
                        .unwrap_or(0.0);
                    z[idx] += -(value / dt) * i_prev;
                }
            },
            Element::VoltageSource {
                name,
                positive,
                negative,
                spec,
            } => {
                let idx = extras
                    .absolute(nodes.len(), &format!("I({name})"))
                    .context("missing voltage source current variable")?;
                stamp_voltage_source_equation(&mut a, nodes, positive, negative, idx);
                let value = dc_overrides.get(name).copied().unwrap_or_else(|| match domain {
                    Domain::Transient { .. } => spec.value_at(time),
                    _ => spec.dc_value,
                });
                z[idx] += value;
            }
            Element::CurrentSource {
                positive,
                negative,
                spec,
                name,
            } => {
                let value = dc_overrides.get(name).copied().unwrap_or_else(|| match domain {
                    Domain::Transient { .. } => spec.value_at(time),
                    _ => spec.dc_value,
                });
                stamp_current(&mut z, nodes, positive, negative, value);
            }
            Element::Vccs {
                positive,
                negative,
                control_positive,
                control_negative,
                transconductance,
                ..
            } => {
                stamp_vccs(
                    &mut a,
                    nodes,
                    positive,
                    negative,
                    control_positive,
                    control_negative,
                    *transconductance,
                );
            }
            Element::Vcvs {
                name,
                positive,
                negative,
                control_positive,
                control_negative,
                gain,
            } => {
                let idx = extras
                    .absolute(nodes.len(), &format!("I({name})"))
                    .context("missing VCVS current variable")?;
                stamp_voltage_source_equation(&mut a, nodes, positive, negative, idx);
                if let Some(cp) = nodes.index_of(control_positive) {
                    a[(idx, cp)] -= *gain;
                }
                if let Some(cn) = nodes.index_of(control_negative) {
                    a[(idx, cn)] += *gain;
                }
            }
            Element::Cccs {
                positive,
                negative,
                control_source,
                gain,
                ..
            } => {
                let control_idx = lookup_control_branch_index(extras, nodes.len(), control_source)
                    .with_context(|| format!("missing controlling branch current for '{}'", control_source))?;
                if let Some(op) = nodes.index_of(positive) {
                    a[(op, control_idx)] += *gain;
                }
                if let Some(on) = nodes.index_of(negative) {
                    a[(on, control_idx)] -= *gain;
                }
            }
            Element::Ccvs {
                name,
                positive,
                negative,
                control_source,
                transresistance,
            } => {
                let idx = extras
                    .absolute(nodes.len(), &format!("I({name})"))
                    .context("missing CCVS current variable")?;
                let control_idx = lookup_control_branch_index(extras, nodes.len(), control_source)
                    .with_context(|| format!("missing controlling branch current for '{}'", control_source))?;
                stamp_voltage_source_equation(&mut a, nodes, positive, negative, idx);
                a[(idx, control_idx)] -= *transresistance;
            }
            Element::Diode {
                anode,
                cathode,
                model,
                ..
            } => {
                let model = circuit
                    .models
                    .get(model)
                    .or_else(|| circuit.models.get("DEFAULT_DIODE"))
                    .ok_or_else(|| anyhow!("missing diode model '{}'", model))?;
                let vd = node_voltage(nodes, anode, guess) - node_voltage(nodes, cathode, guess);
                let (g, i_eq) = diode_linearization(model, vd);
                stamp_conductance(&mut a, nodes, anode, cathode, g);
                stamp_current(&mut z, nodes, anode, cathode, i_eq);
            }
            Element::Mosfet {
                drain,
                gate,
                source,
                body,
                model,
                ..
            } => {
                let model = circuit
                    .mos_models
                    .get(model)
                    .or_else(|| circuit.mos_models.get("DEFAULT_NMOS"))
                    .ok_or_else(|| anyhow!("missing MOS model '{}'", model))?;
                let stamp = mosfet_linearization(model, nodes, drain, gate, source, body, guess);
                stamp_conductance(&mut a, nodes, drain, source, stamp.gds);
                stamp_vccs(
                    &mut a,
                    nodes,
                    drain,
                    source,
                    gate,
                    source,
                    stamp.gm,
                );
                stamp_vccs(
                    &mut a,
                    nodes,
                    drain,
                    source,
                    body,
                    source,
                    stamp.gmb,
                );
                stamp_current(&mut z, nodes, drain, source, stamp.i_eq);
            }
        }
    }

    Ok((a, z))
}

fn build_ac_system(
    circuit: &Circuit,
    nodes: &NodeIndex,
    extras: &ExtraVarIndex,
    omega: f64,
    op_solution: &[f64],
    dc_overrides: &HashMap<String, f64>,
) -> Result<(DMatrix<Complex64>, DVector<Complex64>)> {
    let size = nodes.len() + extras.len();
    let mut a = DMatrix::<Complex64>::zeros(size, size);
    let mut z = DVector::<Complex64>::zeros(size);
    for idx in 0..nodes.len() {
        a[(idx, idx)] += Complex64::new(GMIN, 0.0);
    }

    for element in &circuit.elements {
        match element {
            Element::Resistor { a: n1, b: n2, value, .. } => {
                stamp_complex_conductance(&mut a, nodes, n1, n2, Complex64::new(1.0 / value, 0.0));
            }
            Element::Capacitor { a: n1, b: n2, value, .. } => {
                stamp_complex_conductance(&mut a, nodes, n1, n2, Complex64::new(0.0, omega * value));
            }
            Element::Inductor { a: n1, b: n2, value, .. } => {
                let y = Complex64::new(0.0, -1.0 / (omega * value).max(1e-30));
                stamp_complex_conductance(&mut a, nodes, n1, n2, y);
            }
            Element::VoltageSource {
                name,
                positive,
                negative,
                spec,
            } => {
                let idx = extras
                    .absolute(nodes.len(), &format!("I({name})"))
                    .context("missing AC voltage source current variable")?;
                stamp_complex_voltage_source_equation(&mut a, nodes, positive, negative, idx);
                let mag = dc_overrides.get(name).copied().unwrap_or(spec.ac_magnitude);
                let phase = spec.ac_phase_deg.to_radians();
                z[idx] += Complex64::from_polar(mag, phase);
            }
            Element::CurrentSource {
                positive,
                negative,
                spec,
                name,
            } => {
                let mag = dc_overrides.get(name).copied().unwrap_or(spec.ac_magnitude);
                let phase = spec.ac_phase_deg.to_radians();
                stamp_complex_current(
                    &mut z,
                    nodes,
                    positive,
                    negative,
                    Complex64::from_polar(mag, phase),
                );
            }
            Element::Vccs {
                positive,
                negative,
                control_positive,
                control_negative,
                transconductance,
                ..
            } => {
                stamp_complex_vccs(
                    &mut a,
                    nodes,
                    positive,
                    negative,
                    control_positive,
                    control_negative,
                    Complex64::new(*transconductance, 0.0),
                );
            }
            Element::Vcvs {
                name,
                positive,
                negative,
                control_positive,
                control_negative,
                gain,
            } => {
                let idx = extras
                    .absolute(nodes.len(), &format!("I({name})"))
                    .context("missing AC VCVS current variable")?;
                stamp_complex_voltage_source_equation(&mut a, nodes, positive, negative, idx);
                if let Some(cp) = nodes.index_of(control_positive) {
                    a[(idx, cp)] -= Complex64::new(*gain, 0.0);
                }
                if let Some(cn) = nodes.index_of(control_negative) {
                    a[(idx, cn)] += Complex64::new(*gain, 0.0);
                }
            }
            Element::Cccs {
                positive,
                negative,
                control_source,
                gain,
                ..
            } => {
                let control_idx = lookup_control_branch_index(extras, nodes.len(), control_source)
                    .with_context(|| format!("missing AC controlling branch current for '{}'", control_source))?;
                if let Some(op) = nodes.index_of(positive) {
                    a[(op, control_idx)] += Complex64::new(*gain, 0.0);
                }
                if let Some(on) = nodes.index_of(negative) {
                    a[(on, control_idx)] -= Complex64::new(*gain, 0.0);
                }
            }
            Element::Ccvs {
                name,
                positive,
                negative,
                control_source,
                transresistance,
            } => {
                let idx = extras
                    .absolute(nodes.len(), &format!("I({name})"))
                    .context("missing AC CCVS current variable")?;
                let control_idx = lookup_control_branch_index(extras, nodes.len(), control_source)
                    .with_context(|| format!("missing AC controlling branch current for '{}'", control_source))?;
                stamp_complex_voltage_source_equation(&mut a, nodes, positive, negative, idx);
                a[(idx, control_idx)] -= Complex64::new(*transresistance, 0.0);
            }
            Element::Diode {
                anode,
                cathode,
                model,
                ..
            } => {
                let model = circuit
                    .models
                    .get(model)
                    .or_else(|| circuit.models.get("DEFAULT_DIODE"))
                    .ok_or_else(|| anyhow!("missing diode model '{}'", model))?;
                let vd = node_voltage(nodes, anode, op_solution) - node_voltage(nodes, cathode, op_solution);
                let (g, _) = diode_linearization(model, vd);
                stamp_complex_conductance(&mut a, nodes, anode, cathode, Complex64::new(g, 0.0));
            }
            Element::Mosfet {
                drain,
                gate,
                source,
                body,
                model,
                ..
            } => {
                let model = circuit
                    .mos_models
                    .get(model)
                    .or_else(|| circuit.mos_models.get("DEFAULT_NMOS"))
                    .ok_or_else(|| anyhow!("missing MOS model '{}'", model))?;
                let stamp = mosfet_linearization(model, nodes, drain, gate, source, body, op_solution);
                stamp_complex_conductance(&mut a, nodes, drain, source, Complex64::new(stamp.gds, 0.0));
                stamp_complex_vccs(
                    &mut a,
                    nodes,
                    drain,
                    source,
                    gate,
                    source,
                    Complex64::new(stamp.gm, 0.0),
                );
                stamp_complex_vccs(
                    &mut a,
                    nodes,
                    drain,
                    source,
                    body,
                    source,
                    Complex64::new(stamp.gmb, 0.0),
                );
            }
        }
    }

    Ok((a, z))
}

fn solve_real_linear(a: DMatrix<f64>, z: DVector<f64>) -> Result<Vec<f64>> {
    let lu = a.lu();
    let solution = lu.solve(&z).ok_or_else(|| anyhow!("singular matrix"))?;
    Ok(solution.as_slice().to_vec())
}

fn solve_complex_linear(a: DMatrix<Complex64>, z: DVector<Complex64>) -> Result<Vec<Complex64>> {
    let lu = a.lu();
    let solution = lu
        .solve(&z)
        .ok_or_else(|| anyhow!("singular complex matrix"))?;
    Ok(solution.as_slice().to_vec())
}

fn stamp_conductance(a: &mut DMatrix<f64>, nodes: &NodeIndex, n1: &str, n2: &str, g: f64) {
    let i = nodes.index_of(n1);
    let j = nodes.index_of(n2);
    if let Some(i) = i {
        a[(i, i)] += g;
    }
    if let Some(j) = j {
        a[(j, j)] += g;
    }
    if let (Some(i), Some(j)) = (i, j) {
        a[(i, j)] -= g;
        a[(j, i)] -= g;
    }
}

fn stamp_current(z: &mut DVector<f64>, nodes: &NodeIndex, positive: &str, negative: &str, value: f64) {
    if let Some(i) = nodes.index_of(positive) {
        z[i] -= value;
    }
    if let Some(j) = nodes.index_of(negative) {
        z[j] += value;
    }
}

fn stamp_voltage_source_equation(
    a: &mut DMatrix<f64>,
    nodes: &NodeIndex,
    positive: &str,
    negative: &str,
    branch_idx: usize,
) {
    if let Some(i) = nodes.index_of(positive) {
        a[(i, branch_idx)] += 1.0;
        a[(branch_idx, i)] += 1.0;
    }
    if let Some(j) = nodes.index_of(negative) {
        a[(j, branch_idx)] -= 1.0;
        a[(branch_idx, j)] -= 1.0;
    }
}

fn stamp_vccs(
    a: &mut DMatrix<f64>,
    nodes: &NodeIndex,
    out_p: &str,
    out_n: &str,
    ctrl_p: &str,
    ctrl_n: &str,
    gm: f64,
) {
    let op = nodes.index_of(out_p);
    let on = nodes.index_of(out_n);
    let cp = nodes.index_of(ctrl_p);
    let cn = nodes.index_of(ctrl_n);

    if let (Some(op), Some(cp)) = (op, cp) {
        a[(op, cp)] += gm;
    }
    if let (Some(op), Some(cn)) = (op, cn) {
        a[(op, cn)] -= gm;
    }
    if let (Some(on), Some(cp)) = (on, cp) {
        a[(on, cp)] -= gm;
    }
    if let (Some(on), Some(cn)) = (on, cn) {
        a[(on, cn)] += gm;
    }
}

fn stamp_complex_conductance(
    a: &mut DMatrix<Complex64>,
    nodes: &NodeIndex,
    n1: &str,
    n2: &str,
    g: Complex64,
) {
    let i = nodes.index_of(n1);
    let j = nodes.index_of(n2);
    if let Some(i) = i {
        a[(i, i)] += g;
    }
    if let Some(j) = j {
        a[(j, j)] += g;
    }
    if let (Some(i), Some(j)) = (i, j) {
        a[(i, j)] -= g;
        a[(j, i)] -= g;
    }
}

fn stamp_complex_current(
    z: &mut DVector<Complex64>,
    nodes: &NodeIndex,
    positive: &str,
    negative: &str,
    value: Complex64,
) {
    if let Some(i) = nodes.index_of(positive) {
        z[i] -= value;
    }
    if let Some(j) = nodes.index_of(negative) {
        z[j] += value;
    }
}

fn stamp_complex_voltage_source_equation(
    a: &mut DMatrix<Complex64>,
    nodes: &NodeIndex,
    positive: &str,
    negative: &str,
    branch_idx: usize,
) {
    if let Some(i) = nodes.index_of(positive) {
        a[(i, branch_idx)] += Complex64::new(1.0, 0.0);
        a[(branch_idx, i)] += Complex64::new(1.0, 0.0);
    }
    if let Some(j) = nodes.index_of(negative) {
        a[(j, branch_idx)] -= Complex64::new(1.0, 0.0);
        a[(branch_idx, j)] -= Complex64::new(1.0, 0.0);
    }
}

fn stamp_complex_vccs(
    a: &mut DMatrix<Complex64>,
    nodes: &NodeIndex,
    out_p: &str,
    out_n: &str,
    ctrl_p: &str,
    ctrl_n: &str,
    gm: Complex64,
) {
    let op = nodes.index_of(out_p);
    let on = nodes.index_of(out_n);
    let cp = nodes.index_of(ctrl_p);
    let cn = nodes.index_of(ctrl_n);

    if let (Some(op), Some(cp)) = (op, cp) {
        a[(op, cp)] += gm;
    }
    if let (Some(op), Some(cn)) = (op, cn) {
        a[(op, cn)] -= gm;
    }
    if let (Some(on), Some(cp)) = (on, cp) {
        a[(on, cp)] -= gm;
    }
    if let (Some(on), Some(cn)) = (on, cn) {
        a[(on, cn)] += gm;
    }
}

fn diode_linearization(model: &DiodeModel, vd: f64) -> (f64, f64) {
    let nvt = model.emission_coefficient * THERMAL_VOLTAGE;
    let exponent = (vd / nvt).clamp(-40.0, 40.0);
    let exp_v = exponent.exp();
    let current = model.saturation_current * (exp_v - 1.0);
    let conductance = model.saturation_current * exp_v / nvt + GMIN;
    let i_eq = current - conductance * vd;
    (conductance, i_eq)
}

struct MosfetStamp {
    gm: f64,
    gds: f64,
    gmb: f64,
    i_eq: f64,
}

fn mosfet_linearization(
    model: &MosModel,
    nodes: &NodeIndex,
    drain: &str,
    gate: &str,
    source: &str,
    body: &str,
    solution: &[f64],
) -> MosfetStamp {
    let sign = if model.pmos { -1.0 } else { 1.0 };
    let vd = sign * node_voltage(nodes, drain, solution);
    let vg = sign * node_voltage(nodes, gate, solution);
    let vs = sign * node_voltage(nodes, source, solution);
    let vb = sign * node_voltage(nodes, body, solution);
    let vgs = vg - vs;
    let vds = vd - vs;
    let vbs = vb - vs;
    let vth = model.threshold + 0.1 * vbs;

    if vgs <= vth {
        return MosfetStamp {
            gm: 0.0,
            gds: GMIN,
            gmb: 0.0,
            i_eq: 0.0,
        };
    }

    let overdrive = (vgs - vth).max(0.0);
    let beta = model.transconductance.max(1e-12);
    let lambda = model.lambda.max(0.0);

    let (id, gm, gds) = if vds < overdrive {
        let id = beta * ((overdrive * vds) - 0.5 * vds * vds) * (1.0 + lambda * vds);
        let gm = beta * vds * (1.0 + lambda * vds);
        let gds = beta * ((overdrive - vds) * (1.0 + lambda * vds)
            + ((overdrive * vds) - 0.5 * vds * vds) * lambda)
            + GMIN;
        (id, gm, gds)
    } else {
        let id = 0.5 * beta * overdrive * overdrive * (1.0 + lambda * vds);
        let gm = beta * overdrive * (1.0 + lambda * vds);
        let gds = 0.5 * beta * overdrive * overdrive * lambda + GMIN;
        (id, gm, gds)
    };
    let gmb = -0.1 * gm;
    let i_eq = sign * id - gds * vds - gm * vgs - gmb * vbs;

    MosfetStamp {
        gm,
        gds,
        gmb,
        i_eq,
    }
}

fn node_voltage(nodes: &NodeIndex, name: &str, solution: &[f64]) -> f64 {
    nodes
        .index_of(name)
        .and_then(|idx| solution.get(idx).copied())
        .unwrap_or(0.0)
}

fn collect_real_variables(
    _circuit: &Circuit,
    nodes: &NodeIndex,
    extras: &ExtraVarIndex,
    solution: &[f64],
) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for name in &nodes.names {
        let idx = nodes.index_of(name).unwrap_or_default();
        out.insert(format!("V({name})"), solution.get(idx).copied().unwrap_or_default());
    }
    for name in &extras.names {
        let idx = extras.absolute(nodes.len(), name).unwrap_or_default();
        out.insert(name.to_owned(), solution.get(idx).copied().unwrap_or_default());
    }
    out
}

fn append_real_traces(
    traces: &mut BTreeMap<String, Vec<f64>>,
    nodes: &NodeIndex,
    extras: &ExtraVarIndex,
    solution: &[f64],
) {
    for name in &nodes.names {
        let idx = nodes.index_of(name).unwrap_or_default();
        let value = solution.get(idx).copied().unwrap_or_default();
        traces
            .entry(format!("V({name})"))
            .or_default()
            .push(value);
    }
    for name in &extras.names {
        let idx = extras.absolute(nodes.len(), name).unwrap_or_default();
        let value = solution.get(idx).copied().unwrap_or_default();
        traces.entry(name.to_owned()).or_default().push(value);
    }
}

fn append_ac_traces(
    magnitude: &mut BTreeMap<String, Vec<f64>>,
    phase_deg: &mut BTreeMap<String, Vec<f64>>,
    nodes: &NodeIndex,
    extras: &ExtraVarIndex,
    solution: &[Complex64],
) {
    for name in &nodes.names {
        let idx = nodes.index_of(name).unwrap_or_default();
        let value = solution.get(idx).copied().unwrap_or_default();
        let key = format!("V({name})");
        magnitude.entry(key.clone()).or_default().push(value.norm());
        phase_deg
            .entry(key)
            .or_default()
            .push(value.arg().to_degrees());
    }
    for name in &extras.names {
        let idx = extras.absolute(nodes.len(), name).unwrap_or_default();
        let value = solution.get(idx).copied().unwrap_or_default();
        magnitude.entry(name.to_owned()).or_default().push(value.norm());
        phase_deg
            .entry(name.to_owned())
            .or_default()
            .push(value.arg().to_degrees());
    }
}

fn update_dynamic_state(
    circuit: &Circuit,
    nodes: &NodeIndex,
    extras: &ExtraVarIndex,
    solution: &[f64],
) -> DynamicState {
    let mut state = DynamicState::default();
    for element in &circuit.elements {
        match element {
            Element::Capacitor { name, a, b, .. } => {
                let v = node_voltage(nodes, a, solution) - node_voltage(nodes, b, solution);
                state.capacitor_voltages.insert(name.clone(), v);
            }
            Element::Inductor { name, .. } => {
                let key = format!("I({name})");
                let current = extras
                    .absolute(nodes.len(), &key)
                    .and_then(|idx| solution.get(idx).copied())
                    .unwrap_or(0.0);
                state.inductor_currents.insert(name.clone(), current);
            }
            _ => {}
        }
    }
    state
}

fn is_ground(name: &str) -> bool {
    matches!(name, "0" | "gnd" | "GND")
}
