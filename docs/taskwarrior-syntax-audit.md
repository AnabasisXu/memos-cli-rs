# Taskwarrior 语法对照审计（memos-cli 可吸收点）

- 日期: 2026-09-20
- 基线: memos-cli v0.1.0（src/cli/mod.rs + src/api.rs）、Taskwarrior 3.5.0 `doc/man/task.1.in`（全文 1636 行）+ `doc/notes/taskwarrior-abbreviations.txt`
- 结论来源: 源码/手册/实测（`cargo run` 三例），无推断未标注

## 1. 已吸收（Taskwarrior 来源确认）

| memos-cli 语法 | Taskwarrior 原型 | 状态 |
|---|---|---|
| `+tag` 筛选（只看 API tags 字段） | `+tag` filter | ✅ |
| `-tag` 排除 | `-tag` filter | ✅ |
| `/regex/` 正文正则（Rust regex） | `/regex/` | ✅ |
| 多筛选 AND 组合 | 隐式 and | ✅ |
| 首参 `+tag` / `/regex/` 省略 `list` | `task <filter>`（隐含默认命令） | ✅ 部分（见 §3.1） |
| 编号/范围/逗号/uid 批量目标 `1 2-3,5 u5` | `task 1,4-10,19 delete` | ✅ |
| `+` 命令中 `+word` 打标签（写）vs list 中 `+tag` 筛选（读） | 同词读/写上下文语义 | ✅ |
| 删除默认确认 + `-y/--force` | rc.confirmation + force | ✅ |
| `-` 后数字（`-1`）不视为排除标签 | 类似保留 | ✅ |
| 编号为当次列表 1-based 真实编号、可变 | 同（ID 是 working set 索引） | ✅ |
| 列表编号青色高亮 | 配色 | ✅ |
| 别名：`l/ls`、`e`、`del/rm`、`cfg` | 命令缩写（唯一前缀） | ✅ 但体系不同，见 §3.8 |
| `--` 全局 base/token（等效内联覆盖） | `rc.<name>:<value>` | ✅ |

## 2. 吸收边界（哲学差异，判断取舍用）

- **命令位置**：Taskwarrior 是 `task <filter> <command> [mods]`（filter 前置），**任何命令可作用于筛选集合**（`task +work done`）。memos-cli 是 `<command> <args>`（git 风格，command 前置）。→ 批量操作迁移到筛选目标需要额外设计（§3.4）。
- **裸词语义**：Taskwarrior 裸词 = `description.contains:词`（因为 add 必须显式）；memos-cli 的 `+` 已显式区分新增，**裸词无歧义，可安全吸收**（§3.2）。
- **状态机**：Taskwarrior 有 completed/deleted/waiting/recurring 状态与虚拟标签；memos 只有 content/tags/visibility。状态相关语法（done/purge/log/virtual tags）不适用。
- **可配置性**：Taskwarrior 报表/列/别名/上下文全部 rc 驱动；memos-cli 面向 3 秒即用，YAGNI，只吸收直接提效的部分。

## 3. 未吸收但值得吸收（按价值排序）

### 3.1 `-tag` 首参省略 list —— 纯遗漏，直接修（P0）

`run()` 只重写 `+tag` 和 `/regex/` 首参，漏了 `-tag`：

实测：

```
$ memos-cli -work
error: unexpected argument '-w' found
```

Taskwarrior 等价用法 `task -work list` 是常规模糊。修复：`run()` 增加 `first.starts_with('-') && !first.starts_with("--") && first.len() > 1 && !first[1..].chars().next().is_ascii_digit()` 分支。注意与 `-y`/`--force` 的冲突只在 del 子命令内，顶层无冲突。

### 3.2 裸词筛选（P0）—— `l foo` 应命中正文含 foo 的笔记

Taskwarrior: `task foo list` ≡ `task description.contains:foo list`。
memos-cli 现在实测报错：

```
$ memos-cli l foo
memos-cli: 无法解析筛选: foo
```

建议 `Filter::parse` 对无前缀裸词返回 `Contains(String)`（正文子串，按词边界可选），与 `/regex/` 互补：裸词=子串、`/re/`=正则。AND 组合自然生效。风险：拼错的筛选静默返回空，而非报错——Taskwarrior 同样行为；可用"零命中提示"缓解（可选）。

### 3.3 无参默认 list（P0/P1）

Taskwarrior: `task` ≡ `task list`。实测 memos-cli 裸跑只出 usage。建议无子命令时默认 `list`。对 agent 与终端均友好，零成本。

### 3.4 筛选目标批量删除（P1）—— filter+command 模式的移植

