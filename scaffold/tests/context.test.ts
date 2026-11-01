/* 光标语义（context.ts）的守门测试。
 *
 * 这是编辑器所有智能功能的唯一判断入口：补全、悬停、参数提示、重命名都问它。
 * 所以这里用一张"形状矩阵"压住——加新语法时，只要这里过了，
 * 上游功能不用各写一条正则也能跟着对。
 */

import { contextAt } from '../src/lang/context';

let pass = 0;
let fail = 0;

function eq(name: string, got: unknown, want: unknown): void {
  const a = JSON.stringify(got);
  const b = JSON.stringify(want);
  if (a === b) { pass++; return; }
  fail++;
  console.log(`✗ ${name}\n  期望 ${b}\n  实际 ${a}`);
}

/** 用 `▮` 标出光标位置，省得手数第几列 */
function at(srcWithCaret: string) {
  const idx = srcWithCaret.indexOf('▮');
  const src = srcWithCaret.replace('▮', '');
  const before = srcWithCaret.slice(0, idx);
  const line = before.split('\n').length;
  const col = idx - before.lastIndexOf('\n');
  return contextAt(src, line, col);
}

/* ── 注释 ── */
eq('整行注释', at('-- 说▮明'), { kind: 'comment' });
eq('行尾注释', at('int a = 1 -- 说▮明'), { kind: 'comment' });
eq('空注释行', at('--▮'), { kind: 'comment' });

/* ── 字符串 ── */
eq('字符串里', at('print("ab▮c")'), { kind: 'string' });
eq('字符串里（中文）', at('print("你好▮")'), { kind: 'string' });
eq('字符串外', at('print("abc"▮)'), { kind: 'code' });

/* ── import 路径 ── */
eq('import 路径里', at('import "li▮b.cw"'), { kind: 'importPath', typed: 'li' });
eq('import 路径里（空）', at('import "▮"'), { kind: 'importPath', typed: '' });

/* ── 起名字：声明 / 函数 / 类 / 循环 / 参数 / 字段 ── */
eq('声明后面光标', at('int ▮'), { kind: 'naming', what: 'decl' });
eq('声明打到一半', at('int hp▮'), { kind: 'naming', what: 'decl' });
eq('声明完整后', at('int hp = 1▮'), { kind: 'code' });
eq('字典声明', at('dict ▮'), { kind: 'naming', what: 'decl' });
eq('函数名', at('fn ▮'), { kind: 'naming', what: 'func' });
eq('函数名打到一半', at('fn 打招▮呼(a)'), { kind: 'naming', what: 'func' });
eq('类名', at('class ▮'), { kind: 'naming', what: 'class' });
eq('循环变量', at('for i▮ in range(3)'), { kind: 'naming', what: 'for' });
eq('参数名', at('fn f(▮a, b)'), { kind: 'naming', what: 'param' });
eq('第二个参数', at('fn f(a, ▮b)'), { kind: 'naming', what: 'param' });
eq('类字段', at('class A\n    str 名▮字 = ""'), { kind: 'naming', what: 'decl' });
eq('for 里的表达式不是起名字', at('for i in range(▮3)'), { kind: 'code' });

/* ── 成员：对象. / self. / 下标. ── */
const CLS = `class Person
    str 名字 = ""

    fn hi(a)
        return a

Person p = new Person()
str s = "x"
list xs = [1]
`;
function memberAt(body: string) {
  const src = CLS + body;
  const idx = src.indexOf('▮');
  const clean = src.replace('▮', '');
  const before = src.slice(0, idx);
  const line = before.split('\n').length;
  const col = idx - before.lastIndexOf('\n');
  return contextAt(clean, line, col);
}
eq('类实例成员', memberAt('p.▮'), { kind: 'member', object: 'p', type: 'var', cls: 'Person' });
eq('类实例成员打到一半', memberAt('p.名▮'), { kind: 'member', object: 'p', type: 'var', cls: 'Person' });
eq('字符串变量成员', memberAt('s.▮'), { kind: 'member', object: 's', type: 'str', cls: null });
eq('列表变量成员', memberAt('xs.▮'), { kind: 'member', object: 'xs', type: 'list', cls: null });

/* self. 要按 AST 找到"当前所在类"，不能靠缩进猜 */
const SELF = `class Person
    str 名字 = ""

    fn hi(a)
        return self.▮
`;
{
  const idx = SELF.indexOf('▮');
  const clean = SELF.replace('▮', '');
  const before = SELF.slice(0, idx);
  const line = before.split('\n').length;
  const col = idx - before.lastIndexOf('\n');
  eq('self. 在方法里', contextAt(clean, line, col),
    { kind: 'member', object: 'self', type: 'Person', cls: 'Person' });
}

/* ── 普通代码位置 ── */
eq('参数位置', at('print(▮)'), { kind: 'code' });
eq('表达式里', at('int a = 1 + ▮'), { kind: 'code' });
eq('赋值右边', at('x = ▮'), { kind: 'code' });

/* ── 残缺代码也要给出正确答案（解析器的错误恢复撑着） ── */
eq('残缺声明后仍能识别变量', (() => {
  const src = 'int hp = \nprint(h▮p)\n';
  const idx = src.indexOf('▮');
  const clean = src.replace('▮', '');
  const before = src.slice(0, idx);
  const line = before.split('\n').length;
  const col = idx - before.lastIndexOf('\n');
  return contextAt(clean, line, col).kind;
})(), 'code');

console.log(`\n通过 ${pass} 项，失败 ${fail} 项`);
if (fail) process.exit(1);
console.log('全部通过');
