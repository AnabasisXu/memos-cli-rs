# Memos 部署与数据流

本文档记录本机 Memos 服务的搭建方式和数据流，基于实际环境勘察。

## 1. 数据流程图

```mermaid
flowchart TB
    subgraph CLI["客户端层"]
        mmc["mmc (memos-cli-rs)\nRust CLI/TUI\nREST 客户端"]
        mms["mms (fish 函数)\n直接读 SQLite"]
        other["HTTP 客户端\n(浏览器 / 其他)}\nHTTP 直连公网"]
    end

    subgraph MEMOS["Memos 服务 (v0.30.0)"]
        API["REST API\n:5230"]
        MEM("memos 进程\n/opt/memos/memos")
    end

    subgraph DATA["数据层 (SQLite)"]
        DB["memos_prod.db\n(memo/user/attachment 等表)"]
        WAL["WAL / SHM"]
    end

    mmc --|"GET/POST/PATCH/DELETE /api/v1/*\n(HTTP 明文 + Bearer token)"| API
    other --|"HTTP :5230"| API
    API --> MEM
    MEM --> DB
    DB <--> WAL
    mms --|"sqlite3 直读\n/opt/memos/data/memos_prod.db"| DB

    classDef cli fill:#d1e7dd,stroke:#0f5132
    classDef memos fill:#cfe2ff,stroke:#084298
    classDef data fill:#fff3cd,stroke:#664d03
    class mmc,mms,other cli
    class API,MEM memos
    class DB,WAL data
```

## 2. 部署形态：静态 Go 二进制 + systemd

非 Docker，由官网发布包 + systemd 单元拉起。

### 安装布局（`/opt/memos/`）

| 路径 | 说明 |
|---|---|
| `memos` | 59 MB 静态链接 Go 可执行文件（ELF x86-64），Memos **v0.30.0** |
| `memos.tar.gz` | 发布包（内含单个 `memos` 二进制） |
| `data/` | 数据目录，属主 `root:root`，权限 700 |
| `data/memos_prod.db` | 生产 SQLite 库（memo/user/attachment/reaction 等表） |
| `data/memos.db` | 空文件 |
| `data/*.db-wal` / `*.db-shm` | SQLite WAL 模式的预写日志 / 共享内存文件 |

### systemd 单元（`/etc/systemd/system/memos.service`）

```ini
[Service]
Type=simple
User=root
WorkingDirectory=/opt/memos
ExecStart=/opt/memos/memos --port 5230 --data /opt/memos/data --instance-url http://101.42.65.87:5230
Restart=on-failure
RestartSec=5
LimitNOFILE=65535
```

- `systemctl is-enabled` = **enabled**，开机自启
- 崩溃后 5 秒自动重启
- 当前 active/running

### 关键配置要点

- `--port 5230`：监听端口
- `--data /opt/memos/data`：SQLite 数据目录
- `--instance-url http://101.42.65.87:5230`：声明实例公网地址（分享链接/通知场景）
- 数据库驱动默认 `sqlite`，未设 DSN
- 未启用 `--demo`；`--addr` 默认 → 监听 `*:5230`
- **无 nginx 反向代理、无 TLS/HTTPS 证书**，HTTP 明文，公网直连 `101.42.65.87:5230`

## 3. 客户端接入

| 客户端 | 方式 | 目标 |
|---|---|---|
| `mmc`（memos-cli-rs） | REST API（默认 `http://127.0.0.1:5230`） | Memos API |
| `mms`（fish 函数） | `sqlite3` 直读 | `memos_prod.db` |
| 其他/浏览器 | HTTP 公网直连 | 5230 端口 |

两条读取路径共用同一实例、同一份数据。

## 4. mmc 与 API 交互细节

`mmc` = `/root/cleantest/memos-cli/memos-cli`（`memos-cli-rs` 项目），usememos REST 客户端，不碰数据库。

### 配置来源优先级（`config.rs`）

**命令行 `--base`/`--token` > 环境变量 `MEMOS_BASE`/`MEMOS_TOKEN` > 配置文件 `~/.config/memos-cli/env` > 默认**（`http://127.0.0.1:5230`）。token 必填。

### HTTP 客户端（`api.rs`）

`reqwest::blocking`（同步阻塞），30s 超时，统一封装：

```rust
url      = base + path
Header   : Authorization: Bearer <token>
           Accept: application/json
          (+ Content-Type: application/json, body 时)
```

非 2xx → 报 `HTTP <code> <method> <path>: <body前200字符>`；空响应 → `None`；否则解析 JSON。

### 命令 ↔ API 对照

| 命令 | 方法 | 路径 | 说明 |
|---|---|---|---|
| `list` | GET | `/api/v1/memos?pageSize=N` | 默认 20；`limit` 取 `max(limit, page_size)` |
| `list_all`（export） | GET | `/api/v1/memos?pageSize=100&pageToken=…` | 分页到 `nextPageToken` 空 |
| `get` | GET | `/api/v1/memos/{uid}` | 查看单条 |
| `add` / `+` | POST | `/api/v1/memos` | body `{"content","visibility":"PRIVATE"}` |
| `edit` / `e` | PATCH | `/api/v1/memos/{uid}` | body `{"content"}` |
| `del` / `-` | DELETE | `/api/v1/memos/{uid}` | 逐个删 |

### 关键实现点

- **响应结构**：list 返回 `{"memos":[…], "nextPageToken":…}`（兼容 `items` 别名）。`Memo.name` 形如 `memos/<uid>`。
- **id 解析**（`resolve_id`）：数字 → `list(page_size=20)` 拉一页按第 N 个取 uid；`memos/xxx` 或 uid 直走 `/api/v1/memos/{uid}`。`get 3` 只在前 20 条内定位。
- **标签筛选**：`+tag`/`-tag` 只看 `memo.tags`（服务端从正文 `#tag` 填充）；`/regex/` 匹配 content。筛选与 `-n` 截断都在本地。
- **编辑**：拉正文 → 临时 `.md` → `$EDITOR` 编辑 → 有改动才 PATCH。
- **删除**：`list` 拉一页解析目标（支持 `1-3`、`memos/uid`），非 `-y` 逐个确认再逐个 DELETE。
- **export/import**：TSV；export 用 `list_all` 全量分页，import 逐行 POST。
- **TUI 模式**：同一 `Client`，ratatui 渲染。

传输为明文 HTTP + Bearer token，无重试逻辑。

## 5. mms 功能说明

`mms` 是 fish 函数，直接查询 `/opt/memos/data/memos_prod.db`，用 `xan view` 渲染表格。

| 命令 | 行为 |
|---|---|
| `mms` | 表格列出所有正常笔记（id、创建时间、内容），长内容按终端宽度截断 |
| `mms -n N` | 最新 N 条 |
| `mms -f` | 完整模式：绕过 xan 截断，逐条完整输出 |
| `mms <id>` | 显示单条 id、创建时间、完整内容 |

要点：`SELECT ... FROM memo WHERE row_status='NORMAL'`；`created_ts` Unix epoch → 本地时间。表格模式 `sqlite3 -csv | xan view`；`-n` 先 `ORDER BY id DESC LIMIT N` 再套子查询升序。