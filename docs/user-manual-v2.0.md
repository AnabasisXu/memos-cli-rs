# memos-cli 用户手册

- 版本: 2.0
- 日期: 2026-09-20
- 适用: memos-cli v0.2.0（Rust 单二进制，非官方 usememos 客户端）
- 本手册是验收凭据：所有样例输出均为本机实测，照此走通即视为验收通过
- 语法迭代来源：Taskwarrior（`+tag`/`-tag`/`/regex/`/裸词/`attr.op:value`/命令缩写）

## 1. 这是什么

通过 `Authorization: Bearer` + memos REST API（`/api/v1/memos`）操作笔记的 CLI 与 TUI，同一二进制。不碰 SQLite。已在 usememos v0.30.x 验证。

## 2. 开始使用（Get Started）

### 2.1 安装

```bash
# 预编译二进制（Linux）
chmod +x memos-cli-linux-x86_64
mv memos-cli-linux-x86_64 ~/.local/bin/memos-cli

# 或源码安装
cargo install --path .
```

验证：

```bash
memos-cli --version
# memos-cli 0.1.0 (built 2026-09-20T09:19:10Z)
```

### 2.2 配置

优先级（高 → 低）：
1. 命令行 `--base` / `--token`
2. 环境变量 `MEMOS_BASE` / `MEMOS_TOKEN`
3. 配置文件 `~/.config/memos-cli/env`（`MEMOS_CLI_CONFIG` 可改路径）
4. 默认 `http://127.0.0.1:5230`

推荐用内置命令写配置（自动标准 UTF-8 无 BOM + LF + 0600 权限）：

```bash
memos-cli config --base http://127.0.0.1:5230 --token memos_pat_xxx
memos-cli config --show
```

实测输出（token 脱敏，仅显示前4…后4；含默认筛选行）：

```
配置文件: /root/.config/memos-cli/env
MEMOS_BASE=http://127.0.0.1:5230
MEMOS_TOKEN=memo...Wd3T
```

令牌在网页端 **设置 → Access tokens** 创建（只显示一次）。

**常驻默认筛选**（可选，对标 Taskwarrior context 简化版）：配置行 `MEMOS_DEFAULT_FILTERS=+work`，list/tags/ids 自动叠加该筛选（AND）；`--no-default` 跳过。例：

```bash
echo 'MEMOS_DEFAULT_FILTERS=+work /urgent/' >> ~/.config/memos-cli/env
memos-cli li            # 等效 memos-cli li +work /urgent/
memos-cli --no-default li   # 不去默认筛选
```

环境变量：

```bash
export MEMOS_BASE=http://127.0.0.1:5230
export MEMOS_TOKEN=memos_pat_xxx
```

### 2.3 验收第一条命令

```bash
memos-cli li -n 3
```

预期输出（编号 + 创建时间 + 标签 + 内容四列，制表符分隔）：

```
1	2026-09-20T11:23:50Z	tagA tagB	列验证-测试 #tagA #tagB
2	2026-09-20T07:54:36Z	testtag	test #testtag
3	2026-09-18T05:29:48Z	-	Transaction  life managent
```

看到列表即配置生效，Get Started 通过。

## 3. 命令参考

### 3.1 list：列表（别名 `li`；全列）

```bash
memos-cli li                # 默认 20 条：编号 + 创建时间 + 标签 + 内容
memos-cli li -n 5           # 只显示 5 条
memos-cli li --page-size 100
memos-cli li --raw          # JSON（API 形状：{"memos": [...]}）
memos-cli li +work          # 筛选（见 §4）
```

实测输出：

```
1	2026-09-20T11:23:50Z	tagA tagB	列验证-测试 #tagA #tagB
2	2026-09-20T07:54:36Z	testtag	test #testtag
3	2026-09-18T05:29:48Z	-	Transaction  life managent
```

列含义：编号（当次拉取 1-based）、创建时间（RFC3339）、标签（API `tags` 字段，空格分隔；无标签显示 `-`）、内容（截断 72）。

**v0.2 变更**：`-f/--full` 已移除（list 本为全列，不再需要开关）。`li -f` 不再报错——`-f` 按 Taskwarrior 语义成为"排除标签 f"的筛选（`-x` 形同 `-tag` 排除），与 TW 未知 `-x` 为排除标签一致。

**编号规则**：第一列是**当次拉取列表**的 1-based 真实编号；筛选只过滤展示、不重排编号。列表变化后编号可能变。

### 3.2 ls：短列表（id、标签、内容三列，list 的子集）

