use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicProject {
    pub name: String,
    pub spice_path: String,
    pub svg_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicDocument {
    pub width: f64,
    pub height: f64,
    pub grid: f64,
    pub symbols: Vec<SchematicSymbol>,
    pub wires: Vec<SchematicWire>,
    pub text: Vec<SchematicText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicSymbol {
    pub id: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub rotation_deg: f64,
    pub reference: String,
    pub value: String,
    pub node_a: String,
    pub node_b: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicWire {
    pub id: String,
    pub points: Vec<(f64, f64)>,
    pub net: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicText {
    pub x: f64,
    pub y: f64,
    pub content: String,
}

impl Default for SchematicDocument {
    fn default() -> Self {
        Self {
            width: 2400.0,
            height: 1600.0,
            grid: 20.0,
            symbols: Vec::new(),
            wires: Vec::new(),
            text: Vec::new(),
        }
    }
}

impl SchematicDocument {
    /// Render the lightweight schematic model into an SVG document.
    pub fn to_svg(&self) -> String {
        let mut svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" data-grid="{}">"##,
            self.width, self.height, self.width, self.height, self.grid
        );
        svg.push_str(r##"<rect width="100%" height="100%" fill="#14181f" />"##);
        for wire in &self.wires {
            let points = wire
                .points
                .iter()
                .map(|(x, y)| format!("{x},{y}"))
                .collect::<Vec<_>>()
                .join(" ");
            svg.push_str(&format!(
                r##"<polyline data-id="{}" data-net="{}" fill="none" stroke="#91d7ff" stroke-width="3" points="{}" />"##,
                wire.id, wire.net, points
            ));
        }
        for symbol in &self.symbols {
            svg.push_str(&format!(
                r##"<g data-id="{}" data-kind="{}" transform="translate({}, {}) rotate({})">"##,
                symbol.id, symbol.kind, symbol.x, symbol.y, symbol.rotation_deg
            ));
            svg.push_str(r##"<line x1="-20" y1="0" x2="20" y2="0" stroke="#d6deff" stroke-width="3" />"##);
            svg.push_str(r##"<line x1="0" y1="-20" x2="0" y2="20" stroke="#d6deff" stroke-width="3" />"##);
            svg.push_str(&format!(
                r##"<text x="28" y="-8" fill="#f8fafc" font-size="18">{}</text>"##,
                symbol.reference
            ));
            svg.push_str(&format!(
                r##"<text x="28" y="14" fill="#89ffa0" font-size="16">{}</text>"##,
                symbol.value
            ));
            svg.push_str("</g>");
        }
        for text in &self.text {
            svg.push_str(&format!(
                r##"<text x="{}" y="{}" fill="#f8fafc" font-size="18">{}</text>"##,
                text.x, text.y, text.content
            ));
        }
        svg.push_str("</svg>");
        svg
    }
}
