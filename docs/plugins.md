# Float IDE 插件开发

插件不是"以后再说"的功能——它是现在就能用的。写一个插件只需要一个文件夹、一个 `plugin.json`，不需要编译、不需要重启。

---

## 1. 插件长什么样

```
我的插件/
├── plugin.json      ← 必须有：清单
├── main.js          ← 可选：需要逻辑时才写
└── icon.svg         ← 可选
```

整个文件夹丢进插件目录就装好了。插件目录在「插件 → 打开插件目录」，一般是：

```
%APPDATA%\com.fangstudio.floatide\plugins\
```

装好之后在「插件」面板里能看到，开关立刻生效。

---

## 2. 最小插件：只有清单

`plugin.json`：

```json
{
  "id": "me.hello",
  "name": "打个招呼",
  "version": "1.0.0",
  "publisher": "你的名字",
  "description": "往控制台打一行字",
  "contributes": {
    "commands": [
      {
        "id": "me.hello.say",
        "title": "打个招呼",
        "category": "插件",
        "run": "print(\"你好！\")"
      }
    ]
  }
}
```

放进去 → 打开「插件」面板 → 它已经在列表里了 → 按 `Ctrl+Shift+P` 搜「打个招呼」就能执行。

`run` 里就是一段 **CW Script**，会被语言内核真跑一遍，输出进控制台。

### 命令能做的三件事

| 字段 | 作用 |
| --- | --- |
| `run` | 跑一段 CW Script |
| `insert` | 往当前编辑器插入文本 |
| `handler` | 调用 `main.js` 里 `commands` 导出的同名函数 |

`insert` 支持占位符：

| 占位符 | 变成 |
| --- | --- |
| `${file}` | 当前文件名 |
| `${path}` | 当前文件完整路径 |
| `${project}` | 工程根目录 |
| `${date}` | 2026-10-30 |
| `${time}` | 21:40 |

---

## 3. 代码片段

```json
{
  "contributes": {
    "snippets": [
      {
        "prefix": "fori",
        "body": "for i in range(${1:n})\n    ${2:print(i)}",
        "description": "计数循环"
      }
    ]
  }
}
```

在编辑器里输入 `fori`，补全列表里就会出现这条，回车展开成带占位符的骨架，按 Tab 在 `${1}` `${2}` 之间跳。

`contributes.snippets` 和内置片段是**同一份补全列表**，不存在"插件片段不生效"这种情况。

---

## 4. 需要逻辑：main.js

```js
// main.js
// 只能通过注入的 float 对象操作：拿不到 DOM，拿不到 Node，import 不了任何东西

async function count() {
  const files = await float.files.list('');
  const cw = files.filter((f) => f.toLowerCase().endsWith('.cw'));
  float.console.print('共 ' + cw.length + ' 个 .cw 文件');
  float.ui.notify('统计完成');
}

module.exports = {
  // 插件启用时调用一次（可选）
  activate(float) {
    float.commands.register('me.count.run', count);
  },
  // 供 plugin.json 里 contributes.commands[].handler 引用
  commands: { count }
};
```

对应清单：

```json
{
  "id": "me.count",
  "name": "代码统计",
  "version": "1.0.0",
  "main": "main.js",
  "permissions": ["files:read", "console:write", "ui:notify"],
  "contributes": {
    "commands": [
      { "id": "me.count.run", "title": "统计代码", "handler": "count" }
    ]
  }
}
```

---

## 5. 可用的 API

| API | 说明 | 需要的权限 |
| --- | --- | --- |
| `float.files.read(rel)` | 读工程内文件 | `files:read` |
| `float.files.write(rel, text)` | 写工程内文件 | `files:write` |
| `float.files.list(rel?)` | 递归列文件（返回相对路径） | `files:read` |
| `float.editor.getText()` | 当前编辑器全文 | `editor:read` |
| `float.editor.getSelection()` | 当前选区 | `editor:read` |
| `float.editor.setText(t)` | 替换全文 | `editor:write` |
| `float.editor.insert(t)` | 在光标处插入 | `editor:write` |
| `float.editor.getPath()` | 当前文件路径 | 不需要 |
| `float.editor.getCursor()` | `{ line, col }` | 不需要 |
| `float.language.run(code)` | 跑一段 CW Script，拿 `{ out, error }` | `language:run` |
| `float.console.print(msg)` | 写控制台 | `console:write` |
| `float.console.error(msg)` | 写控制台（错误色） | `console:write` |
| `float.ui.notify(msg)` | 界面提示 | `ui:notify` |
| `float.plugin.storage.get/set` | 插件私有键值存储 | 不需要 |
| `float.commands.register(id, fn)` | 注册命令 | 不需要 |
| `float.commands.execute(id)` | 调用命令 | 不需要 |

**权限不写会怎样**：调用被拒绝，控制台打一行「插件 X 没有 files:read 权限」，插件本身不会崩，别的插件也不受影响。

---

## 6. 清单字段全表

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `id` | ✅ | 唯一标识，建议 `发布者.名字`，只允许字母数字 `.` `-` `_` |
| `name` | ✅ | 显示名 |
| `version` | ✅ | 语义化版本 |
| `publisher` | | 发布者 |
| `description` | | 描述，面板里显示 |
| `main` | | 入口 JS，相对插件目录 |
| `engines.float` | | 例如 `>=0.8.0`，不满足只警告 |
| `tags` | | 分类标签 |
| `permissions` | | 见上表 |
| `contributes.commands` | | 命令 |
| `contributes.snippets` | | 片段 |
| `contributes.keywords` | | 并进 `.cw` 补全 |
| `contributes.menus` | | `{ "tools": [...] }`，加进「工具」菜单 |
| `contributes.statusBar` | | 状态栏文字 |

---

## 7. 插件生态：不用联网也能分发

插件目录下放一个 `registry.json`，就是一份离线索引：

```json
[
  {
    "id": "float.sql-helper",
    "name": "SQL 助手",
    "version": "1.0.0",
    "publisher": "数据组",
    "description": "把选中的 SQL 格式化成一段 Float 脚本",
    "tags": ["sql", "format"],
    "localPath": "D:/team/plugins/float.sql-helper"
  }
]
```

「插件 → 插件仓库」会把它列出来，点「安装」就从 `localPath` 复制进插件目录。

这个设计的好处：团队内部把插件包放共享盘，写一份 registry 就能互相安装，**不需要服务器、不需要账号、不需要联网**。

---

## 8. 内置的三个示例

| 插件 | 形态 | 看什么 |
| --- | --- | --- |
| 起步片段包 | 纯声明 | 怎么用 snippets / commands / menus |
| 代码统计 | 脚本 | 怎么用 `float.files` + `float.console` |
| 待办扫描 | 脚本 | 怎么遍历文件并按行过滤 |

在「插件 → 打开插件目录」里直接打开这三个文件夹照着改，是最快的上手方式。

---

## 9. 调试插件

- 插件出错不会让 IDE 崩：错误会打在该插件的卡片上，并写一行到控制台。
- 改完 `plugin.json` 或 `main.js` → 面板里关掉再打开开关，或者重启插件（重新 `reload`）。
- 想看插件到底干了什么：控制台里搜 `[插件]` 前缀。
