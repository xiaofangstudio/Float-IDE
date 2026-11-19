import { readFileSync } from "node:fs";
import { analyze } from "../src/lang/analyze";
const src=readFileSync("src-tauri/float-lang/bootstrap/colang.co","utf8");
const a=analyze("colang.co",src);
const by=new Map();
for(const d of a.diagnostics) by.set(d.severity+"/"+d.code,(by.get(d.severity+"/"+d.code)||0)+1);
console.log("colang.co:",[...by].map(([k,v])=>k+"="+v).join(" "));
// 没有 import 的文件引用别的文件里定义的名字：工程总表兜住就不报
const other=`fn helperInOtherFile(x)\n    return x\nlet sharedList = [1]`;
const main=`fn run()\n    helperInOtherFile(1)\n    add(sharedList, 2)`;
const strip=(s:string)=>new Set<string>();
import { toplevelNames } from "../src/lang/sema";
import { parseProgram } from "../src/lang/parse";
const names=new Set<string>(); for(const n of toplevelNames(parseProgram("o.co",other).body)) names.add(n);
const a2=analyze("main.co",main,names);
const a3=analyze("main.co",main);
console.log("带工程总表 undefined 条数:",a2.diagnostics.filter(d=>d.code.startsWith("undefined")).length);
console.log("不带总表 undefined 条数（会有，作为对照）:",a3.diagnostics.filter(d=>d.code.startsWith("undefined")).length);