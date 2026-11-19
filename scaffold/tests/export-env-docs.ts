/* 把 IDE 里的教程内容导出到 Copper 环境（copper_env/docs/）。
 *
 * 目的：文档归环境所有。环境更新一次，教程、关于页跟着一起换，
 * IDE 自己不再攥着一份会过期的副本。
 * 用法（scaffold 目录下）：node tests/.export-docs.mjs
 */
import { writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CHAPTERS, TUTORIAL_STATS } from '../src/docs/tutorial';

const HERE = dirname(fileURLToPath(import.meta.url));
const ENV = join(HERE, '..', 'src-tauri', '..', '..', 'copper_env');
const DOCS = join(ENV, 'docs');

if (!existsSync(DOCS)) mkdirSync(DOCS, { recursive: true });
const payload = { provided_by: 'copper-env', format: 'float-tutorial/1', stats: TUTORIAL_STATS, chapters: CHAPTERS };
writeFileSync(join(DOCS, 'tutorial.json'), JSON.stringify(payload), 'utf8');
console.log(`tutorial.json → ${(JSON.stringify(payload).length / 1024).toFixed(1)} KB，${CHAPTERS.length} 章`);

const about = `# 关于 Copper

Float IDE 里的这一页由 **Copper 环境**提供（\`docs/about-copper.md\`），
环境更新时这里会跟着换，IDE 不再自带一份会过期的说明。

## 语言

Copper，中文是「铜」——导电又导热，是流动性最好的金属之一，也是这门语言想要的样子：
**流动**，一次编写，处处能跑（工程、插件、发布出去的应用，同一套源码）；
也流动在纸面上——简单到随便给谁看，都能一眼读懂、随时上手改。
它的原名是 CW Script，从 1.6.0 起正式改名 Copper，旧 .cw 文件全部兼容。
吉祥物是廿七，一只三脚橘猫——所以第 27 行报错时会说「喵～第 27 行瘸了…」。

## 解释器叫什么

**Copper 的解释器不叫 copper，叫 \`colang\`。**
环境里的编译器统一按版本命名：

\`\`\`
colang 1.9.2.exe
colang 2.0.0.exe
\`\`\`

- 格式固定为 \`colang <大>.<中>.<小>.exe\`，小版本占满三位（1.9.002 也会被认成 1.9.2）。
- 目录里有多份时，**优先用版本号最新的那一份**；版本号相同再比文件修改时间。
- 找不到带版本号的文件时，才退回老的 \`copper.exe\`（历史环境的兼容名）。

## 环境里有什么

| 路径 | 作用 |
| --- | --- |
| \`bin/colang <版本>.exe\` | AOT 编译器：把 .co 编成原生 exe |
| \`bin/<平台>/colang <版本>.exe\` | 多平台布局（windows-x64 / linux-x64） |
| \`bin/colang-self <版本>.exe\` | 自举版编译器 |
| \`lib/colang_rt.rs\` | 运行时前奏，发射 Rust 代码时拼在最前面 |
| \`docs/\` | 教程与这份说明（IDE 直接读这里） |
| \`copper.json\` | 环境清单（版本、平台） |
`;
writeFileSync(join(DOCS, 'about-copper.md'), about, 'utf8');
console.log(`about-copper.md → ${(about.length / 1024).toFixed(1)} KB`);
