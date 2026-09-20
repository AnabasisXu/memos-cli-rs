# memos-cli 产品需求文档（PRD）

- 版本: 2.0
- 日期: 2026-09-20
- 状态: 已实现（对应 v0.2.0 代码基线；v1.0 = v0.1.0 基线见 git 历史）
- 关联文档: `user-stories-v1.0.md`（许愿场景）、`user-manual-v2.0.md`（验收凭据）、`test-report-v0.2.0.md`（测试结果）、`v0.2-syntax-upgrade.md`（Taskwarrior 语法吸收修改计划）、`v0.2-test-plan.md`（测试计划）

## 1. 背景与问题

usememos 是自托管笔记服务，官方主要提供 Web 端。对终端重度用户与自动化场景，存在需求空缺：

- **无快速记录入口**：记一条笔记要开浏览器、登录、点新建，路径长
- **缺命令行筛选**：靠标签/正则过滤笔记（taskwarrior 风格 `+tag` / `-tag` / `/regex/`）在 Web 端低效
- **缺批量操作**：批量删除、全量导出备份、脚本化导入在 Web 端不可行
- **Agent 不可用**：AI Agent 操作笔记需要纯命令行接口（无浏览器依赖）
- **语法一致性**：命令行语法吸收 Taskwarrior 词法（裸词=正文子串、`attr.op:value`、唯一前缀缩写、智能日期），终端重度用户零学习成本

## 2. 目标与价值

交付一个**单二进制、无外部运行时依赖**（无 bash/curl/jq/python）的非官方 memos CLI + TUI：

1. 终端用户 3 秒内完成"记录一条笔记"（`memos-cli + 内容`）
2. 支持筛选、批量删除、TSV 全量导出/导入
3. 提供 TUI 供交互式浏览/搜索/批量管理
4. 通过 `Authorization: Bearer` + REST API 操作，不碰 SQLite

## 3. 目标用户

| 角色 | 需求 |
|---|---|
| 终端重度用户 | 快速记录、筛选、批量整理 |
| 备份/迁移用户 | 全量导出 TSV、批量导入 |
| AI Agent | 无浏览器命令行接口，可脚本化调用 |

## 4. 功能需求

### FR1 列表浏览

- 命令: `list`（别名 `li`；v0.2 删除 `l`——TW 中 `l` 是歧义前缀，`li` 是 list 唯一前缀缩写）
- **list 为全列**：`编号\t创建时间\t标签\t内容`（标签空格分隔、无标签 `-`、内容截断 72）；`-n/--limit` 限制条数；`--page-size` 控制 API 拉取量；`--raw` 输出 JSON（全局 flag）。**v0.2 移除 `-f/--full`**（list 本为全列，不再需要开关；`li -f` 现按 TW 语义为"排除标签 f"）
- **ls 短列表**：独立命令，`编号\t标签\t内容` 三列（无时间列，list 的子集；内容截断 40），支持与 list 相同筛选（对齐 TW `ls` = short listing）
- 首列编号为**当次拉取列表的 1-based 真实编号**（不是筛选后的重排序号）
- 验收: `memos-cli li -n 3` 输出四列，`memos-cli ls` 输出三列且不含时间列（见手册 §3.1/§3.2 实测样例）
- 状态: ✅ 实测通过（v0.2）

### FR2 查看正文（v0.2：默认命令取代 get）

- 命令: `get`/`show`/`cat` 在 v0.2 **删除**；查看全文并入默认命令智能分派：
  - `memos-cli`（无参）＝ list；`memos-cli 3`（单个编号）＝ 查看编号 3 全文；`memos-cli <uid>`（≥20 字符或 memos/ 前缀）＝ 按 uid 查看全文
  - 筛选首参（`+tag`/`-tag`/`/re/`/裸词/`time:`）＝ list + 筛选（bash 风格；对齐 TW `task <filter>` 隐含默认报表）
- `--raw` 输出 JSON（全局 flag，`memos-cli <uid> --raw`）
- 多参数数字（`memos-cli 3 4`）按裸词筛选处理（正文含 3 且含 4）
- 验收: `memos-cli <uid>` 原样输出正文；末尾自动补换行（见手册 §3.3）
- 状态: ✅ 实测通过（v0.2）

### FR3 编辑正文