Taskwarrior: `task +work delete`（filter 前置救活一切）。
memos-cli 现状：`del` 目标只收编号/uid；`del +work` 会解析失败。建议 `parse_delete_targets` 接受 `+tag` / `-tag` / `/regex/`（内部先 `apply_filters` 展开为 uid 列表）。冲突处理：`-tag` 与 `-y` 皆 `-` 开头，现有代码已特判 `-y/--force/--confirm`，追加一个"非已知 flag 且符合 `-tag` 形"分支即可（`-` 后非数字）。批量场景价值高：`memos-cli del +draft -y`。

### 3.5 时间筛选 + 智能日期（P1）

Taskwarrior 的属性修饰符 `due.before:eom` / `due.after:today` + 智能日期（now/today/sow/som/soy/eow/eom/eoy/1w/3d/fri）。
memos 每条有 create_time，按时间筛笔记是真实高频需求。建议：

```
memos-cli l time.after:1w     # 最近一周
memos-cli l time.after:2026-09-01
memos-cli l time.before:eom
```

实现：`Filter::TimeAfter/TimeBefore`，日期解析支持相对值（`N[d|w|m|y]`）与 ISO。这是 §3.6 架构的先行者。

### 3.6 属性修饰符架构 `attribute.op:value`（P1，打底）

Taskwarrior 修饰符全集：`before after by none any is isnt has hasnt startswith endswith word noword`（同义词 under/over/equals/contains/left/right）。

memos 当前字段少（tags/create_time/content），但架构值得一次到位：`tags.has:work`、`content.startswith:http`、`visibility:PRIVATE`。未来加字段（pinned/archived 等）零成本扩展。至少把 `tags.` 与 `time.`（§3.5）做出来。

### 3.7 `tags` 命令（P1，低成本）

Taskwarrior: `task tags`（列出全部标签+计数，受 filter 影响）。memos-cli 加：`memos-cli tags [+filter...]` → 标签\t计数，供标签整理与脚本。实现 = `list_all` + 聚合，~30 行。

### 3.8 脚本辅助命令 `ids` / `uuids`（P1，agent 场景）

Taskwarrior `_ids`/`ids`/`uuids`/`_unique <attr>` 输出纯 id 列表供脚本组合。memos-cli 场景：`memos-cli ids +draft` 输出 uid 行 → xargs 批量操作；`memos-cli get $(memos-cli ids +work | head -1)`。实现简单（复用 list + 筛选，只打印列）。

### 3.9 上下文/默认筛选（P2）

Taskwarrior context = 用户定义查询自动应用到所有命令（读/写分离）。memos 简化版：配置文件 `default_filters`（如 `+work`），list/批量操作自动叠加；`--no-default` 跳过。比完整 context（read/write 双定义、`context <name>` 切换）轻得多。若用户单标签使用率高可做。

### 3.10 import 幂等加固（P2，数据安全）

Taskwarrior import 按 UUID 更新已存在任务；memos 无法指定 uid，但可退化为"uid 已存在则跳过"。当前 `import` 重复跑一遍会全量重复创建。语法不变，行为补 `already exists <uid>` 行。

## 4. 不建议吸收（及原因）

| Taskwarrior 特性 | 原因 |
|---|---|
| 虚拟/特殊标签（+TODAY/+ACTIVE/+nocolor…） | memos 无对应状态机，概念空转 |
| delete + purge 两级删除 | usememos API 是物理删除，无软删层 |
| `undo` | memos-cli 无修改历史可回滚，成本高 |
| `task <id> <mods>` 隐含 modify | memos 只有 content 可改，edit 已覆盖 |
| filter or/xor/`()` 逻辑表达式 | 笔记筛选 AND 已够；括号+引号徒增复杂度 |
| 属性值枚举校验（priority:H/M/L） | 校验责任在服务端，客户端浪费 |
| 命令唯一前缀缩写（abbreviation.minimum） | clap 子命令体系冲突多；现有硬编码别名已覆盖高频；（注意 TW 里 `l`/`e` 因歧义/最小长度都不可用，memos 的 `l`/`e` 更激进）
| context write 复杂隐射 | TW 官方文档自认坑：复杂 context 写入会产生垃圾（"or or" 进描述） |
| custom report columns / 别名 alias 配置 | 面向单列输出的轻工具，YAGNI |

## 5. 推荐落地顺序

1. §3.1 `-tag` 首参省略（bug 级，半小时）
2. §3.2 裸词筛选、§3.3 无参默认 list（各半小时，语法体验补齐）
3. §3.7 `tags` 命令、§3.8 `ids/uuids`（脚本化，各 ~1h）
4. §3.4 筛选目标批量删除（与 #2 共享 Filter 解析）
5. §3.5/§3.6 时间筛选 + 属性修饰符架构（一次设计到位）
6. §3.9 context 简化版、§3.10 import 幂等（按需）