use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Netlist {
    pub title: String,
    pub circuit: Circuit,
    pub analyses: Vec<Analysis>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Circuit {
    pub elements: Vec<Element>,
    pub models: BTreeMap<String, DiodeModel>,
    pub mos_models: BTreeMap<String, MosModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Element {
    Resistor {
        name: String,
        a: String,
        b: String,
        value: f64,
    },
    Capacitor {
        name: String,
        a: String,
        b: String,
        value: f64,
        ic: Option<f64>,
    },
    Inductor {
        name: String,
        a: String,
        b: String,
        value: f64,
        ic: Option<f64>,
    },
    VoltageSource {
        name: String,
        positive: String,
        negative: String,
        spec: SourceSpec,
    },
    CurrentSource {
        name: String,
        positive: String,
        negative: String,
        spec: SourceSpec,
    },
    Vccs {
        name: String,
        positive: String,
        negative: String,
        control_positive: String,
        control_negative: String,
        transconductance: f64,
    },
    Vcvs {
        name: String,
        positive: String,
        negative: String,
        control_positive: String,
        control_negative: String,
        gain: f64,
    },
    Cccs {
        name: String,
        positive: String,
        negative: String,
        control_source: String,
        gain: f64,
    },
    Ccvs {
        name: String,
        positive: String,
        negative: String,
        control_source: String,
        transresistance: f64,
    },
    Diode {
        name: String,
        anode: String,
        cathode: String,
        model: String,
    },
    Mosfet {
        name: String,
        drain: String,
        gate: String,
        source: String,
        body: String,
        model: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiodeModel {
    pub name: String,
    pub saturation_current: f64,
    pub emission_coefficient: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MosModel {
    pub name: String,
    pub pmos: bool,
    pub threshold: f64,
    pub transconductance: f64,
    pub lambda: f64,
}

impl Default for MosModel {
    fn default() -> Self {
        Self {
            name: "DEFAULT_NMOS".to_string(),
            pmos: false,
            threshold: 1.0,
            transconductance: 1e-3,
            lambda: 0.0,
        }
    }
}

impl Default for DiodeModel {
    fn default() -> Self {
        Self {
            name: "DEFAULT_DIODE".to_string(),
            saturation_current: 1e-14,
            emission_coefficient: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceWaveform {
    Dc,
    Sin {
        offset: f64,
        amplitude: f64,
        frequency: f64,
        delay: f64,
        damping: f64,
    },
    Pulse {
        v1: f64,
        v2: f64,
        delay: f64,
        rise: f64,
        fall: f64,
        width: f64,
        period: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpec {
    pub dc_value: f64,
    pub ac_magnitude: f64,
    pub ac_phase_deg: f64,
    pub waveform: SourceWaveform,
}

impl Default for SourceSpec {
    fn default() -> Self {
        Self {
            dc_value: 0.0,
            ac_magnitude: 0.0,
            ac_phase_deg: 0.0,
            waveform: SourceWaveform::Dc,
        }
    }
}

impl SourceSpec {
    pub fn value_at(&self, time: f64) -> f64 {
        match self.waveform {
            SourceWaveform::Dc => self.dc_value,
            SourceWaveform::Sin {
                offset,
                amplitude,
                frequency,
                delay,
                damping,
            } => {
                if time < delay {
                    offset
                } else {
                    let shifted = time - delay;
                    let env = (-damping * shifted).exp();
                    offset
                        + amplitude
                            * env
                            * (2.0 * std::f64::consts::PI * frequency * shifted).sin()
                }
            }
            SourceWaveform::Pulse {
                v1,
                v2,
                delay,
                rise,
                fall,
                width,
                period,
            } => {
                if time < delay {
                    return v1;
                }
                let local = if period > 0.0 {
                    (time - delay) % period
                } else {
                    time - delay
                };
                if local < rise {
                    let alpha = if rise <= 0.0 { 1.0 } else { local / rise };
                    v1 + (v2 - v1) * alpha
                } else if local < rise + width {
                    v2
                } else if local < rise + width + fall {
                    let alpha = if fall <= 0.0 {
                        1.0
                    } else {
                        (local - rise - width) / fall
                    };
                    v2 + (v1 - v2) * alpha
                } else {
                    v1
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Analysis {
    OperatingPoint,
    DcSweep {
        source_name: String,
        start: f64,
        stop: f64,
        step: f64,
    },
    Ac {
        sweep: AcSweep,
        points: usize,
        start_hz: f64,
        stop_hz: f64,
    },
    Transient {
        step: f64,
        stop: f64,
        start: f64,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AcSweep {
    Linear,
    Decade,
    Octave,
}

#[derive(Debug, Clone)]
struct SubcircuitDefinition {
    name: String,
    pins: Vec<String>,
    body: Vec<String>,
}

pub fn parse_netlist(input: &str) -> Result<Netlist> {
    let (title, expanded_lines) = preprocess_netlist(input)?;
    let mut circuit = Circuit::default();
    circuit
        .models
        .insert("DEFAULT_DIODE".to_string(), DiodeModel::default());
    circuit
        .mos_models
        .insert("DEFAULT_NMOS".to_string(), MosModel::default());
    let mut analyses = Vec::new();

    for (line_no, line) in expanded_lines.iter().enumerate() {
        if line.eq_ignore_ascii_case(".end") {
            break;
        }
        if line.starts_with('.') {
            parse_command(line_no + 2, line, &mut circuit, &mut analyses)?;
            continue;
        }

        let tokens = tokenize(line);
        let element = parse_element_tokens(&tokens, line_no + 2)?;
        circuit.elements.push(element);
    }

    if analyses.is_empty() {
        analyses.push(Analysis::OperatingPoint);
    }

    Ok(Netlist {
        title,
        circuit,
        analyses,
    })
}

fn preprocess_netlist(input: &str) -> Result<(String, Vec<String>)> {
    let normalized = input.replace("\r\n", "\n");
    let mut raw_lines = normalized.lines();
    let title = raw_lines
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .unwrap_or("Untitled circuit")
        .to_string();

    let mut subcircuits = BTreeMap::<String, SubcircuitDefinition>::new();
    let mut top_level = Vec::<String>::new();
    let mut current_subckt: Option<SubcircuitDefinition> = None;

    for raw in raw_lines {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if let Some(subckt) = &mut current_subckt {
            if line.to_ascii_uppercase().starts_with(".ENDS") {
                let finished = current_subckt.take().expect("subckt exists");
                subcircuits.insert(finished.name.clone(), finished);
            } else {
                subckt.body.push(line.to_string());
            }
            continue;
        }

        if line.to_ascii_uppercase().starts_with(".SUBCKT") {
            let tokens = tokenize(line);
            let name = tokens.get(1).cloned().context(".subckt missing name")?;
            let pins = tokens[2..].to_vec();
            current_subckt = Some(SubcircuitDefinition {
                name,
                pins,
                body: Vec::new(),
            });
            continue;
        }

        top_level.push(line.to_string());
    }

    if current_subckt.is_some() {
        bail!("unterminated .subckt block");
    }

    let expanded = expand_lines(&top_level, &subcircuits, 0)?;
    Ok((title, expanded))
}

fn expand_lines(
    lines: &[String],
    subcircuits: &BTreeMap<String, SubcircuitDefinition>,
    depth: usize,
) -> Result<Vec<String>> {
    if depth > 32 {
        bail!("subcircuit expansion exceeded recursion limit");
    }

    let mut expanded = Vec::new();
    for line in lines {
        let tokens = tokenize(line);
        if tokens.is_empty() {
            continue;
        }
        if is_subckt_instance_tokens(&tokens, subcircuits) {
            expanded.extend(expand_instance(&tokens, subcircuits, depth + 1)?);
        } else {
            expanded.push(line.clone());
        }
    }
    Ok(expanded)
}

fn expand_instance(
    tokens: &[String],
    subcircuits: &BTreeMap<String, SubcircuitDefinition>,
    depth: usize,
) -> Result<Vec<String>> {
    let instance_name = tokens
        .first()
        .cloned()
        .context("subcircuit instance missing name")?;
    let subckt_name = tokens
        .last()
        .cloned()
        .context("subcircuit instance missing target")?;
    let definition = subcircuits
        .get(&subckt_name)
        .with_context(|| format!("unknown subcircuit '{subckt_name}'"))?;
    let connections = &tokens[1..tokens.len() - 1];
    if connections.len() != definition.pins.len() {
        bail!(
            "instance '{}' expected {} pins for subcircuit '{}', found {}",
            instance_name,
            definition.pins.len(),
            subckt_name,
            connections.len()
        );
    }

    let pin_map = definition
        .pins
        .iter()
        .cloned()
        .zip(connections.iter().cloned())
        .collect::<BTreeMap<_, _>>();

    let mut rewritten = Vec::new();
    for body_line in &definition.body {
        let mapped = rewrite_subckt_line(body_line, &instance_name, &pin_map)?;
        let mapped_tokens = tokenize(&mapped);
        if is_subckt_instance_tokens(&mapped_tokens, subcircuits) {
            rewritten.extend(expand_instance(&mapped_tokens, subcircuits, depth + 1)?);
        } else {
            rewritten.push(mapped);
        }
    }
    Ok(rewritten)
}

fn is_subckt_instance_tokens(
    tokens: &[String],
    subcircuits: &BTreeMap<String, SubcircuitDefinition>,
) -> bool {
    tokens
        .first()
        .map(|name| name.starts_with('X') || name.starts_with('x'))
        .unwrap_or(false)
        && tokens
            .last()
            .map(|last| subcircuits.contains_key(last))
            .unwrap_or(false)
}

fn rewrite_subckt_line(
    line: &str,
    instance_name: &str,
    pin_map: &BTreeMap<String, String>,
) -> Result<String> {
    let mut tokens = tokenize(line);
    if tokens.is_empty() {
        return Ok(String::new());
    }
    if tokens[0].starts_with('.') {
        return Ok(line.to_string());
    }

    let original_name = tokens[0].clone();
    tokens[0] = format!("{}@{}", original_name, instance_name);
    let kind = tokens[0]
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase())
        .context("missing device name")?;

    let node_indices: &[usize] = match kind {
        'R' | 'C' | 'L' | 'V' | 'I' | 'D' => &[1, 2],
        'G' | 'E' | 'M' => &[1, 2, 3, 4],
        'F' | 'H' => &[1, 2],
        'X' => {
            if tokens.len() < 3 {
                bail!("subcircuit instance inside subcircuit is malformed");
            }
            let upper = tokens.len() - 1;
            for token in tokens.iter_mut().take(upper).skip(1) {
                *token = map_subckt_node(token, instance_name, pin_map);
            }
            return Ok(tokens.join(" "));
        }
        _ => &[1, 2],
    };

    for idx in node_indices {
        if let Some(token) = tokens.get_mut(*idx) {
            *token = map_subckt_node(token, instance_name, pin_map);
        }
    }
    Ok(tokens.join(" "))
}

fn map_subckt_node(node: &str, instance_name: &str, pin_map: &BTreeMap<String, String>) -> String {
    if is_ground_name(node) {
        node.to_string()
    } else if let Some(mapped) = pin_map.get(node) {
        mapped.clone()
    } else {
        format!("{instance_name}:{node}")
    }
}

fn parse_element_tokens(tokens: &[String], line_no: usize) -> Result<Element> {
    if tokens.len() < 4 {
        bail!("line {}: expected at least 4 tokens", line_no);
    }

    let name = tokens[0].clone();
    let kind = name
        .chars()
        .next()
        .ok_or_else(|| anyhow!("line {}: empty element name", line_no))?
        .to_ascii_uppercase();

    match kind {
        'R' => Ok(Element::Resistor {
            name,
            a: tokens[1].clone(),
            b: tokens[2].clone(),
            value: parse_number(&tokens[3])?,
        }),
        'C' => Ok(Element::Capacitor {
            name,
            a: tokens[1].clone(),
            b: tokens[2].clone(),
            value: parse_number(&tokens[3])?,
            ic: parse_optional_ic(&tokens[4..])?,
        }),
        'L' => Ok(Element::Inductor {
            name,
            a: tokens[1].clone(),
            b: tokens[2].clone(),
            value: parse_number(&tokens[3])?,
            ic: parse_optional_ic(&tokens[4..])?,
        }),
        'V' => Ok(Element::VoltageSource {
            name,
            positive: tokens[1].clone(),
            negative: tokens[2].clone(),
            spec: parse_source_spec(&tokens[3..])?,
        }),
        'I' => Ok(Element::CurrentSource {
            name,
            positive: tokens[1].clone(),
            negative: tokens[2].clone(),
            spec: parse_source_spec(&tokens[3..])?,
        }),
        'G' => Ok(Element::Vccs {
            name,
            positive: tokens.get(1).cloned().context("VCCS missing output+ node")?,
            negative: tokens.get(2).cloned().context("VCCS missing output- node")?,
            control_positive: tokens.get(3).cloned().context("VCCS missing control+ node")?,
            control_negative: tokens.get(4).cloned().context("VCCS missing control- node")?,
            transconductance: parse_number(tokens.get(5).context("VCCS missing gain")?)?,
        }),
        'E' => Ok(Element::Vcvs {
            name,
            positive: tokens.get(1).cloned().context("VCVS missing output+ node")?,
            negative: tokens.get(2).cloned().context("VCVS missing output- node")?,
            control_positive: tokens.get(3).cloned().context("VCVS missing control+ node")?,
            control_negative: tokens.get(4).cloned().context("VCVS missing control- node")?,
            gain: parse_number(tokens.get(5).context("VCVS missing gain")?)?,
        }),
        'F' => Ok(Element::Cccs {
            name,
            positive: tokens.get(1).cloned().context("CCCS missing output+ node")?,
            negative: tokens.get(2).cloned().context("CCCS missing output- node")?,
            control_source: tokens.get(3).cloned().context("CCCS missing controlling source")?,
            gain: parse_number(tokens.get(4).context("CCCS missing gain")?)?,
        }),
        'H' => Ok(Element::Ccvs {
            name,
            positive: tokens.get(1).cloned().context("CCVS missing output+ node")?,
            negative: tokens.get(2).cloned().context("CCVS missing output- node")?,
            control_source: tokens.get(3).cloned().context("CCVS missing controlling source")?,
            transresistance: parse_number(tokens.get(4).context("CCVS missing transresistance")?)?,
        }),
        'D' => Ok(Element::Diode {
            name,
            anode: tokens[1].clone(),
            cathode: tokens[2].clone(),
            model: tokens
                .get(3)
                .cloned()
                .unwrap_or_else(|| "DEFAULT_DIODE".to_string()),
        }),
        'M' => Ok(Element::Mosfet {
            name,
            drain: tokens.get(1).cloned().context("MOSFET missing drain node")?,
            gate: tokens.get(2).cloned().context("MOSFET missing gate node")?,
            source: tokens.get(3).cloned().context("MOSFET missing source node")?,
            body: tokens.get(4).cloned().context("MOSFET missing body node")?,
            model: tokens.get(5).cloned().context("MOSFET missing model name")?,
        }),
        other => bail!("line {}: unsupported device '{}'", line_no, other),
    }
}

fn strip_comment(line: &str) -> &str {
    line.split(';').next().unwrap_or(line)
}

fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0usize;
    for ch in line.chars() {
        match ch {
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
            }
            ' ' | '\t' if paren_depth == 0 => {
                if !current.is_empty() {
                    tokens.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current.trim().to_string());
    }
    tokens
}

fn parse_optional_ic(tokens: &[String]) -> Result<Option<f64>> {
    for token in tokens {
        if let Some(value) = token.strip_prefix("IC=") {
            return Ok(Some(parse_number(value)?));
        }
    }
    Ok(None)
}

fn parse_source_spec(tokens: &[String]) -> Result<SourceSpec> {
    if tokens.is_empty() {
        return Ok(SourceSpec::default());
    }
    if tokens.len() == 1 {
        let token = &tokens[0];
        if token.to_ascii_uppercase().starts_with("SIN") {
            let args = extract_args(token, "SIN")?;
            return Ok(SourceSpec {
                dc_value: args.first().copied().unwrap_or_default(),
                waveform: SourceWaveform::Sin {
                    offset: args.first().copied().unwrap_or_default(),
                    amplitude: args.get(1).copied().unwrap_or(1.0),
                    frequency: args.get(2).copied().unwrap_or(1.0),
                    delay: args.get(3).copied().unwrap_or(0.0),
                    damping: args.get(4).copied().unwrap_or(0.0),
                },
                ..SourceSpec::default()
            });
        }
        if token.to_ascii_uppercase().starts_with("PULSE") {
            let args = extract_args(token, "PULSE")?;
            return Ok(SourceSpec {
                dc_value: args.first().copied().unwrap_or_default(),
                waveform: SourceWaveform::Pulse {
                    v1: args.first().copied().unwrap_or_default(),
                    v2: args.get(1).copied().unwrap_or(1.0),
                    delay: args.get(2).copied().unwrap_or(0.0),
                    rise: args.get(3).copied().unwrap_or(1e-12),
                    fall: args.get(4).copied().unwrap_or(1e-12),
                    width: args.get(5).copied().unwrap_or(1e-9),
                    period: args.get(6).copied().unwrap_or(2e-9),
                },
                ..SourceSpec::default()
            });
        }
        return Ok(SourceSpec {
            dc_value: parse_number(token)?,
            ..SourceSpec::default()
        });
    }

    let mut spec = SourceSpec::default();
    let mut idx = 0usize;
    while idx < tokens.len() {
        let token = &tokens[idx];
        let upper = token.to_ascii_uppercase();
        if upper == "DC" {
            idx += 1;
            spec.dc_value = parse_number(tokens.get(idx).context("missing DC source value")?)?;
        } else if upper == "AC" {
            idx += 1;
            spec.ac_magnitude = parse_number(tokens.get(idx).context("missing AC magnitude")?)?;
            if let Some(phase) = tokens.get(idx + 1) {
                if !phase.eq_ignore_ascii_case("SIN")
                    && !phase.eq_ignore_ascii_case("PULSE")
                    && !phase.to_ascii_uppercase().starts_with("SIN(")
                    && !phase.to_ascii_uppercase().starts_with("PULSE(")
                {
                    spec.ac_phase_deg = parse_number(phase)?;
                    idx += 1;
                }
            }
        } else if upper.starts_with("SIN") {
            let args = extract_args(token, "SIN")?;
            spec.waveform = SourceWaveform::Sin {
                offset: args.first().copied().unwrap_or(spec.dc_value),
                amplitude: args.get(1).copied().unwrap_or(1.0),
                frequency: args.get(2).copied().unwrap_or(1.0),
                delay: args.get(3).copied().unwrap_or(0.0),
                damping: args.get(4).copied().unwrap_or(0.0),
            };
        } else if upper.starts_with("PULSE") {
            let args = extract_args(token, "PULSE")?;
            spec.waveform = SourceWaveform::Pulse {
                v1: args.first().copied().unwrap_or(spec.dc_value),
                v2: args.get(1).copied().unwrap_or(1.0),
                delay: args.get(2).copied().unwrap_or(0.0),
                rise: args.get(3).copied().unwrap_or(1e-12),
                fall: args.get(4).copied().unwrap_or(1e-12),
                width: args.get(5).copied().unwrap_or(1e-9),
                period: args.get(6).copied().unwrap_or(2e-9),
            };
        } else if idx == 0 {
            spec.dc_value = parse_number(token)?;
        }
        idx += 1;
    }
    Ok(spec)
}

fn extract_args(token: &str, head: &str) -> Result<Vec<f64>> {
    let trimmed = token.trim();
    let inner = trimmed
        .strip_prefix(head)
        .or_else(|| trimmed.strip_prefix(&head.to_ascii_lowercase()))
        .unwrap_or(trimmed)
        .trim();
    let inner = inner
        .strip_prefix('(')
        .and_then(|text| text.strip_suffix(')'))
        .ok_or_else(|| anyhow!("expected {}(...) source specification", head))?;
    inner
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .map(parse_number)
        .collect()
}

fn parse_command(
    line_no: usize,
    line: &str,
    circuit: &mut Circuit,
    analyses: &mut Vec<Analysis>,
) -> Result<()> {
    let tokens = tokenize(line);
    let cmd = tokens[0].to_ascii_uppercase();
    match cmd.as_str() {
        ".OP" => analyses.push(Analysis::OperatingPoint),
        ".TRAN" => {
            let step = parse_number(tokens.get(1).context(".tran missing step")?)?;
            let stop = parse_number(tokens.get(2).context(".tran missing stop")?)?;
            let start = tokens
                .get(3)
                .map(|v| parse_number(v))
                .transpose()?
                .unwrap_or(0.0);
            analyses.push(Analysis::Transient { step, stop, start });
        }
        ".DC" => {
            analyses.push(Analysis::DcSweep {
                source_name: tokens.get(1).cloned().context(".dc missing source")?,
                start: parse_number(tokens.get(2).context(".dc missing start")?)?,
                stop: parse_number(tokens.get(3).context(".dc missing stop")?)?,
                step: parse_number(tokens.get(4).context(".dc missing step")?)?,
            });
        }
        ".AC" => {
            let sweep = match tokens
                .get(1)
                .map(|v| v.to_ascii_uppercase())
                .as_deref()
                .unwrap_or("DEC")
            {
                "LIN" => AcSweep::Linear,
                "OCT" => AcSweep::Octave,
                _ => AcSweep::Decade,
            };
            analyses.push(Analysis::Ac {
                sweep,
                points: tokens
                    .get(2)
                    .context(".ac missing point count")?
                    .parse::<usize>()?,
                start_hz: parse_number(tokens.get(3).context(".ac missing start")?)?,
                stop_hz: parse_number(tokens.get(4).context(".ac missing stop")?)?,
            });
        }
        ".MODEL" => {
            let name = tokens.get(1).cloned().context(".model missing name")?;
            let model_type = tokens
                .get(2)
                .map(|v| v.to_ascii_uppercase())
                .context(".model missing type")?;
            if model_type == "D" {
                let mut model = DiodeModel {
                    name: name.clone(),
                    ..DiodeModel::default()
                };
                for token in &tokens[3..] {
                    let (key, value) = token
                        .split_once('=')
                        .ok_or_else(|| anyhow!("line {}: expected key=value in .model", line_no))?;
                    match key.to_ascii_uppercase().as_str() {
                        "IS" => model.saturation_current = parse_number(value)?,
                        "N" => model.emission_coefficient = parse_number(value)?,
                        _ => {}
                    }
                }
                circuit.models.insert(name, model);
            } else if model_type == "NMOS" || model_type == "PMOS" {
                let mut model = MosModel {
                    name: name.clone(),
                    pmos: model_type == "PMOS",
                    ..MosModel::default()
                };
                for token in &tokens[3..] {
                    let (key, value) = token
                        .split_once('=')
                        .ok_or_else(|| anyhow!("line {}: expected key=value in .model", line_no))?;
                    match key.to_ascii_uppercase().as_str() {
                        "KP" | "BETA" => model.transconductance = parse_number(value)?,
                        "VTO" | "VT0" | "VTH" => model.threshold = parse_number(value)?,
                        "LAMBDA" => model.lambda = parse_number(value)?,
                        _ => {}
                    }
                }
                circuit.mos_models.insert(name, model);
            } else {
                bail!(
                    "line {}: only diode and basic NMOS/PMOS models are supported right now",
                    line_no
                );
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn parse_number(token: &str) -> Result<f64> {
    let token = token.trim();
    if token.is_empty() {
        bail!("empty numeric token");
    }
    let lower = token.to_ascii_lowercase();
    let suffixes = [
        ("meg", 1e6),
        ("t", 1e12),
        ("g", 1e9),
        ("k", 1e3),
        ("m", 1e-3),
        ("u", 1e-6),
        ("n", 1e-9),
        ("p", 1e-12),
        ("f", 1e-15),
    ];
    for (suffix, scale) in suffixes {
        if lower.ends_with(suffix) && lower.len() > suffix.len() {
            let base = &token[..token.len() - suffix.len()];
            return Ok(base.parse::<f64>()? * scale);
        }
    }
    Ok(token.parse::<f64>()?)
}

fn is_ground_name(name: &str) -> bool {
    matches!(name, "0" | "gnd" | "GND")
}