- 命令: `edit`（别名 `e`），调 `$EDITOR`，无改动不写回（输出 `no change`）
- 验收: 修改后 `patch` 成功输出 `updated <uid>`；未修改输出 `no change <uid>`
- 状态: ✅ 代码审阅通过（需要 EDITOR 的交互环境）

### FR4 新增笔记

- 命令: `add`（v0.2 主名；别名 `+` 保留兼容；对标 TW `task add`），正文多词拼接；`+word` 打标签；`--tag` 兼容；正文为空则读 stdin
- **+语法决策**（v0.2）：标签输入用 `+tag`，内部转写为正文 `#tag`——usememos 前端只在正文识别 `#tag` 渲染，但 `#` 是 shell 注释符（输入需引号）；TW 同款 `+` 免引号且与筛选词法一致
- 验收: `memos-cli add another test +sometag` 无引号创建，正文含 `#sometag` 且 API tags 命中 `+sometag` 筛选；`memos-cli + hello` 别名可用
- 状态: ✅ 实测通过（v0.2）

### FR5 批量删除

- 命令: `delete`（别名 `-` / `del` / `rm`；v0.2 主名改 `delete`，对齐 TW `task delete`，help 只展示完整命令名——`-`/`+` 等符号别名隐藏，统一由 `--alias` 展示），支持编号/范围/逗号列表/uid；默认逐条预览并确认（y/N）；`-y/--force` 跳过确认
- **筛选目标**（v0.2）：`+tag` / `-tag` / `/regex/` / 裸词 / `time:` 展开为匹配集合并入（全量 list_all 展开，防 page_size 窗口假象）；筛选零匹配报错（防误删）；与编号/uid 合并去重
- `-tag` 与 `-y`/`--force` 的 `-` 前缀冲突由解析器先消费 flag 再识别标签
- 编号 0 必须报"编号 0 超出范围"而非 panic（P0-1，含范围分支 `0-2`）
- 验收: 见手册 §3.6；`memos-cli del 0` 不崩溃、退出码非 0；`memos-cli del +draft -y` 只删匹配项
- 状态: ✅ 实测通过；P0-1 已修复并有回归测试

### FR6 标签、正则、裸词、时间与属性筛选

- 语法: `+tag` 只看 API `tags` 字段、`-tag` 排除、`/regex/` 正文正则（Rust regex，可 `(?m)` 多行）；可组合（AND）
- **裸词**（v0.2）：无前缀词 = 正文子串筛选（对标 TW `description.contains:word`）；`l foo` 不再报"无法解析筛选"
- **时间筛选**（v0.2）：`time.after:X` / `time.before:X`（X 支持相对量 `1w`/`3d`/`2mo`/`1y`＝距今往前 N、命名 `today`/`som`/`eom`/`soy`/`eoy`/`sow`/`eow`、绝对日期与 RFC3339；边界严格不含相等）
- **属性修饰符**（v0.2）：`tags.has:` / `tags.hasnt:` / `content.startswith:` / `content.endswith:`
- **首参省略 `list`**（v0.2 扩展至全部筛选语法，含 `-tag`；bash 风格 + TW 隐含默认命令）
- `memos-cli +work /urgent/ -test` 三者同时满足才显示
- 状态: ✅ 实测通过（`/验收/` 命中测试 memo；`time.after:1w` 命中近期条目）

### FR7 TSV 导出

- 命令: `export`，全量 `list_all`（分页，pageToken 支持 URL 编码），输出 `uid \t content \t create_time`
- content 内换行/制表符/反斜杠转义（`\n` `\t` `\\`）
- 状态: ✅ 实测通过

### FR8 TSV 导入（v0.2 幂等）

- 命令: `import`，从 stdin 读，跳过空行与 `uid` 头行；`splitn(3, '\t')`，反转义；逐条 `create`，单条失败不中断（输出 `error` 行）
- **幂等**（v0.2）：uid 已存在 → `already exists <uid>` 跳过（对标 TW import 按 UUID 更新；memos 无法指定 uid，降级为跳过）；重复导入不产生重复笔记
- 状态: ✅ 实测通过（同 uid 二次导入输出 already exists）

### FR9 配置管理

