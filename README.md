# memos-cli

用 **Rust** 重写的非官方 [usememos](https://github.com/usememos/memos) **CLI 与 TUI**（同一二进制）。  
通过 `Authorization: Bearer` + `/api/v1/memos` 操作笔记，不碰 SQLite。已在 usememos v0.30.x 验证。

单二进制，无 `bash`/`curl`/`jq`/`python` 依赖。

## 安装

### 预编译二进制（Release）

从 [Releases](https://github.com/AnabasisXu/memos-cli-rs/releases) 下载：

| 文件 | 平台 |
|------|------|
| `memos-cli-linux-x86_64` | Linux x86_64 |
| `memos-cli-windows-x86_64.exe` | Windows x86_64 |

```bash
# Linux
chmod +x memos-cli-linux-x86_64
mv memos-cli-linux-x86_64 ~/.local/bin/memos-cli

# Windows：把 memos-cli-windows-x86_64.exe 放到 PATH 目录，可改名为 memos-cli.exe
```

### 源码

```bash
cargo install --path .
# 或
cargo build --release                 # Linux：target/release/memos-cli
# Windows cross（可选）:
# cargo build --release --target x86_64-pc-windows-gnu
# 产物：target/x86_64-pc-windows-gnu/release/memos-cli.exe
```

### 要求

- 预编译包：无需 Rust
- 源码编译：Rust 1.75+（edition 2021）
- 可访问的 usememos 实例 + Access Token

## 配置

优先级（高 → 低）：

1. `--base` / `--token`
2. 环境变量 `MEMOS_BASE` / `MEMOS_TOKEN`
3. `~/.config/memos-cli/env`（可用 `MEMOS_CLI_CONFIG` 改路径）

```bash
# ~/.config/memos-cli/env
MEMOS_BASE=http://127.0.0.1:5230
MEMOS_TOKEN=memos_pat_xxx
```

```bash
chmod 600 ~/.config/memos-cli/env
```

推荐用内置命令写配置（自动标准 UTF-8 无 BOM + LF，Windows 上无需 dos2unix）：

```bash
memos-cli config --base http://127.0.0.1:5230 --token memos_pat_xxx
memos-cli config --show          # 查看当前配置（token 脱敏）
```

读取对 Windows 编辑器产物容错：自动剥离 UTF-8 BOM、解码 UTF-16 LE/BE、容忍 CRLF。
如确需手改 `env` 文件，把编辑器设为 **UTF-8 无 BOM + LF**。始终建议优先 `config` 命令。

令牌：网页端 **设置 → Access tokens** 创建（只显示一次）。

## 用法

```bash
memos-cli l                       # 列表（默认 20 条）：编号 + 内容
memos-cli l -n 5                  # 只显示 5 条
memos-cli l -f                    # 编号 + 时间 + 内容
memos-cli l --page-size 100       # API 多拉一些
memos-cli get 1                   # 正文（别名 show / cat）
memos-cli e 1                     # $EDITOR 编辑（别名 edit）

# 筛选（taskwarrior 风格）
memos-cli l +work                 # 含标签 work（只看 API tags，不扫正文）
memos-cli l -test                 # 排除 test
memos-cli l /hello/               # 正文正则（Rust regex）
memos-cli l +work /urgent/ -test  # 可组合
memos-cli +work                   # 省略 l：首参 +tag 直接筛选
memos-cli /http/                  # 省略 l：正则筛选

# 增删
memos-cli + hello world           # 新增（+ 后须有空格）
memos-cli + bright +love +work    # 正文后 +word 打标签 → 写入 #love #work
memos-cli + 任务 --tag work        # 兼容 --tag
memos-cli - 1                     # 删除（默认确认）
memos-cli - 1 2-3 5 -y            # 批量；-y 跳过确认
memos-cli del 2-3,5 --force       # 别名

memos-cli export > all.tsv        # 全量 TSV
memos-cli import < all.tsv        # 从 TSV 导入
memos-cli tui                     # 交互 TUI
memos-cli --help
```

### 编号

- `l` 第一列是**当次拉取列表**的 1-based 真实编号。
- 筛选只过滤展示，**不重排编号**；`get 11` / `e 12` / `- 11` 可用筛选结果里的序号。
- 列表变化后编号可能变。

### 标签

usememos 从正文 `#tag` 识别标签并填进 API 的 `tags` 字段。`+ word +tag` 会把 `#tag` 追加进正文。  
列表筛选 `+tag` / `-tag` **只匹配 `tags` 字段**，不扫正文里的字面 `#tag`。

### TUI

| 按键 | 功能 |
|------|------|
| `j`/`k` 方向键 | 上下 |
| `g`/`G` | 首/末 |
| `h`/`l` PageUp/Down | 翻页 |
| `Space` | 选中/取消 |
| `d` / `D` | 删选中 / 删当前（确认） |
| `Enter` | `$EDITOR` 编辑 |
| `/` | 搜索；`Esc` 清空 |
| `r` | 刷新 |
| `q` / `Ctrl-C` | 退出 |

## 测试

```bash
cargo test                 # 单元 + 功能（功能测需本机 memos + token）
cargo run -- l -n 3
```


## License

MIT
