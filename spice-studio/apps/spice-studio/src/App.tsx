import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  AcResult,
  AnalysisResult,
  SimulationOutput,
  SymbolRecord,
  Tool,
  TransientResult,
  WireRecord,
} from "./types";

const GRID = 20;

const palette: Array<{
  tool: Tool;
  label: string;
  reference: string;
  value: string;
  controlA?: string;
  controlB?: string;
}> = [
  { tool: "select", label: "Select", reference: "", value: "" },
  { tool: "wire", label: "Wire", reference: "", value: "" },
  { tool: "resistor", label: "Resistor", reference: "R?", value: "1k" },
  { tool: "capacitor", label: "Capacitor", reference: "C?", value: "1u" },
  { tool: "inductor", label: "Inductor", reference: "L?", value: "1m" },
  { tool: "vsource", label: "Voltage", reference: "V?", value: "DC 5" },
  { tool: "isource", label: "Current", reference: "I?", value: "DC 1m" },
  { tool: "vccs", label: "VCCS", reference: "G?", value: "1m", controlA: "ctrlp", controlB: "ctrln" },
  { tool: "vcvs", label: "VCVS", reference: "E?", value: "10", controlA: "ctrlp", controlB: "ctrln" },
  { tool: "cccs", label: "CCCS", reference: "F?", value: "2", controlA: "branch" },
  { tool: "ccvs", label: "CCVS", reference: "H?", value: "500", controlA: "branch" },
  { tool: "diode", label: "Diode", reference: "D?", value: "DEFAULT_DIODE" },
  { tool: "ground", label: "Ground", reference: "0", value: "0" },
];

const sourceTemplates = ["DC 5", "DC 0 AC 1", "SIN(0 1 1k)", "PULSE(0 5 0 1n 1n 1m 2m)"];
const CONTROLLED_SOURCE_KINDS: Tool[] = ["vccs", "vcvs", "cccs", "ccvs"];
const DEFAULT_VISIBLE_TRACE_COUNT = 4;
const MAX_ACTIVE_TRACES = 6;
const CHART_COLORS = ["#7bd3ff", "#89ffa0", "#ffcf6e", "#ff8fa3", "#c7a6ff", "#ffa94d"];

const initialSymbols: SymbolRecord[] = [
  { id: "r1", kind: "resistor", x: 220, y: 200, reference: "R1", value: "1k", nodeA: "in", nodeB: "out", rotationDeg: 0 },
  { id: "c1", kind: "capacitor", x: 420, y: 200, reference: "C1", value: "1u", nodeA: "out", nodeB: "0", rotationDeg: 90 },
  { id: "v1", kind: "vsource", x: 120, y: 200, reference: "V1", value: "SIN(0 1 1k)", nodeA: "in", nodeB: "0", rotationDeg: 0 },
  { id: "g1", kind: "vccs", x: 320, y: 360, reference: "G1", value: "1m", nodeA: "sense", nodeB: "0", controlNodeA: "in", controlNodeB: "0", rotationDeg: 0 },
  { id: "f1", kind: "cccs", x: 540, y: 360, reference: "F1", value: "2", nodeA: "mirror", nodeB: "0", controlNodeA: "V1", rotationDeg: 0 },
];

const initialWires: WireRecord[] = [
  { id: "w1", net: "in", points: [[140, 200], [200, 200]] },
  { id: "w2", net: "out", points: [[240, 200], [400, 200]] },
  { id: "w3", net: "0", points: [[120, 240], [120, 300], [420, 300], [420, 220], [540, 300], [540, 340]] },
  { id: "w4", net: "sense", points: [[320, 360], [420, 360], [420, 300]] },
  { id: "w5", net: "mirror", points: [[540, 360], [620, 360]] },
];

type InferredSymbolNodes = {
  primaryA: string;
  primaryB: string;
  controlA?: string;
  controlB?: string;
};

type InferredSchematic = {
  netlist: string;
  nodesBySymbol: Record<string, InferredSymbolNodes>;
  labeledPoints: Array<{ x: number; y: number; net: string }>;
};

function snap(value: number) {
  return Math.round(value / GRID) * GRID;
}

