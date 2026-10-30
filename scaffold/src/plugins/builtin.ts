/* 内置示例插件：随应用一起走，首次运行写到插件目录。
 *
 * 三个例子分别演示插件生态的三种形态：
 *   1. 纯声明 —— 只写 plugin.json，命令 / 片段 / 补全全在里面（谁都改得动）
 *   2. 脚本 —— main.js 用注入的 float API 干活（需要逻辑时）
 *   3. 工具集成 —— 往工具菜单加项，把日常工作流一键化
 */

export type BuiltinPlugin = {
  id: string;
  manifest: Record<string, unknown>;
  files: Record<string, string>;
};

const starter = `{
  "id": "float.starter-pack",
  "name": "起步片段包",
  "version": "1.0.0",
  "publisher": "小方工作室",
  "description": "把最常用的 Float Script 写法做成片段与命令，输入前缀就能展开。纯声明式，不需要任何脚本。",
  "tags": ["snippet", "productivity"],
  "contributes": {
    "snippets": [
      { "prefix": "fori", "body": "for i in range(\${1:n})\\n    \${2:print(i)}", "description": "计数循环" },
      { "prefix": "forin", "body": "for \${1:item} in \${2:列表}\\n    \${3:print(item)}", "description": "遍历集合" },
      { "prefix": "iff", "body": "if (\${1:条件})\\n    \${2:print(\\"成立\\")}\\nelse\\n    \${3:print(\\"不成立\\")}", "description": "条件分支" },
      { "prefix": "fn", "body": "function \${1:名字}(\${2:a})\\n    return \${3:a}", "description": "定义函数" },
      { "prefix": "cls", "body": "class \${1:名字}\\n    str name = \\"\\"\\n\\n    function init(name)\\n        self.name = name", "description": "定义类" },
      { "prefix": "try", "body": "try\\n    \${1:1 / 0}\\ncatch e\\n    print(\\"出错：\\" + e.message)\\nfinally\\n    print(\\"清理\\")", "description": "异常处理" },
      { "prefix": "while", "body": "while (\${1:条件})\\n    \${2:out}", "description": "循环" },
      { "prefix": "ask", "body": "ask(\${1:\\"请输入：\\"})\\nprint(ans)", "description": "读取用户输入" },
      { "prefix": "listop", "body": "list \${1:xs} = [\\n    1,\\n    2\\n]\\nprint(len(\${1:xs}))", "description": "列表字面量" },
      { "prefix": "dictop", "body": "dict \${1:d} = {\\"键\\": \\"值\\"}\\nprint(\${1:d}[\\"键\\"])", "description": "字典字面量" }
    ],
    "keywords": [
      { "label": "join", "detail": "拼接任意个值为字符串", "insert": "join(" },
      { "label": "len", "detail": "长度", "insert": "len(" },
      { "label": "range", "detail": "计数范围", "insert": "range(" }
    ],
    "commands": [
      { "id": "float.starter.hello", "title": "起步包：在控制台打个招呼", "category": "插件", "run": "print(\\"你好，Float Script！\\")" },
      { "id": "float.starter.note", "title": "起步包：插入文件头注释", "category": "插件", "when": "editor", "insert": "// \${file}\\n// 创建于 \${date}\\n\\n" }
    ],
    "menus": {
      "tools": [
        { "command": "float.starter.hello", "label": "起步包：打招呼" }
      ]
    }
  }
}
`;

