# memos-cli v0.2.0 测试用例（正式版）

- 日期: 2026-09-20
- 适用: memos-cli v0.2.0（源码构建；命令示例统一 `memos-cli`，可用 `mct` 调试构建别名指代）
- 关联: `docs/user-manual-v2.0.md`（验收凭据）、`docs/v0.2-syntax-upgrade.md`、`docs/v0.2-test-plan.md`（round 1 记录）、`docs/v0.2-test-plan-round2.md`（round 2 记录）
- 编号: `TC-<组>-<序号>`；映射需求 FR（PRD v2.0）
- 优先级: P0=核心/数据安全（阻断发布）；P1=常规功能；P2=边界/体验/兼容

## 优先级定义

| 级 | 含义 | 失败影响 |
|---|---|---|
| P0 | 主流程、数据安全、回归锁定 | 阻断发布，必须修复 |
| P1 | 常规功能、常用语法 | 高优先级修复 |
| P2 | 边界、兼容、体验 | 按需修复 |

## A. 新增（FR4）

### TC-A-01 add 主命令 + `+tag` 转写（P0）

前置: usememos 在线、token 已配置；无 `sometag` 标签残留。
步骤:
1. 执行 `memos-cli add another test +sometag`
预期:
- stdout 首行 `created\t<22 字符 uid>`，退出码 0
- 该 memo 正文 = `another test #sometag`（`+` 转写入字面 `#`）
- API tags 字段含 `sometag`：`memos-cli li +sometag` 命中且仅命中该条
- 命令全程无引号（shell 不产生注释截断）

### TC-A-02 多词正文拼接（P0)

前置: 同 A-01。
步骤:
1. `memos-cli add hello world test`
预期: 正文 = `hello world test`（空格分隔拼接）。

### TC-A-03 多标签（P1）

步骤:
1. `memos-cli add note +tag1 +tag2`
预期: 正文 = `note #tag1 #tag2`；tags 含 tag1、tag2；`li +tag1 +tag2` 命中。

### TC-A-04 `--tag` 兼容（P1）

步骤:
1. `memos-cli add note --tag work`
预期: 正文 = `note #work`；tags 含 work。

### TC-A-05 空正文读 stdin（P1）

步骤:
1. `printf 'stdin line' | memos-cli add`
预期: 正文 = `stdin line`，`created` 成功。

### TC-A-06 `+` 别名兼容（P0 回归）

前置: v0.1 用户习惯不受破坏。
步骤:
1. `memos-cli + hello`
预期: 创建成功（`created`），与 add 行为一致。

### TC-A-07 字面 `#` 需要引号（P2，文档化行为）

步骤:
1. `memos-cli add issue #123`（无引号）
预期: 创建 memo 正文仅 `issue`（shell 吞掉 `#` 后续）——文档化约束，同 TW。
2. `memos-cli add "issue #123"`（引号）
预期: 正文 = `issue #123`（`#123` 不被当作标签）。

## B. 列表与表头（FR1）

### TC-B-01 li 默认列表四列（P0）

前置: 库内 ≥3 条 memo（含 1 条无标签、1 条多标签）。
步骤:
1. `memos-cli li`
预期:
- 首行恰为 `Id\tDate\tTag\tDescription`（title case，tab 分隔）
- 数据行 4 列：编号（1-based）、`YYYY-MM-DD`、标签（空格分隔/无标签 `-`）、内容（截断 72）
- 默认 20 条以内

### TC-B-02 ls 三列（P0）

步骤:
1. `memos-cli ls`
预期: 首行 `Id\tTag\tDescription`；数据行 3 列，无时间列；内容截断 40。

### TC-B-03 表头大小写（P1）

步骤:
1. `memos-cli li`、`memos-cli ls`
预期: 表头为 title case（`Id/Date/Tag/Description`），非全大写。

### TC-B-04 TTY 对齐 / 管道 TSV 双模式（P1）

步骤:
1. 终端直接执行 `memos-cli li`
预期: 空格对齐表格；`Date` 与 `Tag` 列之间有 ≥2 空格间距；全行数据列起始一致；编号青色 ANSI。
2. 执行 `memos-cli li > f.tsv`，查看文件
预期: 文件为 tab 分隔纯文本（无对齐空格、无 ANSI）；`cut -f 3 f.tsv` 可取标签列。

