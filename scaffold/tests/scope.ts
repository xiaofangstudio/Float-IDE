import { analyze } from "../src/lang/analyze";
import { toplevelNames } from "../src/lang/sema";
import { parseProgram } from "../src/lang/parse";
// c.co 定义 deepfn / 列表 / 字典
const c = `fn deepfn(x)\n    return x + 1\nlet sharedList = [1, 2]\nlet sharedDict = {"a": 1}`;
// b.co import c.co，再定义一个自己的
const b = `import "c.co"\nfn midfn()\n    return deepfn(3)`;
// a.co 只 import b.co，用到 c.co 里的东西（跨两级）
const a = `import "b.co"\nfn main()\n    let v = deepfn(1)\n    print(v)\n    add(sharedList, 4)`;
const names = new Set<string>();
for (const s of [c, b]) for (const n of toplevelNames(parseProgram("dep.co", s).body)) names.add(n);
const a1 = analyze("a.co", a, names);
console.log("带总表:", a1.diagnostics.filter(d=>d.code.startsWith("undefined")).length, "条未定义");
const a2 = analyze("a.co", a);
console.log("没总表（依赖没解析完）:", a2.diagnostics.length, "条诊断（应为 0，宁可沉默也不误报）");