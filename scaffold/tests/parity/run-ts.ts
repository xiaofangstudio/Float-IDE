/* 对拍用：把 cases.txt 里每段脚本用 TS 兼容引擎跑一遍，结果写 ts.json */
import { readFileSync, writeFileSync } from 'node:fs';
import { run } from '../harness';

function cases(path: string): { name: string; src: string }[] {
  const text = readFileSync(path, 'utf8');
  const out: { name: string; src: string }[] = [];
  let cur: { name: string; lines: string[] } | null = null;
  for (const line of text.split('\n')) {
    if (line.startsWith('### ')) {
      if (cur) out.push({ name: cur.name, src: cur.lines.join('\n') });
      cur = { name: line.slice(4).trim(), lines: [] };
    } else if (cur) {
      cur.lines.push(line);
    }
  }
  if (cur) out.push({ name: cur.name, src: cur.lines.join('\n') });
  return out;
}

const results: Record<string, { out: string[]; err: string[] }> = {};
for (const c of cases('tests/parity/' + (process.env.CASES || 'cases.txt') + '')) {
  const r = await run(c.src + '\n', { timeoutMs: 3000 });
  results[c.name] = { out: r.out, err: r.err };
}
writeFileSync('tests/parity/ts.json', JSON.stringify(results, null, 2));
console.log('TS 侧完成', Object.keys(results).length, '个用例');