### TC-B-05 日期精确到日（P0）

步骤:
1. `memos-cli li`
预期: 第 2 列匹配 `^\d{4}-\d{2}-\d{2}$`（无 `T`、无时区）。
2. `memos-cli li --raw`
预期: `create_time` 仍为完整 RFC3339。

### TC-B-06 条数控制（P1）

步骤:
1. `memos-cli li -n 1`
预期: 表头 + 恰 1 数据行。
2. `memos-cli li --page-size 100`
预期: 拉取量参数生效（API 侧），无报错。

### TC-B-07 `--raw` JSON（P1）

步骤:
1. `memos-cli li --raw`
预期: 输出 `{"memos": [...]}` JSON；无表头。

### TC-B-08 空列表表头（P2）

前置: 筛选条件无命中。
步骤:
1. `memos-cli li /无此正则xyz/`
预期: 仅输出表头行（`Id Date Tag Description`），无数据行。

### TC-B-09 编号不重排（P1）

步骤:
1. `memos-cli li`（记录编号）、`memos-cli li +tagA`
预期: 命中行的编号与全量列表一致（筛选只过滤展示、不重排）。

## C. 查看与完整信息（FR2）

### TC-C-01 无参默认 list（P0)

步骤:
1. `memos-cli`
预期: 输出与 `memos-cli li` 相同（无子命令 = 默认 list）。

### TC-C-02 编号查看完整信息（P0）

步骤:
1. `memos-cli li` 取第一条编号 N；`memos-cli N`
预期: 输出四键值行：`uid:`（22 字符）、`date:`（YYYY-MM-DD）、`tags:`（无标签 `-`）、`content:`（正文原样）；退出码 0。

### TC-C-03 uid 查看完整信息（P0）

步骤:
1. `memos-cli add sample +tagX` 取 uid；`memos-cli <uid>`
预期: 与 TC-C-02 输出结构一致；content 含 sample。

### TC-C-04 完整信息键值完整性（P1）

步骤:
1. 任一 memo 执行默认查看
预期: 含且仅含 `uid:`、`date:`、`tags:`、`content:` 四键；无多余字段。

### TC-C-05 完整信息 --raw（P1）

步骤:
1. `memos-cli <uid> --raw`
预期: JSON 全文（含完整 create_time、visibility）。

### TC-C-06 多行正文展示（P2）

前置: 存在多行正文 memo（经 edit 或 import 创建）。
步骤:
1. `memos-cli <uid>`
预期: content 行首行带前缀，后续行原样输出（无前缀）——文档化格式。

## D. 筛选语法（FR6）

### TC-D-01 `+tag` / `-tag`（P0）

步骤:
1. `memos-cli li +work`、`memos-cli li -home`
预期: 分别只含/排除对应 API tags；`-home` 等价补集展示。

### TC-D-02 `/regex/`（P0）

步骤:
1. `memos-cli li /^https/`、`memos-cli li '/(?m)^AB$/'`
预期: Rust regex 语义；`(?m)` 多行生效。

### TC-D-03 裸词 contains（P0）

步骤:
1. `memos-cli li 草稿`
预期: 命中正文含 `草稿` 子串的 memo（大小写敏感）。

### TC-D-04 组合 AND（P0）

步骤:
1. `memos-cli li +work /urgent/ -test`
预期: 三者同时满足才显示。

### TC-D-05 筛选首参省略 list（P0）

步骤（各一次）:
1. `memos-cli +work`、`memos-cli -home`、`memos-cli /re/`、`memos-cli 裸词`、`memos-cli time.after:1w`
预期: 均等价于对应 `li <筛选>`；`+` 精确等于新增命令（`memos-cli +` 走 add 别名，`memos-cli +work` 走筛选）。

### TC-D-06 时间筛选与智能日期（P1）

步骤:
1. `memos-cli li time.after:1w`
预期: 命中最近一周创建的 memo（相对量=距今往前 N）。
2. `memos-cli li time.before:2026-08-01`
预期: 命中该日前创建的 memo。
3. `memos-cli li time.after:banana`
预期: 报 `time.after 日期无效: banana`，退出非零。
4. `memos-cli li time.after:`
预期: 报错（空值非法）。

### TC-D-07 属性修饰符（P2）

