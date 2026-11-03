/* 教程代码的守门测试：**每一段 code 都必须能跑通，而且 out 必须是真输出**。
 *
 * 教程里写错的输出比没有输出更糟——用户照着敲一遍，发现对不上，
 * 从此不再相信文档。所以这里逐段真跑一遍，一个字一个字比。
 *
 * 跳过的情况：
 *  - 标了 `ask: true` 的（要人工输入，跑不通是正常的）
 *  - 需要宿主的（文件/网络/时间戳类）——这类在 out 里写了"桌面版"或者由代码自己决定
 */

import { run } from './harness';
import { CHAPTERS } from '../src/docs/tutorial';

let pass = 0;
let fail = 0;
const skipped: string[] = [];

function norm(s: string): string {
  return s.replace(/\r\n/g, '\n').replace(/\s+$/g, '');
}

/* 需要宿主能力（桌面版才有）的示例：浏览器/Node 里跑不了，跳过并记下来 */
const NEEDS_HOST = /(readfile|writefile|appendfile|hasfile|fetch\(|post\(|net\.|os\.(readfile|writefile|appendfile|hasfile|listdir|makedir|removedir|removefile|renamefile|copyfile|mtime|filesize|exists|isfile|isdir|getenv|setenv|runcmd|pid|args|tempdir|homedir|chdir|cwd|exit)|ui\.(menu|input))/;

async function main(): Promise<void> {
  for (const ch of CHAPTERS) {
    for (const sec of ch.sections) {
      for (const [i, sample] of (sec.codes ?? []).entries()) {
        const where = `${ch.title} / ${sec.title}${(sec.codes?.length ?? 0) > 1 ? ` #${i + 1}` : ''}`;
        if (sample.ask) { skipped.push(`${where}（要人工输入）`); continue; }
        if (NEEDS_HOST.test(sample.code)) { skipped.push(`${where}（要桌面宿主）`); continue; }
        if (sample.out === undefined) continue;

        const r = await run(sample.code.endsWith('\n') ? sample.code : sample.code + '\n', { timeoutMs: 10000 });
        const want = norm(sample.out);

        /* 教程里有一部分示例是**故意写错**的（讲坑），out 写成 `(报错) 具体原因`。
         * 这类要求：确实报错，而且报的正是写了的那句话。 */
        if (want.startsWith('(报错)')) {
          const needle = want.slice('(报错)'.length).trim();
          const errText = norm(r.err.join('\n'));
          if (!r.err.length) {
            fail++;
            console.log(`✗ ${where}：本该报错，却跑成功了（输出 ${JSON.stringify(norm(r.out.join('\n')))}）`);
          } else if (errText.includes(needle)) {
            pass++;
          } else {
            fail++;
            console.log(`✗ ${where}：报错内容对不上\n  期望包含：${JSON.stringify(needle)}\n  实际：${JSON.stringify(errText)}`);
          }
          continue;
        }

        if (r.err.length) {
          fail++;
          console.log(`✗ ${where}：跑出错了\n  ${r.err.join('\n  ')}`);
          continue;
        }
        const got = norm(r.out.join('\n'));
        if (got === want) { pass++; continue; }
        fail++;
        console.log(`✗ ${where}：输出对不上\n  期望：${JSON.stringify(want)}\n  实际：${JSON.stringify(got)}`);
      }
    }
  }

  if (skipped.length) console.log(`\n跳过 ${skipped.length} 段（需要交互或桌面宿主）：\n  ${skipped.join('\n  ')}`);
  console.log(`\n通过 ${pass} 项，失败 ${fail} 项`);
  if (fail) process.exit(1);
  console.log('全部通过');
}

void main();
