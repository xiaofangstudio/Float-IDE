import { readFileSync } from "node:fs";
import { analyze } from "../src/lang/analyze";
const src=readFileSync("src-tauri/float-lang/bootstrap/colang.co","utf8");
const a=analyze("colang.co",src);
const by=new Map();
for(const d of a.diagnostics) by.set(d.severity+"/"+d.code,(by.get(d.severity+"/"+d.code)||0)+1);
console.log([...by].map(([k,v])=>k+"="+v).join("  "));
for(const d of a.diagnostics){ if(d.code.startsWith("undefined")||d.severity==="error") console.log(d.severity,d.code,d.span.line+":"+d.span.col,d.message); }