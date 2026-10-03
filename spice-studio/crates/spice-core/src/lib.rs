pub mod netlist;
pub mod schematic;
pub mod simulator;

pub use netlist::{parse_netlist, Analysis, Circuit, Element, Netlist, SourceSpec};
pub use schematic::{SchematicDocument, SchematicProject, SchematicSymbol};
pub use simulator::{
    simulate, AnalysisOutput, OperatingPoint, SimulationConfig, SimulationResult, SweepPoint,
    WaveformSet,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_engineering_suffixes() {
        let netlist = parse_netlist(
            "Suffixes\nV1 in 0 5\nR1 in out 10k\nC1 out 0 22u\n.op\n.end",
        )
        .expect("parse should succeed");

        assert_eq!(netlist.circuit.elements.len(), 3);
        match &netlist.circuit.elements[1] {
            Element::Resistor { value, .. } => assert!((value - 10_000.0).abs() < 1e-9),
            other => panic!("unexpected element: {other:?}"),
        }
        match &netlist.circuit.elements[2] {
            Element::Capacitor { value, .. } => assert!((value - 22e-6).abs() < 1e-12),
            other => panic!("unexpected element: {other:?}"),
        }
    }

    #[test]
    fn resistor_divider_operating_point() {
        let netlist = parse_netlist(
            "Divider\nV1 in 0 DC 10\nR1 in out 10k\nR2 out 0 10k\n.op\n.end",
        )
        .expect("parse should succeed");
        let result = simulate(&netlist, &SimulationConfig::default()).expect("simulate");

        match &result.analyses[0] {
            AnalysisOutput::OperatingPoint(op) => {
                let vout = op.variables.get("V(out)").copied().unwrap_or_default();
                assert!((vout - 5.0).abs() < 1e-6);
            }
            other => panic!("unexpected analysis: {other:?}"),
        }
    }

    #[test]
    fn rc_transient_reaches_expected_target() {
        let netlist = parse_netlist(
            "RC\nV1 in 0 PULSE(0 5 0 1n 1n 1m 2m)\nR1 in out 1k\nC1 out 0 1u\n.tran 100u 5m\n.end",
        )
        .expect("parse should succeed");
        let result = simulate(&netlist, &SimulationConfig::default()).expect("simulate");

        match &result.analyses[0] {
            AnalysisOutput::Transient(tran) => {
                let out = tran.traces.get("V(out)").expect("trace");
                let peak = out.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                assert!(peak > 3.0);
            }
            other => panic!("unexpected analysis: {other:?}"),
        }
    }

    #[test]
    fn vccs_operating_point_matches_transconductance() {
        let netlist = parse_netlist(
            "VCCS\nV1 ctrl 0 DC 2\nR1 out 0 1k\nG1 out 0 ctrl 0 1m\n.op\n.end",
        )
        .expect("parse should succeed");
        let result = simulate(&netlist, &SimulationConfig::default()).expect("simulate");

        match &result.analyses[0] {
            AnalysisOutput::OperatingPoint(op) => {
                let vout = op.variables.get("V(out)").copied().unwrap_or_default();
                assert!((vout + 2.0).abs() < 1e-6);
            }
            other => panic!("unexpected analysis: {other:?}"),
        }
    }

    #[test]
    fn subcircuit_expansion_preserves_divider_behavior() {
        let netlist = parse_netlist(
            "Subckt\n.subckt DIV in mid out\nR1 in mid 10k\nR2 mid out 10k\n.ends\nV1 vin 0 DC 10\nX1 vin vout 0 DIV\n.op\n.end",
        )
        .expect("parse should succeed");
        let result = simulate(&netlist, &SimulationConfig::default()).expect("simulate");

        match &result.analyses[0] {
            AnalysisOutput::OperatingPoint(op) => {
                let vout = op.variables.get("V(vout)").copied().unwrap_or_default();
                assert!((vout - 5.0).abs() < 1e-6);
            }
            other => panic!("unexpected analysis: {other:?}"),
        }
    }

    #[test]
    fn cccs_operating_point_tracks_voltage_source_current() {
        let netlist = parse_netlist(
            "CCCS\nV1 in 0 DC 5\nR1 in 0 1k\nR2 out 0 1k\nF1 out 0 V1 2\n.op\n.end",
        )
        .expect("parse should succeed");
        let result = simulate(&netlist, &SimulationConfig::default()).expect("simulate");

        match &result.analyses[0] {
            AnalysisOutput::OperatingPoint(op) => {
                let vout = op.variables.get("V(out)").copied().unwrap_or_default();
                assert!((vout - 10.0).abs() < 1e-6);
            }
            other => panic!("unexpected analysis: {other:?}"),
        }
    }

    #[test]
    fn ccvs_operating_point_tracks_voltage_source_current() {
        let netlist = parse_netlist(
            "CCVS\nV1 in 0 DC 5\nR1 in 0 1k\nR2 out 0 1k\nH1 out 0 V1 500\n.op\n.end",
        )
        .expect("parse should succeed");
        let result = simulate(&netlist, &SimulationConfig::default()).expect("simulate");

        match &result.analyses[0] {
            AnalysisOutput::OperatingPoint(op) => {
                let vout = op.variables.get("V(out)").copied().unwrap_or_default();
                assert!((vout + 2.5).abs() < 1e-6);
            }
            other => panic!("unexpected analysis: {other:?}"),
        }
    }

    #[test]
    fn parses_basic_mosfet_model_and_device() {
        let netlist = parse_netlist(
            "MOS Parse\n.model NM1 NMOS VTO=1 KP=2m LAMBDA=0.02\nM1 out in 0 0 NM1\n.op\n.end",
        )
        .expect("parse should succeed");

        assert!(netlist.circuit.mos_models.contains_key("NM1"));
        match &netlist.circuit.elements[0] {
            Element::Mosfet {
                drain,
                gate,
                source,
                body,
                model,
                ..
            } => {
                assert_eq!(drain, "out");
                assert_eq!(gate, "in");
                assert_eq!(source, "0");
                assert_eq!(body, "0");
                assert_eq!(model, "NM1");
            }
            other => panic!("unexpected element: {other:?}"),
        }
    }

    #[test]
    fn nmos_common_source_produces_gain() {
        let netlist = parse_netlist(
            "NMOS Gain\n.model NM1 NMOS VTO=1 KP=2m LAMBDA=0.02\nVDD vdd 0 DC 5\nVIN in 0 DC 3\nRD vdd out 1k\nM1 out in 0 0 NM1\n.op\n.end",
        )
        .expect("parse should succeed");
        let result = simulate(&netlist, &SimulationConfig::default()).expect("simulate");

        match &result.analyses[0] {
            AnalysisOutput::OperatingPoint(op) => {
                let vout = op.variables.get("V(out)").copied().unwrap_or_default();
                assert!(vout > 0.0 && vout < 5.0);
            }
            other => panic!("unexpected analysis: {other:?}"),
        }
    }
}