```bash
memos-cli ls                # 编号\t标签\t内容；无标签显示 -
memos-cli ls +work          # 支持与 list 相同的筛选
```

实测输出（ls）：

```
1	tagA tagB	列验证-测试 #tagA #tagB
2	testtag	test #testtag
3	-	Transaction  life managent
```

标签为 API `tags` 字段（空格分隔），无标签显示 `-`；内容截断 40 字符；不含时间列（时间列在 `list`）。

### 3.3 默认命令：查看全文（v0.2 取代原 `get`）

无子命令时的智能分派：

```bash
memos-cli                   # 无参数 = list
memos-cli 3                 # 单个编号 → 查看编号 3 全文
memos-cli <uid>             # 单个 uid（≥20 字符）→ 查看全文
memos-cli <uid> --raw       # JSON 全文
memos-cli +work             # 筛选首参 → list + 筛选
memos-cli -test             # -tag 首参同样有效
memos-cli /正则/            # 正则首参
memos-cli 草稿              # 裸词首参 → list 筛选
```

实测输出（`memos-cli <uid>`）：

```
草稿-会议记录 2026 预算 #work
```

**规则**：纯数字 → 看编号；`memos/` 前缀或 ≥20 字符 → 按 uid 看；其余 → list 筛选。多参数数字（`memos-cli 3 4`）按裸词筛选处理。

### 3.4 edit：编辑（别名 `e`）

```bash
memos-cli e 3
memos-cli e <uid>
```

调 `$EDITOR`（依次尝试 `VISUAL` → `EDITOR` → `vi` → `vim` → `nano`）。编辑后自动 `patch`：
- 有改动：`updated <uid>`
- 无改动：`no change <uid>`

### 3.5 新增（命令名 `add`，别名 `+`；对标 TW `task add`）

```bash
memos-cli add hello world                        # 多词拼接为正文
memos-cli add another test +sometag              # +tag 打标签（无引号，TW 同款语法）
memos-cli add bright +love +work                 # 多标签
memos-cli add 任务 --tag work                    # 兼容 --tag
echo "从 stdin 读" | memos-cli add               # 正文为空时读 stdin
memos-cli + hello                                # 别名 +（v0.1 兼容）
```

实测输出：

```
$ memos-cli add another test +sometag
created	MR2skW3ZVuHfXDBY7vNbu9
$ memos-cli li +sometag
Id	Date	Tag	Description
1	2026-09-20	sometag	another test #sometag
```

**标签为何用 `+` 而非 `#`（v0.2 语法决策）**：usememos 只在正文识别 `#tag` 字面并渲染成标签，但 `#` 在 shell 中是注释符（`memos-cli add 备注 #tag` 实际只传 `备注`，必须引号包裹）。因此命令行用 TW 同款 `+tag`，memos-cli **内部转写为正文 `#tag`**——用户零引号负担、后端渲染正常。正文中要写字面 `#`（如 `issue #123`）时仍按 shell 规则加引号（与 TW 相同约束）。

### 3.6 删除（命令名 `delete`，别名 `-` / `del` / `rm`）

```bash
memos-cli delete 1              # 删除编号 1（默认确认）
memos-cli delete 1 2-3 5        # 单项、范围
memos-cli del 2-3,5 --force     # 逗号列表 + 跳过确认（别名）
memos-cli - <uid> -y            # 按 uid 删除（别名 -）
memos-cli del +draft -y         # 筛选目标：删除全部匹配（全量展开）
memos-cli del /草稿-/           # 正则目标，确认后删除
```

默认流程：逐条预览 → `确认删除？ [y/N]` → 输入 `y` 才删。实测（筛选目标 + 确认）：

```
确认删除 #1 草稿-超市清单 #home ？
确认删除 #2 草稿-会议记录 2026 预算 #work ？
确认删除？ [y/N] deleted	XMUnshytPyRCQGY6YNAbrr
deleted	SgMtPmo6GLE2BaNBnvGE7u
```

**筛选删除安全规则**：筛选零匹配报错（防误删）；`-tag`/`-y` 的 `-` 前缀冲突由解析器先消费 flag 再识别标签；编号/uid 直通解析自当次 page_size 列表，筛选展开用全量。

错误行为（已修复并锁定，回归测试）：

```bash
memos-cli del 0
# memos-cli: 编号 0 超出范围   （退出码非 0，不崩溃；0-2 同理）
```

### 3.7 tags：标签统计（对标 TW `task tags`）

```bash
memos-cli tags                # 全部标签 + 命中数（全量）
memos-cli tags +work          # 受筛选影响
```

实测输出（计数降序、同数字典序）：

