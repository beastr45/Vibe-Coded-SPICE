use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use spice_core::{parse_netlist, simulate, AnalysisOutput, SimulationConfig};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Csv,
}

#[derive(Debug, Parser)]
#[command(author, version, about = "Standalone SPICE/Xyce-inspired circuit simulator CLI")]
struct Cli {
    #[arg(help = "Path to the SPICE netlist (.cir, .sp, .xyce)")]
    input: PathBuf,

    #[arg(short, long, help = "Optional output file. Defaults to stdout.")]
    output: Option<PathBuf>,

    #[arg(short, long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,

    #[arg(long, default_value_t = 50)]
    max_iters: usize,

    #[arg(long, default_value_t = 1e-8)]
    rel_tol: f64,

    #[arg(long, default_value_t = 1e-12)]
    abs_tol: f64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let contents = fs::read_to_string(&cli.input)
        .with_context(|| format!("failed to read {}", cli.input.display()))?;
    let netlist = parse_netlist(&contents)?;
    let config = SimulationConfig {
        max_iters: cli.max_iters,
        rel_tol: cli.rel_tol,
        abs_tol: cli.abs_tol,
    };
    let result = simulate(&netlist, &config)?;

    let rendered = match cli.format {
        OutputFormat::Json => serde_json::to_string_pretty(&result)?,
        OutputFormat::Csv => render_csv(&result),
    };

    if let Some(path) = cli.output {
        fs::write(&path, rendered)
            .with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        println!("{rendered}");
    }

    Ok(())
}

fn render_csv(result: &spice_core::SimulationResult) -> String {
    let mut out = String::new();
    for analysis in &result.analyses {
        match analysis {
            AnalysisOutput::OperatingPoint(op) => {
                out.push_str("analysis,variable,value\n");
                for (name, value) in &op.variables {
                    out.push_str(&format!("op,{name},{value}\n"));
                }
            }
            AnalysisOutput::DcSweep(sweep) => {
                let mut headers = vec![sweep.source.clone()];
                let keys = sweep
                    .points
                    .first()
                    .map(|p| p.variables.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                headers.extend(keys.iter().cloned());
                out.push_str(&headers.join(","));
                out.push('\n');
                for point in &sweep.points {
                    out.push_str(&point.swept_value.to_string());
                    for key in &keys {
                        let value = point.variables.get(key).copied().unwrap_or_default();
                        out.push_str(&format!(",{value}"));
                    }
                    out.push('\n');
                }
            }
            AnalysisOutput::Transient(tran) => {
                let mut keys = tran.traces.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                out.push_str("time");
                for key in &keys {
                    out.push_str(&format!(",{key}"));
                }
                out.push('\n');
                for idx in 0..tran.time.len() {
                    out.push_str(&tran.time[idx].to_string());
                    for key in &keys {
                        let value = tran
                            .traces
                            .get(key)
                            .and_then(|trace| trace.get(idx))
                            .copied()
                            .unwrap_or_default();
                        out.push_str(&format!(",{value}"));
                    }
                    out.push('\n');
                }
            }
            AnalysisOutput::Ac(ac) => {
                let mut keys = ac.magnitude.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                out.push_str("frequency");
                for key in &keys {
                    out.push_str(&format!(",{key}:mag,{key}:phase_deg"));
                }
                out.push('\n');
                for idx in 0..ac.frequency.len() {
                    out.push_str(&ac.frequency[idx].to_string());
                    for key in &keys {
                        let mag = ac
                            .magnitude
                            .get(key)
                            .and_then(|trace| trace.get(idx))
                            .copied()
                            .unwrap_or_default();
                        let phase = ac
                            .phase_deg
                            .get(key)
                            .and_then(|trace| trace.get(idx))
                            .copied()
                            .unwrap_or_default();
                        out.push_str(&format!(",{mag},{phase}"));
                    }
                    out.push('\n');
                }
            }
        }
        out.push('\n');
    }
    out
}
