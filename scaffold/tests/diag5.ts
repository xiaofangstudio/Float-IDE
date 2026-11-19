import { readFileSync } from "node:fs";
import { parseProgram } from "../src/lang/parse";
const lines = readFileSync("src-tauri/float-lang/bootstrap/colang.co","utf8").split("\n");
const one = lines[3209];
const p = parseProgram("t.co", one);
console.log("diag", JSON.stringify(p.diagnostics));
console.log("col82>", JSON.stringify(one.slice(78,92)));
const p2 = parseProgram("t.co", lines.slice(0,3210).join("\n"));
console.log("diag2", JSON.stringify(p2.diagnostics.filter(d=>d.span && d.span.line>=3205)));