```
work	3
functest	2
home	1
```

### 3.8 ids：输出 uid 列表（脚本组合用）

```bash
memos-cli ids +work           # 每行一个 uid（稳定，不受编号变化影响）
```

实测输出：uid 行列表，可直接喂给 `del` / 传给 shell：

```bash
for u in $(memos-cli ids /draft/); do memos-cli $u; done
```

### 3.9 export：全量导出 TSV

```bash
memos-cli export > all.tsv
```

分页拉取全部；每行 `uid \t content \t create_time`；内容内的 `\`、换行、制表符分别转义为 `\\`、`\n`、`\t`。

### 3.10 import：从 stdin 导入 TSV（幂等）

```bash
memos-cli import < all.tsv
```

- 跳过空行与 `uid` 头行；列不足时整行作为正文；反转义 `\n` `\t` `\\`
- **幂等**：uid 已存在 → 输出 `already exists <uid>` 跳过，不重复创建（对标 TW import 按 UUID 更新；memos 无法指定 uid，降级为跳过）
- 单条失败不中断：成功 `imported <uid>`，失败 `error <uid>: 原因`

### 3.11 tui：交互式界面

```bash
memos-cli tui
```

| 按键 | 功能 |
|------|------|
| `j` / `k` / 方向键 | 上 / 下 |
| `g` / `G` | 首 / 末 |
| `h` / `l` / PageUp / PageDown | 上 / 下翻页（每页 20 条） |
| `Space` | 选中 / 取消选中 |
| `d` / `D` | 删除选中 / 删除当前行（弹出 y/N 确认） |
| `Enter` | `$EDITOR` 编辑当前行 |
| `/` | 搜索（本地过滤，Enter 确认 / Esc 取消） |
| `Esc`（非搜索态） | 清空搜索 |
| `r` / `R` | 刷新 |
| `q` / `Ctrl-C` | 退出 |

### 3.12 config：配置管理（别名 `cfg`）

```bash
memos-cli config --base http://127.0.0.1:5230 --token memos_pat_xxx   # 写入/更新
memos-cli config --show            # 展示当前配置（token 脱敏 + 默认筛选）
memos-cli config                   # 不带参数等同 --show
```

### 3.13 version / help / alias

```bash
memos-cli --version        # memos-cli 0.2.0 (built 2026-09-20T11:23:20Z)
memos-cli version          # 同上（子命令形式）
memos-cli --help           # 全部命令（英文）
memos-cli help             # 同 --help
memos-cli help delete      # 指定命令帮助（英文；用完整命令名）
memos-cli help -           # 符号别名也可查
memos-cli --alias          # 命令别名对照表（别名的展示都集中在这里）
```

`--help` 输出（v0.2，英文）：

```
Unofficial usememos CLI + TUI

Usage: memos-cli [OPTIONS] [COMMAND]

Commands:
  list     List memos with all columns
  ls       Short list: id, tag, description
  tags     List all tags with counts
  ids      Output matching memo uids (one per line)
  edit     Edit with $EDITOR
  add      Add a memo (use +tag for tags)
  delete   Delete memos
  export   Export all memos as TSV
  import   Import TSV from stdin
  tui      Interactive TUI mode
  config   View or write configuration
  version  Print version and build time
  help     Show help for a command

Options:
      --base <BASE>    Override server base URL
      --token <TOKEN>  Override access token
      --raw            Raw JSON output (global)
      --no-default     Skip default filters from configuration
      --alias          List command aliases
  -h, --help           Print help
  -V, --version        Print version
```

`memos-cli --alias` 输出：

```
Aliases:
  list:   li
  add:    +
  delete: -, del, rm
  edit:   e
  config: cfg
```

### 3.14 全局参数

```bash
memos-cli --base <url> --token <pat> li      # 覆盖配置
memos-cli li --raw                           # JSON 原始输出（全局 flag）
memos-cli --no-default li                    # 跳过默认筛选
```

`--raw` / `--no-default` 为全局 flag，位置在命令前后均可（`memos-cli 3 --raw` 查看 JSON 全文）。

## 4. 筛选语法

筛选作用于 `list` / `ls` / `tags` / `ids` / 删除目标；多筛选为 AND；带 `time:` 或属性修饰符时可省略 `list` 命令（首参自动分派）。

| 语法 | 含义 | 示例 |
|---|---|---|
| `+tag` | 含标签（只看 API `tags` 字段，不扫正文 `#tag`） | `li +work` |
| `-tag` | 排除标签 | `li -test` |
| `/正则/` | 正文正则（Rust regex；`(?m)` 可多行） | `li /^https/` |
| 裸词 | 正文子串（对标 TW `description.contains`，大小写敏感） | `li 草稿` |
| `time.after:X` | 创建时间在 X 之后 | `li time.after:1w` |
| `time.before:X` | 创建时间在 X 之前 | `li time.before:2026-08-01` |
| `tags.has:X` / `tags.hasnt:X` | 标签含/不含（等价 `+X`/`-X`） | `tags.has:urgent` |
| `content.startswith:X` | 正文前缀 | `content.startswith:https://` |
| `content.endswith:X` | 正文后缀 | `content.endswith:.md` |

