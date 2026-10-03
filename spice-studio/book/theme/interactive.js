window.addEventListener("DOMContentLoaded", () => {
  const button = document.getElementById("solve-divider");
  if (!button) return;
  button.addEventListener("click", () => {
    const r1 = Number(document.getElementById("r1")?.value ?? 0);
    const r2 = Number(document.getElementById("r2")?.value ?? 0);
    const vin = Number(document.getElementById("vin")?.value ?? 0);
    const output = document.getElementById("divider-output");
    if (!output || r1 <= 0 || r2 <= 0) return;
    const vout = vin * r2 / (r1 + r2);
    output.textContent = `Predicted midpoint voltage: ${vout.toFixed(6)} V`;
  });
});