步骤:
1. `memos-cli li tags.has:work`（等价 `+work`）
2. `memos-cli li tags.hasnt:work`（等价 `-work`）
3. `memos-cli li content.startswith:https://`
4. `memos-cli li content.endswith:.md`
预期: 各自语义正确；空值（`tags.has:`）报错。

### TC-D-08 空标记/-数字非筛选（P1 回归）

步骤:
1. `memos-cli li +`、`memos-cli li -`、`memos-cli li //`、`memos-cli li -1`
预期: 空标记报"无法解析筛选"；`-1` 不视为排除标签。

### TC-D-09 default_filters 与 --no-default（P2）

前置: 配置含 `MEMOS_DEFAULT_FILTERS=+work`（临时 MEMOS_CLI_CONFIG 文件）。
步骤:
1. `memos-cli li` / `memos-cli tags` / `memos-cli ids`
预期: 自动叠加 `+work`（AND）。
2. `memos-cli --no-default li`
预期: 不叠加默认筛选。
3. `memos-cli del +work -y`
预期: 默认筛选**不**自动应用于删除（数据安全）。

## E. 删除（FR5）

### TC-E-01 编号/范围/逗号/uid（P0）

步骤:
1. 建 3 条测试 memo；`memos-cli del 1 2-3 5`（确认流程）与 `del 2-3,5 --force`
预期: 分别删除对应编号；范围、逗号、uid 目标均生效。

### TC-E-02 确认流程（P0）

步骤:
1. `printf 'y\n' | memos-cli del 1`
预期: 先逐条预览（`确认删除 #N <预览>？`），输入 y 后逐条 `deleted\t<uid>`。
2. `printf 'n\n' | memos-cli del 1`
预期: 输出 `已取消`，不删除。

### TC-E-03 `-y`/`--force`（P0）

步骤:
1. `memos-cli del <uid> -y`
预期: 跳过确认直接 `deleted`。

### TC-E-04 筛选目标删除（P0）

步骤:
1. 建多条带 `draft` 标签 memo；`memos-cli del +draft -y`
预期: 全部 draft 标签 memo 被删（全量展开，非仅 page_size 窗口）；输出逐条 `deleted`。
2. `memos-cli del /草稿-/`（确认流程）
预期: 预览正则命中项，确认后删除。

### TC-E-05 筛选零匹配报错（P1）

步骤:
1. `memos-cli del +不存在标签xyz -y`
预期: 报 `筛选无匹配: +不存在标签xyz`，退出非零，无删除。

### TC-E-06 编号 0 超出范围（P0 回归，P0-1）

步骤:
1. `memos-cli del 0`；`memos-cli del 0-2`
预期: 报 `编号 0 超出范围`，退出非零，不 panic（debug/release 一致）。

### TC-E-07 `-tag` 与 `-y` flag 冲突（P1）

步骤:
1. `memos-cli del -work -y`
预期: `-y` 被消费为 force；`-work` 为排除标签筛选；结果 = 删除非 work 标签 memo。

### TC-E-08 筛选与编号合并去重（P2）

步骤:
1. 命中某编号的 memo 同时被同一筛选命中：`memos-cli del +draft 3 -y`
预期: 该 memo 只删除一次（去重）。

## F. 导出与导入（FR7/FR8）

### TC-F-01 export TSV 格式与转义（P1）

步骤:
1. `memos-cli export > all.tsv`
预期: 每行 `uid\tcontent\tcreate_time`；content 内 `\`/换行/制表符转义为 `\\`/`\n`/`\t`；全量分页拉取。

### TC-F-02 import 幂等（P0）

步骤:
1. 构造 TSV 行含已存在 uid：`printf '<uid>\tnew\t\n' | memos-cli import`
预期: 输出 `already exists\t<uid>`，不创建新 memo（重复导入安全）。
2. 全新 uid 行：输出 `imported\t<uid>`。

### TC-F-03 import 错误行不中断（P2）

步骤:
1. 输入含 1 条非法行 + 1 条合法行
预期: 非法行输出 `error ...`，合法行仍 `imported`；退出码 0。

## G. 配置（FR9）

### TC-G-01 config --show 脱敏（P0）

步骤:
1. `memos-cli config --show`
预期: token ≤8 字符显示 `****`，否则 `前4...后4`；绝不打印完整 token。

### TC-G-02 config 写入回读校验（P1）

步骤:
1. `memos-cli config --base http://x:5230 --token t123`；`memos-cli config --show`
预期: 写入成功、回读校验通过、show 输出所见即所得。