**日期语法**（`time.after:` / `time.before:`）：

- 相对量：`N[d|w|m|y]`（含 day/week/month/year 全称；**含义为"距今往前 N"**，如 `time.after:1w` = 最近一周；m=30d、y=365d 精度）
- 命名：`now` `today` `yesterday` `tomorrow` `sow`（周一）`eow`（周五）`som` `eom` `soy` `eoy`
- 绝对：`YYYY-MM-DD`（当日 00:00）、RFC3339 `2026-09-01T08:00:00+08:00`
- 边界不含相等时刻（对齐 TW `due.before`/`due.after` 严格比较）

**限制**：`+tag` / `-tag` 只匹配 API `tags` 字段，正文里的字面 `#tag` 不匹配；删除目标中 <20 字符的短词按裸词筛选（真实 uid 均为 22 字符，不受影响）；`time` 未带操作符（如 `time:1w`）为非法。

## 5. 配置文件格式与容错

文件：`~/.config/memos-cli/env`（或 `$MEMOS_CLI_CONFIG`）：

```
MEMOS_BASE=http://127.0.0.1:5230
MEMOS_TOKEN=memos_pat_xxx
MEMOS_DEFAULT_FILTERS=+work /urgent/     # 可选
```

- 行尾 CRLF、UTF-8 BOM、UTF-16 LE/BE（含 BOM）均可读取
- 无 BOM 的 UTF-16BE 无法自指字节序，会被按 LE 误读（已知限制）
- 写入总是 UTF-8 无 BOM + LF + 0600 权限
- `MEMOS_DEFAULT_FILTERS` 按空白分隔（不支持含空格的筛选词；`+tag`/`/regex/` 无空格，够用）
- 手改文件请用 UTF-8 无 BOM + LF；始终建议优先 `config` 命令

## 6. 故障排查

| 现象 | 处理 |
|---|---|
| 列表为空 / 连接拒绝 | 确认 memos 服务在线、`MEMOS_BASE` 可达；默认试 `http://127.0.0.1:5230` |
| `未设置 MEMOS_TOKEN` | 缺 token：`memos-cli config --token <pat>` 或 `export MEMOS_TOKEN=...` |
| HTTP 404 / 401 | token 无效或过期；网页端重新创建 Access token |
| 删除确认卡住 | 删除默认要交互确认；脚本请加 `-y` |
| 中文配置乱码 | 配置文件编码问题；用 `memos-cli config` 重写 |
| 编辑器未启动 | 设置 `EDITOR`：`export EDITOR=vim` |
| 列表被意外过滤 | 检查 `MEMOS_DEFAULT_FILTERS`；`--no-default li` 对比 |
| `time.after:` 无结果 | 确认日期语法；相对量是"距今往前 N"（未来时间自然无结果） |
| `管道中断` 无报错退出 | SIGPIPE 默认恢复；`| head` 截断为正常 shell 行为 |

## 7. 验收清单（照此走一遍）

- [ ] `memos-cli --version` 输出版本号与编译时间
- [ ] `memos-cli config --show` 显示脱敏 token
- [ ] `memos-cli li -n 3` 输出编号+内容
- [ ] `memos-cli + 验收测试` 返回 `created <uid>`
- [ ] `memos-cli li /验收/` 命中刚创建的一条
- [ ] `memos-cli <uid>` 显示全文（默认命令）
- [ ] `memos-cli ls` 三列（编号/标签/内容）
- [ ] `memos-cli li time.after:1w` 命中近期条目
- [ ] `memos-cli tags` 与 `memos-cli ids +work` 可用
- [ ] `memos-cli del /验收/ -y` 删除成功
- [ ] `memos-cli del 0` 报"编号 0 超出范围"且退出码非 0
- [ ] `memos-cli help delete` 英文帮助
- [ ] `memos-cli --alias` 显示别名对照表
- [ ] `memos-cli tui` 进入界面，`/` 搜索可用，`q` 退出

全部通过即验收完成。