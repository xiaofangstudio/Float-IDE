/* 体检一个 .co 文件：打印解析/语义诊断，用来定位"未定义"与"字符串没结束"这类假警报。
 * 用法：node tests/.diag-file.mjs <文件路径>
 */
import { readFileSync } from 'node:fs';
import { parseProgram } from '../src/lang/parse';
import { analyze } from '../src/lang/analyze';
import { builtinList } from '../src/lang/builtins';

const file = process.argv[2];
if (!file) {
  console.error('用法: node tests/.diag-file.mjs <file.co>');
  process.exit(2);
}
const src = readFileSync(file, 'utf8');
const program = parseProgram(file.split(/[\\/]/).pop() || 'x.co', src);
const a = analyze(file.split(/[\\/]/).pop() || 'x.co', src);
console.log(`lines=${src.split('\n').length} parseDiag=${program.diagnostics.length} allDiag=${a.diagnostics.length}`);
const byCode = new Map<string, number>();
for (const d of a.diagnostics) byCode.set(d.code, (byCode.get(d.code) ?? 0) + 1);
console.log('按类别:', [...byCode].map(([k, v]) => `${k}=${v}`).join('  '));
for (const d of a.diagnostics.slice(0, 12)) {
  console.log(`${d.severity} ${d.code} ${d.span.line}:${d.span.col} ${d.message}`);
}
const names = new Set(builtinList().map((b) => b.name));
console.log('charfrom 在内建里?', names.has('charfrom'));