### TC-G-03 编码容错（P2 回归）

步骤:
1. 分别以 UTF-8 BOM、UTF-16 LE/BE（含 BOM）、CRLF 保存配置文件后 `config --show`
预期: 均能正确解析（无 BOM UTF-16BE 误读为文档化限制）。

### TC-G-04 default_filters 行解析（P2）

步骤:
1. 配置 `MEMOS_DEFAULT_FILTERS=+work /urgent/ -test`
预期: `config --show` 显示该行；解析为 `["+work","/urgent/","-test"]`。

## H. help 与 version（FR 全局）

### TC-H-01 版本与编译时间（P1）

步骤:
1. `memos-cli --version`；`memos-cli version`
预期: 均输出 `memos-cli 0.2.0 (built <ISO8601>)`，含 built。

### TC-H-02 help 英文（P1）

步骤:
1. `memos-cli --help` / `memos-cli help`
预期: 子命令说明为英文；含 `Unofficial usememos CLI + TUI`；`Usage: memos-cli [OPTIONS] [COMMAND]`（子命令可选）。
2. `memos-cli help delete` / `memos-cli help -`
预期: 对应子命令英文帮助；`delete` 用完整命令名（`Usage: delete [OPTIONS] [TARGETS]...`）。
3. `memos-cli help badcmd`
预期: 报 `Unknown subcommand: badcmd`，退出非零。
4. `memos-cli --alias`
预期: 输出别名对照表（list/add/delete/edit/config），不含其他内容。
5. `memos-cli --alias --raw`
预期: 报 requires a subcommand（--alias 独立，不与其它 flag 混用），退出非零。

### TC-H-03 已移除命令（P1 回归）

步骤:
1. `memos-cli get 1`、`memos-cli --help`
预期: `get`/`show`/`cat` 不在 help 列表；`get` 报未知子命令。
2. `memos-cli l`（单字符）
预期: 不报错——按裸词筛选（等价 `memos-cli li l`）；help 中无 `l` 命令。

### TC-H-04 未知子命令（P2）

步骤:
1. `memos-cli frobnicate`
预期: clap 报未知子命令，退出非零。

## I. TUI（FR10）

### TC-I-01 tui 启动与导航（P2）

步骤:
1. TTY 下 `memos-cli tui`；`/` 搜索；`j/k` 导航；`q` 退出
预期: 全量加载、搜索过滤、退出正常。

### TC-I-02 bad token 快速失败（P1）

步骤:
1. `memos-cli tui --token x --base http://127.0.0.1:1`
预期: 快速报错退出，不 hang。

## J. 系统行为（非功能）

### TC-J-01 管道截断无 panic（P1）

步骤:
1. `memos-cli li | head -1`
预期: head 取 1 行；memos-cli 被 SIGPIPE 静默终止（无 broken pipe panic 脏输出）。

### TC-J-02 超时与非 2xx 摘要（P2）

步骤:
1. `memos-cli --base http://127.0.0.1:1 li`
预期: 连接错误快速返回（30s 超时内）；非 2xx 输出状态码+body 摘要（≤200 字符）。

### TC-J-03 单二进制零外部依赖（P2）

步骤:
1. `ldd target/release/memos-cli` 检查
预期: 无 bash/curl/jq/python 运行时依赖（动态链接仅系统库）。

## K. 回归锁定清单（每次发布前全跑）

| 用例 | 锁定内容 |
|---|---|
| TC-E-06 | P0-1 编号 0 不 panic |
| TC-A-06 | `+` 别名兼容 |
| TC-D-08 | 空标记/-数字非筛选 |
| TC-G-03 | 配置编码容错 |
| TC-H-03 | get/l 已移除语义 |
| TC-B-04/TC-J-01 | TSV 纯净与 SIGPIPE |

## L. 执行与通过标准

1. 自动化: `cargo test` 全绿（lib 59 + client_mock 8 + func_help 6 = 73）
2. 手工: 本用例集 P0 全过 → 可发布；P1 全过 → 视觉/语法完整；P2 按需
3. 每条用例执行后记录实际输出；与预期不符 → 回填缺陷（用例 ID + 实际/期望）
4. 实机环境: usememos v0.30.x + 本机 token（MEMOS_CLI_CONFIG 可隔离测试配置）