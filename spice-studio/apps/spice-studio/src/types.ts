export type Tool =
  | "select"
  | "wire"
  | "resistor"
  | "capacitor"
  | "inductor"
  | "vsource"
  | "isource"
  | "vccs"
  | "vcvs"
  | "cccs"
  | "ccvs"
  | "diode"
  | "ground";

export interface SymbolRecord {
  id: string;
  kind: Tool;
  x: number;
  y: number;
  reference: string;
  value: string;
  nodeA: string;
  nodeB: string;
  controlNodeA?: string;
  controlNodeB?: string;
  rotationDeg: number;
}

export interface WireRecord {
  id: string;
  net: string;
  points: Array<[number, number]>;
}

export interface OperatingPointResult {
  variables: Record<string, number>;
}

export interface AcResult {
  frequency: number[];
  magnitude: Record<string, number[]>;
  phase_deg: Record<string, number[]>;
}

export interface TransientResult {
  time: number[];
  traces: Record<string, number[]>;
}

export type AnalysisResult =
  | { OperatingPoint: OperatingPointResult }
  | { Ac: AcResult }
  | { Transient: TransientResult }
  | { DcSweep: unknown };

export interface SimulationOutput {
  title: string;
  analyses: AnalysisResult[];
}
