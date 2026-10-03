use std::{fs, path::PathBuf};

use anyhow::Context;
use serde::Serialize;
use spice_core::{parse_netlist, simulate, SchematicProject, SimulationConfig};

#[derive(Debug, Serialize)]
struct SaveResult {
    manifest_path: String,
    spice_path: String,
    svg_path: String,
}

#[tauri::command]
fn run_simulation(netlist: String) -> Result<spice_core::SimulationResult, String> {
    let parsed = parse_netlist(&netlist).map_err(|error| error.to_string())?;
    simulate(&parsed, &SimulationConfig::default()).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_project_bundle(project_name: String, svg: String, netlist: String) -> Result<SaveResult, String> {
    let output_dir = PathBuf::from("/workspace/spice-studio/output").join(&project_name);
    fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    let spice_path = output_dir.join(format!("{project_name}.cir"));
    let svg_path = output_dir.join(format!("{project_name}.svg"));
    let manifest_path = output_dir.join(format!("{project_name}.project.json"));

    fs::write(&spice_path, netlist).map_err(|error| error.to_string())?;
    fs::write(&svg_path, svg).map_err(|error| error.to_string())?;

    let manifest = SchematicProject {
        name: project_name,
        spice_path: spice_path.display().to_string(),
        svg_path: svg_path.display().to_string(),
    };
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .with_context(|| format!("failed to write {}", manifest_path.display()))
    .map_err(|error| error.to_string())?;

    Ok(SaveResult {
        manifest_path: manifest_path.display().to_string(),
        spice_path: spice_path.display().to_string(),
        svg_path: svg_path.display().to_string(),
    })
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![run_simulation, save_project_bundle])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
