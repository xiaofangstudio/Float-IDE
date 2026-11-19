import { readFileSync } from 'node:fs';
import { analyze } from '../src/lang/analyze';
const src=readFileSync('src-tauri/float-lang/bootstrap/colang.co','utf8');
const a=analyze('colang.co',src);
for(const d of a.diagnostics){ if(d.severity==='error'||d.code==='lex'||d.code.startsWith('undefined')) console.log(d.severity,d.code,d.span.line+':'+d.span.col,d.message); }