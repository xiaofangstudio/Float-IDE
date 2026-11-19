import { readFileSync } from "node:fs";
import { codePart, tokenizeLine } from "../src/lang/token";
const lines = readFileSync("src-tauri/float-lang/bootstrap/colang.co","utf8").split("\n");
function prep(n:number){ const t=lines[n-1].replace(/\t/g,"    ").trim(); const r=codePart(t,n); return r.code.replace(/;+\s*$/,"").trim(); }
let merged = prep(3210);
for (const n of [3211,3212,3213]) {
  merged = merged + " " + prep(n);
  try { tokenizeLine(merged, 3210, 1); console.log(n, "ok len", merged.length); }
  catch(e){ console.log(n, "ERR", e.message, e.col, JSON.stringify(merged.slice(Math.max(0,e.col-15), e.col+25))); }
}