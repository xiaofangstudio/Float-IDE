import { readFileSync } from "node:fs";
import { tokenizeLine } from "../src/lang/token";
const lines = readFileSync("src-tauri/float-lang/bootstrap/colang.co","utf8").split("\n");
const s = lines[3209];
try { const t = tokenizeLine(s, 3210, 1); console.log(t.map(x=>x.k+":"+JSON.stringify(x.v).slice(0,40)).join(" | ")); }
catch(e){ console.log("ERR", e.message, e.col); console.log(JSON.stringify(s.slice(e.col-1, e.col+30))); }