function createUniqueId(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

function selectInitialTraceKeys(series: Record<string, unknown> | undefined, limit = DEFAULT_VISIBLE_TRACE_COUNT) {
  return Object.keys(series ?? {}).slice(0, limit);
}

function symbolUsesControlNodes(symbol: SymbolRecord) {
  return symbol.kind === "vccs" || symbol.kind === "vcvs";
}

function symbolUsesControlBranch(symbol: SymbolRecord) {
  return symbol.kind === "cccs" || symbol.kind === "ccvs";
}

function App() {
  const [tool, setTool] = useState<Tool>("select");
  const [symbols, setSymbols] = useState<SymbolRecord[]>(initialSymbols);
  const [wires, setWires] = useState<WireRecord[]>(initialWires);
  const [selectedId, setSelectedId] = useState<string>("r1");
  const [status, setStatus] = useState<string>("Ready");
  const [simulation, setSimulation] = useState<SimulationOutput | null>(null);
  const [pendingWireStart, setPendingWireStart] = useState<[number, number] | null>(null);
  const [activeTransientKeys, setActiveTransientKeys] = useState<string[]>([]);
  const [activeAcKeys, setActiveAcKeys] = useState<string[]>([]);
  const [selectedWireId, setSelectedWireId] = useState<string>("w1");

  const selected = symbols.find((symbol) => symbol.id === selectedId) ?? null;
  const selectedWire = wires.find((wire) => wire.id === selectedWireId) ?? null;

  const inferred = useMemo(() => inferSchematic(symbols, wires), [symbols, wires]);

  const svg = useMemo(() => {
    const body = [
      `<svg xmlns="http://www.w3.org/2000/svg" width="2400" height="1600" viewBox="0 0 2400 1600" data-grid="20">`,
      `<rect width="100%" height="100%" fill="#14181f" />`,
    ];
    for (const wire of wires) {
      body.push(`<polyline data-id="${wire.id}" data-net="${wire.net}" fill="none" stroke="#91d7ff" stroke-width="3" points="${wire.points.map((p) => p.join(",")).join(" ")}"/>`);
    }
    for (const point of inferred.labeledPoints) {
      body.push(`<text x="${point.x + 6}" y="${point.y - 6}" fill="#7bd3ff" font-size="14">${point.net}</text>`);
    }
    for (const symbol of symbols) {
      body.push(`<g data-id="${symbol.id}" transform="translate(${symbol.x},${symbol.y}) rotate(${symbol.rotationDeg})">`);
      body.push(`<line x1="-20" y1="0" x2="20" y2="0" stroke="#d6deff" stroke-width="3" />`);
      body.push(`<line x1="0" y1="-20" x2="0" y2="20" stroke="#d6deff" stroke-width="3" />`);
      if (CONTROLLED_SOURCE_KINDS.includes(symbol.kind)) {
        body.push(`<circle cx="0" cy="0" r="16" fill="none" stroke="#ffcf6e" stroke-width="2" />`);
      }
      body.push(`<text x="28" y="-8" fill="#f8fafc" font-size="18">${symbol.reference}</text>`);
      body.push(`<text x="28" y="14" fill="#89ffa0" font-size="16">${symbol.value}</text>`);
      body.push(`</g>`);
    }
    body.push(`</svg>`);
    return body.join("");
  }, [symbols, wires, inferred]);

  const transient = findTransient(simulation?.analyses ?? []);
  const ac = findAc(simulation?.analyses ?? []);
  const transientKeys = useMemo(() => Object.keys(transient?.traces ?? {}), [transient]);
  const acKeys = useMemo(() => Object.keys(ac?.magnitude ?? {}), [ac]);
  const selectedInferred = selected ? inferred.nodesBySymbol[selected.id] : null;

  async function runSimulation() {
    setStatus("Running simulation...");
    try {
      const result = await invoke<SimulationOutput>("run_simulation", { netlist: inferred.netlist });
      setSimulation(result);
      const nextTransient = findTransient(result.analyses);
      const nextAc = findAc(result.analyses);
      setActiveTransientKeys(selectInitialTraceKeys(nextTransient?.traces));
      setActiveAcKeys(selectInitialTraceKeys(nextAc?.magnitude));
      setStatus("Simulation complete");
    } catch (error) {
      setStatus(`Simulation failed: ${String(error)}`);
    }
  }

  async function saveProject() {
    setStatus("Saving project...");
    try {
      await invoke("save_project_bundle", {
        projectName: "spice-studio-demo",
        svg,
        netlist: inferred.netlist,
      });
      setStatus("Project exported to workspace/output");
    } catch (error) {
      setStatus(`Save failed: ${String(error)}`);
    }
  }

  function updateSelected(field: keyof SymbolRecord, value: string | number) {
    setSymbols((current) =>
      current.map((symbol) =>
        symbol.id === selectedId ? { ...symbol, [field]: value } : symbol,
      ),
    );
  }

  function onCanvasClick(event: React.MouseEvent<SVGSVGElement>) {
    const svgElement = event.currentTarget;
    const point = svgElement.createSVGPoint();
    point.x = event.clientX;
    point.y = event.clientY;
    const transformed = point.matrixTransform(svgElement.getScreenCTM()?.inverse());
    const x = snap(transformed.x);
    const y = snap(transformed.y);

    if (tool === "wire") {
      if (!pendingWireStart) {
        setPendingWireStart([x, y]);
        setStatus(`Wire start set at (${x}, ${y})`);
      } else {
        const id = createUniqueId("wire");
        setWires((current) => [...current, { id, net: `net_${current.length + 1}`, points: [pendingWireStart, [x, y]] }]);
        setSelectedWireId(id);
        setPendingWireStart(null);
        setStatus(`Wire ${id} created`);
      }
      return;
    }

    if (tool === "select") {
      setSelectedId("");
      return;
    }

    addSymbolAt(tool, x, y);
  }

  function addSymbolAt(kind: Tool, x: number, y: number) {
    const item = palette.find((entry) => entry.tool === kind);
    if (!item || kind === "select" || kind === "wire") return;
    const index = symbols.filter((symbol) => symbol.kind === kind).length + 1;
    const prefix = item.reference.replace("?", "") || kind[0].toUpperCase();
    const id = createUniqueId(kind);
    setSymbols((current) => [
      ...current,
      {
        id,
        kind,
        x,
        y,
        reference: item.reference.includes("?") ? `${prefix}${index}` : item.reference,
        value: item.value,
        nodeA: `${kind}${index}_a`,
        nodeB: `${kind}${index}_b`,
        controlNodeA: item.controlA ? `${kind}${index}_${item.controlA}` : undefined,
        controlNodeB: item.controlB ? `${kind}${index}_${item.controlB}` : undefined,
        rotationDeg: 0,
      },
    ]);
    setSelectedId(id);
  }

  function nudgeSelected(dx: number, dy: number) {
    setSymbols((current) =>
      current.map((symbol) =>
        symbol.id === selectedId
          ? { ...symbol, x: snap(symbol.x + dx), y: snap(symbol.y + dy) }
          : symbol,
      ),
    );
  }

  function deleteSelected() {
    setSymbols((current) => current.filter((symbol) => symbol.id !== selectedId));
    setSelectedId("");
  }

  function updateSelectedWireNet(net: string) {
    setWires((current) =>
      current.map((wire) => (wire.id === selectedWireId ? { ...wire, net } : wire)),
    );
  }

  function applySourceTemplate(template: string) {
    if (!selected) return;
    if (selected.kind === "vsource" || selected.kind === "isource") {
      updateSelected("value", template);
    }
  }

  return (
    <div className="window-shell">
      <header className="titlebar">
        <div className="title">Spice Studio — LTspice-inspired schematic and simulation environment</div>
        <div className="title-actions">
          <button onClick={runSimulation}>Run</button>
          <button onClick={saveProject}>Export SVG + SPICE</button>
        </div>
      </header>

      <div className="workspace-grid">
        <aside className="panel palette">
          <h2>Palette</h2>
          {palette.map((item) => (
            <button
              key={item.tool}
              className={tool === item.tool ? "palette-item active" : "palette-item"}
              onClick={() => setTool(item.tool)}
            >
              {item.label}
            </button>
          ))}
          <p className="hint">
            Nets are now inferred from wire geometry and terminal positions. Use wire labels only to
            name inferred nets rather than manually typing every node for each component.
          </p>
          {selected && (
            <div className="nudge-box">
              <h3>Move selected</h3>
              <div className="nudge-grid">
                <button onClick={() => nudgeSelected(0, -GRID)}>↑</button>
                <button onClick={() => nudgeSelected(-GRID, 0)}>←</button>
                <button onClick={() => nudgeSelected(GRID, 0)}>→</button>
                <button onClick={() => nudgeSelected(0, GRID)}>↓</button>
              </div>
              <button className="danger" onClick={deleteSelected}>Delete</button>
            </div>
          )}
          {selectedWire && (
            <div className="wire-box">
              <h3>Wire</h3>
              <label>
                Net label
                <input value={selectedWire.net} onChange={(e) => updateSelectedWireNet(e.target.value)} />
              </label>
            </div>
          )}
        </aside>

        <main className="panel canvas-panel">
          <div className="canvas-toolbar">
            Tool: {tool}
            {pendingWireStart && <span className="toolbar-note">Wire start: {pendingWireStart.join(", ")}</span>}
          </div>
          <svg className="schematic-canvas" viewBox="0 0 1000 700" onClick={onCanvasClick}>
            <defs>
              <pattern id="grid" width="20" height="20" patternUnits="userSpaceOnUse">
                <path d="M 20 0 L 0 0 0 20" fill="none" stroke="#223042" strokeWidth="1" />
              </pattern>
            </defs>
            <rect width="1000" height="700" fill="url(#grid)" />
            {wires.map((wire) => {
              const [labelX, labelY] = wire.points[0] ?? [0, 0];
              return (
                <g
                  key={wire.id}
                  className={selectedWireId === wire.id ? "wire-group selected-wire" : "wire-group"}
                  onClick={(event) => {
                    event.stopPropagation();
                    setSelectedWireId(wire.id);
                  }}
                >
                  <polyline
                    points={wire.points.map((point) => point.join(",")).join(" ")}
                    fill="none"
                    stroke="#7bd3ff"
                    strokeWidth="3"
                  />
                  <text x={labelX + 6} y={labelY - 6} className="wire-label">{wire.net}</text>
                </g>
              );
            })}
            {pendingWireStart && (
              <circle cx={pendingWireStart[0]} cy={pendingWireStart[1]} r={6} fill="#ffcf6e" />
            )}
            {symbols.map((symbol) => (
              <g
                key={symbol.id}
                transform={`translate(${symbol.x}, ${symbol.y}) rotate(${symbol.rotationDeg})`}
                className={selectedId === symbol.id ? "symbol selected" : "symbol"}
                onClick={(event) => {
                  event.stopPropagation();
                  setSelectedId(symbol.id);
                }}
              >
                <line x1={-20} y1={0} x2={20} y2={0} strokeWidth={3} />
                <line x1={0} y1={-20} x2={0} y2={20} strokeWidth={3} />
                {CONTROLLED_SOURCE_KINDS.includes(symbol.kind) && (
                  <circle cx={0} cy={0} r={16} className="control-source-ring" />
                )}
                <text x={28} y={-8}>{symbol.reference}</text>
                <text x={28} y={14} className="symbol-value">{symbol.value}</text>
              </g>
            ))}
          </svg>
        </main>

        <aside className="panel inspector">
          <h2>Inspector</h2>
          {selected ? (
            <div className="form-grid">
              <label>Reference<input value={selected.reference} onChange={(e) => updateSelected("reference", e.target.value)} /></label>
              <label>Value<input value={selected.value} onChange={(e) => updateSelected("value", e.target.value)} /></label>
              <label>Fallback A<input value={selected.nodeA} onChange={(e) => updateSelected("nodeA", e.target.value)} /></label>
              <label>Fallback B<input value={selected.nodeB} onChange={(e) => updateSelected("nodeB", e.target.value)} /></label>
              {symbolUsesControlNodes(selected) && (
                <>
                  <label>Fallback Ctrl A<input value={selected.controlNodeA ?? ""} onChange={(e) => updateSelected("controlNodeA", e.target.value)} /></label>
                  <label>Fallback Ctrl B<input value={selected.controlNodeB ?? ""} onChange={(e) => updateSelected("controlNodeB", e.target.value)} /></label>
                </>
              )}
              {symbolUsesControlBranch(selected) && (
                <label>Control branch<input value={selected.controlNodeA ?? ""} onChange={(e) => updateSelected("controlNodeA", e.target.value)} /></label>
              )}
              <label>X<input type="number" value={selected.x} onChange={(e) => updateSelected("x", Number(e.target.value))} /></label>
              <label>Y<input type="number" value={selected.y} onChange={(e) => updateSelected("y", Number(e.target.value))} /></label>
              <label>Rotation<input type="number" value={selected.rotationDeg} onChange={(e) => updateSelected("rotationDeg", Number(e.target.value))} /></label>
              {(selected.kind === "vsource" || selected.kind === "isource") && (
                <div className="template-box">
                  <span>Source templates</span>
                  <div className="template-buttons">
                    {sourceTemplates.map((template) => (
                      <button key={template} onClick={() => applySourceTemplate(template)}>{template}</button>
                    ))}
                  </div>
                </div>
              )}
              {selectedInferred && (
                <div className="inferred-box">
                  <strong>Inferred nets</strong>
                  <span>A: {selectedInferred.primaryA}</span>
                  <span>B: {selectedInferred.primaryB}</span>
                  {selectedInferred.controlA ? <span>Ctrl A: {selectedInferred.controlA}</span> : null}
                  {selectedInferred.controlB ? <span>Ctrl B: {selectedInferred.controlB}</span> : null}
                </div>
              )}
            </div>
          ) : (
            <p>Select a symbol to edit its properties.</p>
          )}
          <h3>Project storage</h3>
          <p>
            Schematics are stored as vector SVG together with the inferred SPICE netlist. Fallback
            node names remain editable for pins that are not yet wired.
          </p>
        </aside>

        <section className="panel netlist-panel">
          <h2>Inferred netlist</h2>
          <textarea value={inferred.netlist} readOnly />
        </section>

        <section className="panel waveform-panel">
          <h2>Waveforms / Results</h2>
          <div className="status">{status}</div>
          {transient ? (
            <>
              <TraceSelector
                title="Transient traces"
                available={transientKeys}
                active={activeTransientKeys}
                onToggle={(key) => setActiveTransientKeys(toggleKey(activeTransientKeys, key))}
              />
              <WaveformChart title="Transient" x={transient.time} series={transient.traces} activeKeys={activeTransientKeys} />
            </>
          ) : null}
          {ac ? (
            <>
              <TraceSelector
                title="AC traces"
                available={acKeys}
                active={activeAcKeys}
                onToggle={(key) => setActiveAcKeys(toggleKey(activeAcKeys, key))}
              />
              <WaveformChart title="AC magnitude" x={ac.frequency} series={ac.magnitude} activeKeys={activeAcKeys} logX />
            </>
          ) : null}
          <pre>{simulation ? JSON.stringify(simulation, null, 2) : "Run a simulation to populate this pane."}</pre>
        </section>
      </div>
    </div>
  );
}

function inferSchematic(symbols: SymbolRecord[], wires: WireRecord[]): InferredSchematic {
  const dsu = new DisjointSet();
  const preferredLabels = new Map<string, string[]>();

  for (const wire of wires) {
    for (const point of wire.points) {
      dsu.make(pointKey(point));
    }
    for (let i = 1; i < wire.points.length; i += 1) {
      dsu.union(pointKey(wire.points[i - 1]), pointKey(wire.points[i]));
    }
  }

  for (const wire of wires) {
    const root = dsu.find(pointKey(wire.points[0] ?? [0, 0]));
    const current = preferredLabels.get(root) ?? [];
    current.push(wire.net);
    preferredLabels.set(root, current);
  }

  const groundRoots = new Set<string>();
  for (const symbol of symbols) {
    if (symbol.kind !== "ground") continue;
    const pins = terminalPoints(symbol);
    groundRoots.add(dsu.find(pointKey(pins.primaryA)));
  }

  const rootNames = new Map<string, string>();
  let autoIndex = 1;
  for (const key of dsu.keys()) {
    const root = dsu.find(key);
    if (rootNames.has(root)) continue;
    if (groundRoots.has(root)) {
      rootNames.set(root, "0");
      continue;
    }
    const labels = preferredLabels.get(root) ?? [];
    const preferred = labels.find((label) => label === "0" || !label.startsWith("net_"));
    rootNames.set(root, preferred ?? `n${autoIndex++}`);
  }

  const nodesBySymbol: Record<string, InferredSymbolNodes> = {};
  const lines = ["Generated by Spice Studio (inferred)"];

  for (const symbol of symbols) {
    if (symbol.kind === "ground") continue;
    const pins = terminalPoints(symbol);
    const primaryA = resolvePinNet(pins.primaryA, dsu, rootNames, symbol.nodeA);
    const primaryB = resolvePinNet(pins.primaryB, dsu, rootNames, symbol.nodeB);
    const controlA = pins.controlA
      ? resolvePinNet(pins.controlA, dsu, rootNames, symbol.controlNodeA ?? `${symbol.reference}_ctrlp`)
      : undefined;
    const controlB = pins.controlB
      ? resolvePinNet(pins.controlB, dsu, rootNames, symbol.controlNodeB ?? `${symbol.reference}_ctrln`)
      : undefined;

    nodesBySymbol[symbol.id] = { primaryA, primaryB, controlA, controlB };

    switch (symbol.kind) {
      case "resistor":
      case "capacitor":
      case "inductor":
      case "vsource":
      case "isource":
      case "diode":
        lines.push(`${symbol.reference} ${primaryA} ${primaryB} ${symbol.value}`);
        break;
      case "vccs":
      case "vcvs":
        lines.push(`${symbol.reference} ${primaryA} ${primaryB} ${controlA ?? symbol.controlNodeA ?? "ctrlp"} ${controlB ?? symbol.controlNodeB ?? "ctrln"} ${symbol.value}`);
        break;
      case "cccs":
      case "ccvs":
        lines.push(`${symbol.reference} ${primaryA} ${primaryB} ${symbol.controlNodeA ?? "V1"} ${symbol.value}`);
        break;
      default:
        break;
    }
  }

  lines.push(".op");
  lines.push(".ac dec 20 10 100000");
  lines.push(".tran 100u 10m");
  lines.push(".end");

  const labeledPoints = wires.map((wire) => {
    const root = dsu.find(pointKey(wire.points[0] ?? [0, 0]));
    const [x, y] = wire.points[0] ?? [0, 0];
    return { x, y, net: rootNames.get(root) ?? wire.net };
  });

  return {
    netlist: lines.join("\n"),
    nodesBySymbol,
    labeledPoints,
  };
}

function terminalPoints(symbol: SymbolRecord) {
  const rotation = ((symbol.rotationDeg % 360) + 360) % 360;
  let primaryA: [number, number];
  let primaryB: [number, number];
  if (rotation === 90 || rotation === 270) {
    primaryA = [symbol.x, symbol.y - 20];
    primaryB = [symbol.x, symbol.y + 20];
  } else {
    primaryA = [symbol.x - 20, symbol.y];
    primaryB = [symbol.x + 20, symbol.y];
  }

  let controlA: [number, number] | undefined;
  let controlB: [number, number] | undefined;
  if (symbol.kind === "vccs" || symbol.kind === "vcvs") {
    controlA = [symbol.x, symbol.y - 20];
    controlB = [symbol.x, symbol.y + 20];
  }

  return { primaryA, primaryB, controlA, controlB };
}

function resolvePinNet(
  point: [number, number],
  dsu: DisjointSet,
  rootNames: Map<string, string>,
  fallback: string,
) {
  const key = pointKey(point);
  if (!dsu.has(key)) {
    return fallback;
  }
  return rootNames.get(dsu.find(key)) ?? fallback;
}

class DisjointSet {
  private parent = new Map<string, string>();

  make(key: string) {
    if (!this.parent.has(key)) this.parent.set(key, key);
  }

  has(key: string) {
    return this.parent.has(key);
  }

  find(key: string): string {
    this.make(key);
    const parent = this.parent.get(key) ?? key;
    if (parent === key) return key;
    const root = this.find(parent);
    this.parent.set(key, root);
    return root;
  }

  union(a: string, b: string) {
    const ra = this.find(a);
    const rb = this.find(b);
    if (ra !== rb) this.parent.set(rb, ra);
  }

  keys() {
    return [...this.parent.keys()];
  }
}

function pointKey([x, y]: [number, number]) {
  return `${snap(x)},${snap(y)}`;
}

function averageBetween(values: number[], a: number, b: number) {
  const start = Math.min(a, b);
  const end = Math.max(a, b);
  const slice = values.slice(start, end + 1);
  if (slice.length === 0) return 0;
  return slice.reduce((sum, value) => sum + value, 0) / slice.length;
}

function findTransient(analyses: AnalysisResult[]): TransientResult | null {
  for (const analysis of analyses) {
    if ("Transient" in analysis) return analysis.Transient;
  }
  return null;
}

function findAc(analyses: AnalysisResult[]): AcResult | null {
  for (const analysis of analyses) {
    if ("Ac" in analysis) return analysis.Ac;
  }
  return null;
}

function toggleKey(active: string[], key: string) {
  return active.includes(key)
    ? active.filter((item) => item !== key)
    : [...active, key].slice(-MAX_ACTIVE_TRACES);
}

function TraceSelector({
  title,
  available,
  active,
  onToggle,
}: {
  title: string;
  available: string[];
  active: string[];
  onToggle: (key: string) => void;
}) {
  if (available.length === 0) return null;
  return (
    <div className="trace-selector">
      <h3>{title}</h3>
      <div className="trace-chips">
        {available.map((key) => (
          <button
            key={key}
            className={active.includes(key) ? "trace-chip active" : "trace-chip"}
            onClick={() => onToggle(key)}
          >
            {key}
          </button>
        ))}
      </div>
    </div>
  );
}

function WaveformChart({
  title,
  x,
  series,
  activeKeys,
  logX = false,
}: {
  title: string;
  x: number[];
  series: Record<string, number[]>;
  activeKeys: string[];
  logX?: boolean;
}) {
  const [cursorAIndex, setCursorAIndex] = useState<number | null>(null);
  const [cursorBIndex, setCursorBIndex] = useState<number | null>(null);
  const entries = Object.entries(series).filter(([name]) => activeKeys.includes(name)).slice(0, 6);
  if (x.length < 2 || entries.length === 0) return null;

  const width = 520;
  const height = 180;
  const pad = 28;
  const allValues = entries.flatMap(([, values]) => values);
  const yMin = Math.min(...allValues);
  const yMax = Math.max(...allValues);
  const xMinRaw = logX ? Math.max(Math.min(...x), 1e-12) : Math.min(...x);
  const xMaxRaw = Math.max(...x);
  const scaledX = x.map((value) => {
    if (logX) {
      const min = Math.log10(xMinRaw);
      const max = Math.log10(xMaxRaw);
      return pad + ((Math.log10(Math.max(value, 1e-12)) - min) / (max - min || 1)) * (width - pad * 2);
    }
    return pad + ((value - xMinRaw) / (xMaxRaw - xMinRaw || 1)) * (width - pad * 2);
  });

  const xScale = (value: number) => {
    if (logX) {
      const min = Math.log10(xMinRaw);
      const max = Math.log10(xMaxRaw);
      return pad + ((Math.log10(Math.max(value, 1e-12)) - min) / (max - min || 1)) * (width - pad * 2);
    }
    return pad + ((value - xMinRaw) / (xMaxRaw - xMinRaw || 1)) * (width - pad * 2);
  };
  const yScale = (value: number) =>
    height - pad - ((value - yMin) / (yMax - yMin || 1)) * (height - pad * 2);

  const cursorAX = cursorAIndex !== null ? xScale(x[cursorAIndex]) : null;
  const cursorBX = cursorBIndex !== null ? xScale(x[cursorBIndex]) : null;
  const deltaX =
    cursorAIndex !== null && cursorBIndex !== null ? Math.abs(x[cursorBIndex] - x[cursorAIndex]) : null;
  const deltaFrequency = deltaX && deltaX > 0 ? 1 / deltaX : null;

  function nearestIndex(svgX: number) {
    let bestIndex = 0;
    let bestDistance = Number.POSITIVE_INFINITY;
    scaledX.forEach((position, index) => {
      const distance = Math.abs(position - svgX);
      if (distance < bestDistance) {
        bestDistance = distance;
        bestIndex = index;
      }
    });
    return bestIndex;
  }

  return (
    <div className="chart-block">
      <div className="chart-header">
        <h3>{title}</h3>
        <button className="trace-chip" onClick={() => { setCursorAIndex(null); setCursorBIndex(null); }}>
          Clear cursors
        </button>
      </div>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="wave-chart"
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          const svgX = ((event.clientX - rect.left) / rect.width) * width;
          const bestIndex = nearestIndex(svgX);
          if (event.shiftKey) {
            setCursorBIndex(bestIndex);
          } else {
            setCursorAIndex(bestIndex);
          }
        }}
      >
        <rect x={0} y={0} width={width} height={height} fill="#0d1218" rx={10} />
        <line x1={pad} y1={height - pad} x2={width - pad} y2={height - pad} stroke="#41546b" />
        <line x1={pad} y1={pad} x2={pad} y2={height - pad} stroke="#41546b" />
        {entries.map(([name, values], index) => {
          const points = values
            .map((value, idx) => `${scaledX[idx] ?? scaledX[scaledX.length - 1]},${yScale(value)}`)
            .join(" ");
          return <polyline key={name} points={points} fill="none" stroke={CHART_COLORS[index % CHART_COLORS.length]} strokeWidth={2} />;
        })}
        {cursorAX !== null ? (
          <line x1={cursorAX} y1={pad} x2={cursorAX} y2={height - pad} className="cursor-line cursor-a-line" />
        ) : null}
        {cursorBX !== null ? (
          <line x1={cursorBX} y1={pad} x2={cursorBX} y2={height - pad} className="cursor-line cursor-b-line" />
        ) : null}
      </svg>
      <div className="chart-help">Click to place cursor A. Shift+click to place cursor B.</div>
      {cursorAIndex !== null ? (
        <div className="cursor-readout">
          <strong>A: x = {x[cursorAIndex].toPrecision(5)}</strong>
          {cursorBIndex !== null ? <strong>B: x = {x[cursorBIndex].toPrecision(5)}</strong> : null}
          {deltaX !== null ? <strong>Δx = {deltaX.toPrecision(5)}</strong> : null}
          {deltaFrequency !== null ? <strong>1/Δx = {deltaFrequency.toPrecision(5)}</strong> : null}
          {entries.map(([name, values], index) => (
            <span key={name} className="cursor-series" style={{ color: CHART_COLORS[index % CHART_COLORS.length] }}>
              <span>{name} A: {values[cursorAIndex]?.toPrecision(5)}</span>
              {cursorBIndex !== null ? <span>{name} B: {values[cursorBIndex]?.toPrecision(5)}</span> : null}
              {cursorBIndex !== null ? (
                <span>Δ{name}: {(values[cursorBIndex] - values[cursorAIndex]).toPrecision(5)}</span>
              ) : null}
              {cursorBIndex !== null && deltaX && deltaX > 0 ? (
                <span>slope({name}): {((values[cursorBIndex] - values[cursorAIndex]) / deltaX).toPrecision(5)}</span>
              ) : null}
              {cursorBIndex !== null ? (
                <span>avg({name}): {averageBetween(values, cursorAIndex, cursorBIndex).toPrecision(5)}</span>
              ) : null}
            </span>
          ))}
        </div>
      ) : null}
        <div className="legend">
          {entries.map(([name], index) => (
          <span key={name} style={{ color: CHART_COLORS[index % CHART_COLORS.length] }}>{name}</span>
          ))}
        </div>
    </div>
  );
}

export default App;
