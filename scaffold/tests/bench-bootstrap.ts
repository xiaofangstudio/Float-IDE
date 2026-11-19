import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { analyze } from "../src/lang/analyze";
const dir = "src-tauri/float-lang/bootstrap";
for (const f of readdirSync(dir).filter(n=>n.endsWith(".co"))) {
  const p = join(dir, f);
  const src = readFileSync(p, "utf8");
  const t0 = Date.now();
  const a = analyze(f, src, undefined, { symbolsOnly: true });
  const t1 = Date.now();
  const full = analyze(f, src);
  const t2 = Date.now();
  console.log(`${f.padEnd(22)} ${String(src.length).padStart(7)}B  symbols ${String(t1-t0).padStart(5)}ms  full ${String(t2-t1).padStart(5)}ms  diag=${full.diagnostics.length}`);
}