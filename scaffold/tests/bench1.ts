import { readFileSync } from "node:fs";
import { analyze } from "../src/lang/analyze";
import { parseProgram } from "../src/lang/parse";
const src=readFileSync("src-tauri/float-lang/bootstrap/colang.co","utf8");
for (const opt of [{},{symbolsOnly:true}]) {
  const t=performance.now(); analyze("colang.co",src,undefined,opt as any); console.log("analyze",JSON.stringify(opt),(performance.now()-t).toFixed(1)+"ms");
}
const t2=performance.now(); parseProgram("colang.co",src); console.log("parse",(performance.now()-t2).toFixed(1)+"ms");