const statsJs = `// 代码统计插件：演示 main.js 怎么用注入的 float API
// 这个文件完全跑在受控环境里，只能碰 float 暴露出来的能力。

async function countAll() {
  const files = await float.files.list('');
  const cw = files.filter((f) => f.toLowerCase().endsWith('.cw'));
  let total = 0;
  let blank = 0;
  let comment = 0;
  const rows = [];

  for (const f of cw) {
    const text = await float.files.read(f);
    const lines = text.split('\\n');
    const nonEmpty = lines.filter((l) => l.trim() !== '').length;
    const cmt = lines.filter((l) => l.trim().startsWith('//')).length;
    total += lines.length;
    blank += lines.length - nonEmpty;
    comment += cmt;
    rows.push({ file: f, lines: lines.length });
  }

  rows.sort((a, b) => b.lines - a.lines);
  float.console.print('工程共 ' + cw.length + ' 个 .cw 文件');
  float.console.print('总行数 ' + total + '（空行 ' + blank + '，注释 ' + comment + '）');
  for (const r of rows.slice(0, 10)) {
    float.console.print('  ' + r.file + ' — ' + r.lines + ' 行');
  }
  float.ui.notify('代码统计完成：' + total + ' 行');
  return total;
}

module.exports = {
  activate(float) {
    float.commands.register('float.stats.count', countAll);
    float.console.print('代码统计插件已启用');
  },
  commands: {
    count: countAll
  }
};
`;

const statsManifest = `{
  "id": "float.code-stats",
  "name": "代码统计",
  "version": "1.0.0",
  "publisher": "小方工作室",
  "description": "统计工程里 .cw 文件的数量、总行数、空行与注释行，并列出最大的文件。演示脚本式插件。",
  "main": "main.js",
  "tags": ["script", "analysis"],
  "permissions": ["files:read", "console:write", "ui:notify"],
  "contributes": {
    "commands": [
      { "id": "float.stats.run", "title": "代码统计：统计整个工程", "category": "插件", "handler": "count" }
    ],
    "menus": {
      "tools": [
        { "command": "float.stats.run", "label": "统计代码行数" }
      ]
    }
  }
}
`;

const todoJs = `// TODO 扫描插件：找出工程里所有 TODO / FIXME / XXX，方便收尾

async function scanTodos() {
  const files = await float.files.list('');
  const cw = files.filter((f) => f.toLowerCase().endsWith('.cw'));
  const marks = ['TODO', 'FIXME', 'XXX', 'HACK'];
  let found = 0;

  for (const f of cw) {
    const text = await float.files.read(f);
    const lines = text.split('\\n');
    lines.forEach((line, i) => {
      for (const m of marks) {
        if (line.includes(m)) {
          found += 1;
          float.console.print(f + ':' + (i + 1) + '  ' + line.trim());
          break;
        }
      }
    });
  }

  if (found === 0) {
    float.console.print('没有待办标记，干净。');
    float.ui.notify('没有 TODO');
  } else {
    float.console.print('共找到 ' + found + ' 处待办');
    float.ui.notify('找到 ' + found + ' 处待办');
  }
  return found;
}

module.exports = {
  activate(float) {
    float.commands.register('float.todo.scan', scanTodos);
  },
  commands: { scan: scanTodos }
};
`;

const todoManifest = `{
  "id": "float.todo-scan",
  "name": "待办扫描",
  "version": "1.0.0",
  "publisher": "小方工作室",
  "description": "扫描工程中所有 TODO / FIXME / XXX / HACK 标记，输出到控制台。",
  "main": "main.js",
  "tags": ["script", "todo"],
  "permissions": ["files:read", "console:write", "ui:notify"],
  "contributes": {
    "commands": [
      { "id": "float.todo.run", "title": "待办扫描：列出全部 TODO", "category": "插件", "handler": "scan" }
    ],
    "menus": {
      "tools": [
        { "command": "float.todo.run", "label": "扫描待办标记" }
      ]
    }
  }
}
`;

export const BUILTIN_PLUGINS: BuiltinPlugin[] = [
  { id: 'float.starter-pack', manifest: JSON.parse(starter), files: { 'plugin.json': starter } },
  {
    id: 'float.code-stats',
    manifest: JSON.parse(statsManifest),
    files: { 'plugin.json': statsManifest, 'main.js': statsJs }
  },
  {
    id: 'float.todo-scan',
    manifest: JSON.parse(todoManifest),
    files: { 'plugin.json': todoManifest, 'main.js': todoJs }
  }
];
