import { readFileSync } from "node:fs";
import { codePart, tokenizeLine } from "../src/lang/token";
const lines = readFileSync("src-tauri/float-lang/bootstrap/colang.co","utf8").split("\n");
const raw = lines[3209];
const trimmed = raw.replace(/\t/g,"    ").trim();
const { code, comment } = codePart(trimmed, 3210);
console.log("code len", code.length, "comment?", comment);
try { tokenizeLine(code.replace(/;+\s*$/,"").trim(), 3210, 1); console.log("ok"); }
catch(e){ console.log("ERR", e.message, e.col, JSON.stringify(code.slice(Math.max(0,e.col-10), e.col+20))); }