/* 类型检查的守门测试：类型标注必须"是真的"，不能只是写着好看。
 * 覆盖：变量声明、赋值、函数返回值、参数默认值、类字段、各种合法情况不误报。 */

import { analyze } from '../src/lang/analyze';

let pass = 0;
let fail = 0;
const ok = (name: string, cond: boolean, extra = ''): void => {
  if (cond) { pass++; return; }
  fail++;
  console.log(`✗ ${name}${extra ? '  — ' + extra : ''}`);
};

function diags(src: string) {
  return analyze('main.cw', src).diagnostics;
}
function hasError(src: string) {
  return diags(src).some((d) => d.severity === 'error');
}
function msgOf(src: string): string {
  return diags(src).map((d) => d.message).join(' | ');
}

/* ── 该报错的一定要报 ── */
ok('int 装字符串要报错', hasError('int hp = "abc"'), msgOf('int hp = "abc"'));
ok('报错要说清类型', /int/.test(msgOf('int hp = "abc"')) && /str|字符串/.test(msgOf('int hp = "abc"')), msgOf('int hp = "abc"'));
ok('str 装数字要报错', hasError('str s = 123'), msgOf('str s = 123'));
ok('bool 装字符串要报错', hasError('bool b = "yes"'), msgOf('bool b = "yes"'));
ok('float 装字符串要报错', hasError('float f = "1.5"'), msgOf('float f = "1.5"'));
ok('int 装小数要报错', hasError('int n = 1.5'), msgOf('int n = 1.5'));
ok('返回值类型不符要报错', hasError('int f()\n    return "x"'), msgOf('int f()\n    return "x"'));
ok('void 函数写了 return 值要报错', hasError('void f()\n    return 1'), msgOf('void f()\n    return 1'));
ok('参数默认值类型不符要报错', hasError('fn f(int a = "x")\n    return a'), msgOf('fn f(int a = "x")\n    return a'));
ok('列表装进 int 要报错', hasError('int n = [1]'), msgOf('int n = [1]'));

/* ── 合法的绝不能误报 ── */
ok('int 装整数 OK', !hasError('int a = 42'));
ok('int 运算 OK', !hasError('int a = 1 + 2 * 3'));
ok('float 装整数 OK（放宽）', !hasError('float f = 1'));
ok('float 装小数 OK', !hasError('float f = 1.5'));
ok('字符串拼接 OK', !hasError('str s = "a" + 1'));
ok('插值 OK', !hasError('int n = 3\nstr s = "值是 {n}"'));
ok('bool 比较 OK', !hasError('bool b = 1 < 2'));
ok('列表 OK', !hasError('list xs = [1, 2]'));
ok('字典 OK', !hasError('dict d = {"a": 1}'));
ok('var 不限定类型', !hasError('var v = "x"\nv = 1'));
ok('转换函数 OK', !hasError('int n = int("42")'));
ok('函数返回值 OK', !hasError('int f()\n    return 1 + 2'));
ok('参数类型 OK', !hasError('fn f(int a)\n    return a + 1'));
ok('类实例声明 OK', !hasError('class P\n    int n = 1\n\nP p = new P()'));
ok('类字段类型 OK', !hasError('class P\n    int n = 5\n    str s = "x"'));
ok('类字段类型错要报错', hasError('class P\n    int n = "x"'));

/* ── 赋值：不同宽度只提醒，不拦运行 ── */
{
  const d = diags('int n = 1\nn = "x"');
  ok('赋值类型不符给警告而不是错误',
    d.some((x) => x.severity === 'warning' && /int/.test(x.message)) && !d.some((x) => x.severity === 'error'),
    d.map((x) => `${x.severity}:${x.message}`).join(' | '));
}

/* ── 推断不出来就别说话 ── */
ok('未知来源不误报', !hasError('int n = f()'), msgOf('int n = f()'));
ok('空值是 null 不算错', !hasError('str s = null'));

/* ── 7 种类型都能用，不是只有 int ── */
for (const t of ['int', 'float', 'str', 'bool', 'list', 'dict', 'var']) {
  ok(`类型 ${t} 能声明`, !diags(`${t} x`).some((d) => d.severity === 'error'), t);
}

console.log(`\n通过 ${pass} 项，失败 ${fail} 项`);
if (fail) process.exit(1);
console.log('全部通过');