- 优先级: `--base/--token` > 环境变量 `MEMOS_BASE/MEMOS_TOKEN` > 配置文件 `~/.config/memos-cli/env`（可 `MEMOS_CLI_CONFIG` 覆盖）> 默认 `http://127.0.0.1:5230`
- **默认筛选**（v0.2）：`MEMOS_DEFAULT_FILTERS=+work /urgent/` 常驻叠加到 list/ls/tags/ids（AND）；`--no-default` 跳过；不同于危险命令（del 不自动应用）；对标 TW context 简化版
- 缺 token 时报错提示；`config --show` 展示生效配置且 token 脱敏（≤8 字符全 `****`，否则前4…后4）
- 写入自动 UTF-8 无 BOM + LF + 0600；读取容错 UTF-8 BOM / UTF-16 LE/BE / CRLF
- 状态: ✅ 实测通过（`config --show` 输出脱敏 token；default_filters 生效与 --no-default 跳过）

### FR10 TUI

- 命令: `tui`，全量加载 + 本地搜索（`/`），导航 `j/k/g/G/h/l`，选中 `Space`，删除 `d`（选中）/`D`（当前），编辑 `Enter`，刷新 `r`，退出 `q`/`Ctrl-C`
- 分页每页 20 条；`PAGE` 常量
- 状态: ✅ 实网集成测试覆盖（bad token 快速失败）

### FR11 tags：标签统计（v0.2）

- 命令: `tags [筛选...]`，`list_all` + `apply_filters` 后聚合 API `tags` 字段，输出 `标签\t命中数`，计数降序、同数字典序
- 对标 TW `task tags`；用途：标签整理、发现拼写变体、配 FR5 批量清理
- 状态: ✅ 实测通过

### FR12 ids：uid 列表（v0.2）

- 命令: `ids [筛选...]`，每行一个 uid（稳定，不受编号变化影响）；供脚本组合（`memos-cli get $(memos-cli ids +work | head -1)`）
- memos 无删除/完成状态，TW 的 ids/uuids 区分无意义 → 单命令输出 uid
- 状态: ✅ 实测通过

## 5. 非功能需求

| 项 | 要求 |
|---|---|
| 依赖 | 单二进制；运行时零外部命令依赖（构建期 date 注入编译时间；unix 下 libc 处理 SIGPIPE） |
| 网络 | HTTP 30s 超时；非 2xx 抛出状态码+body 摘要（≤200 字符） |
| 安全 | 配置文件权限 0600；token 脱敏；绝不打印完整 token |
| 编码 | 配置读写编码容错（见 FR9）；导出内容转义保证 TSV 结构化 |
| 兼容 | usememos v0.30.x 验证；`items` 别名响应兼容 |
| 行为一致性 | debug/release 构建行为一致（P0-1 已固化） |
| 管道 | SIGPIPE 恢复默认：`\| head` 截断静默终止，无 broken pipe panic 脏输出 |
| 版本 | `--version`/`version` 输出 `0.2.0 (built <UTC ISO-8601>)`（build.rs 注入；时间戳随重编译更新） |

## 6. 边界与非目标

- 不提供 Web UI、不管理多实例、不做本地缓存/离线
- 不替代官方客户端；`visibility` 固定 `PRIVATE`
- `+tag` 筛选只匹配 API `tags` 字段，不扫正文字面 `#tag`（文档化限制）
- uid 长度启发式：≥20 字符或 `memos/` 前缀视为显式目标；<20 字符短词按裸词筛选（真实 uid 均为 22 字符 base62，不受影响）
- 智能日期精度：相对量 m=30d、y=365d（日历语义如 `eom` 用真实历法）；`time.after:1w` 语义为"距今往前一周"（create_time 均为过去值）
- `time:1w`（缺操作符）为非法；`time.after:` 空值报错
- 无 BOM 的 UTF-16BE 配置会被按 LE 误读（文档化限制，见 test-report）

## 7. 验收总则

1. 按 `user-manual-v2.0.md` 从 Get Started 到全部命令走通一遍，每处输出与文档样例一致
2. `cargo test` 全绿（lib 59 + client_mock 8 + func_help 6 + doc 0）
3. `user-stories-v1.0.md` 中每一个场景均可在手册中找到对应操作可复现
4. 无 API/行为破坏性变更发生时，手册经审阅后与代码同步维护