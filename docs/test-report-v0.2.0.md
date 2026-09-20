# memos-cli 测试结果报告

日期：2026-09-19
范围：本项目全部已有 + 本次新增测试
命令：`cargo test`（lib + 集成）

## 总结

| 套件 | 通过 | 失败 | 说明 |
|---|---|---|---|
| `src/lib.rs` 单元测试 | 48 | **1** | 唯一失败为真实 bug：删除编号 0 下溢 panic |
| `tests/client_mock.rs` | 8 | 0 | 新增：本机假 HTTP 服务器，不依赖真实 memos |
| `tests/func_help_cmds.rs` | 5 | 0 | 已有：CLI 端到端（依赖本机 memos 服务） |
| **合计** | **61** | **1** | |

当前基线：**61 通过 / 1 失败**（失败项见下，为真实代码缺陷，非测试环境问题）。

## 失败明细

```
api::tests::test_parse_delete_targets_zero_must_not_panic
panicked at src/api.rs:277:26: attempt to subtract with overflow
```

- 触发输入：`parse_delete_targets(&["0"], memos)`
- 根因：`api.rs:277` `memos.get(n - 1)` —— `n == 0` 时 `0usize - 1` 在 debug 构建下溢出 panic，release 构建下回绕成 `usize::MAX` 后取到 `None` 并报"编号 0 超出范围"（panic 仅 debug 可见）。
- 影响：用户执行 `memos-cli del 0`（或 `- 0`）在 debug 构建直接崩溃；release 构建行为不一致（报"超出范围"）。
- 状态：测试已红线锁定，修复见修复计划 P0-1。

## 新增覆盖（本次）

### src/api.rs（+9）
| 测试 | 验证点 |
|---|---|
| `test_urlencoding_simple_ascii` / `_multibyte` | URL 编码保留字符集、中文完整 UTF-8 编码（暴露并修复截断 bug，见下） |
| `test_filter_parse_empty_markers` | `+`/`-`/`//`/`/` 空标记 → 非筛选 |
| `test_filter_parse_digit_leading_minus` | `-1`/`-0x`/`-1tag` 数字开头 → 非排除标签；`-abc` → 排除 |
| `test_filter_parse_unicode_tag` | 中文标签 `+工作` |
| `test_filter_parse_multiline_regex` | `(?m)` 多行正则命中 |
| `test_memo_uid_prefix_variants` | `memos/x`、`/memos/x`、`memos/`、`/`、裸 id 的 uid 提取 |
| `test_flat_content_zero_and_boundary` / `_unicode_not_split` | 0 长度、精确截断、边界不破字 |
| `test_parse_delete_targets_ranges_and_prefix` | 范围、`memos/` 前缀、逗号、空段、反向范围、越界、空入参 |
| `test_parse_delete_targets_zero_must_not_panic` | **0 编号不应 panic（红，真实 bug）** |

### src/config.rs（+19）
| 测试 | 验证点 |
|---|---|
| `test_decode_plain_utf8` / `_empty` / `_utf8_bom_only` / `_utf8_bom_then_invalid` | 各编码基本路径 |
| `test_decode_invalid_utf8_no_nul` | 非法 UTF-8 → Err |
| `test_decode_utf16le_no_bom` / `_utf16be_no_bom_is_le_misread` | 无 BOM UTF-16 兜底行为（记录 BE 被误读为 LE 的已知限制） |
| `test_decode_odd_nul_utf8_passthrough` | 奇数长度含 NUL → UTF-8 原样返回 |
| `test_decode_utf16le_invalid_surrogate_replaced` | 孤立代理 → U+FFFD |
| `test_config_path_env_override` / `_home_default` | `MEMOS_CLI_CONFIG` 优先于 `$HOME` 默认路径 |
| `test_load_defaults_when_nothing_configured` / `_base_default_with_token` | 默认 base `127.0.0.1:5230`、缺 token 报错 |
| `test_load_file_only` / `_env_overrides_file` / `_cli_overrides_env` | 优先级链 cli > env > file |
| `test_write_at_preserves_comments_and_unknown_keys` / `_skips_empty_values` / `_removes_duplicate_key_lines` | 合并写入保留注释/未知键、空值跳过、去重 |

### src/cli/mod.rs（+1）
| 测试 | 验证点 |
|---|---|
| `test_parse_filters_valid_and_invalid` | 合法筛选解析、普通词/空 tag/坏正则报错 |

### tests/client_mock.rs（新文件 +8）
| 测试 | 验证点 |
|---|---|
| `list_sends_path_and_auth_header` | 请求路径 `pageSize`、`Authorization`、`Accept` 头 |
| `list_all_paginates_with_page_token` | 分页循环、`pageToken` 拼接、终止条件 |
| `create_serializes_content_and_visibility` | POST body JSON 字段 |
| `patch_and_delete_use_correct_method_and_path` | PATCH/DELETE 方法与路径 |
| `http_error_surfaces_status_and_body` | 404 错误透出状态码与 body |
| `non_json_success_body_is_parse_error` | 200 + 非 JSON → 解析错误 |
| `empty_body_on_list_is_explicit_error` | 200 + 空 body → "空响应" |
| `list_response_with_items_alias_field_parses` | `items` 别名字段解析（RPC 风格响应） |

## 测试过程中发现并已修复的缺陷

### fixed-1：urlencoding_simple 中文截断（已修复）
- 原实现 `c as u8` 只取 char 低 8 位：`"你好"` → `%60%7D`，正确应为 `%E4%BD%A0%E5%A5%BD`。
- 影响：`list_all` 分页 pageToken 含非 ASCII 时 URL 编码错误，分页失效。
- 修复：按字节遍历 `s.bytes()`，保留字符直通、其余 `%XX` 逐字节编码。
- 测试：`test_urlencoding_simple_multibyte`（红→绿闭环）

## 已知限制（测试记录行为，暂不改动）

1. **UTF-16BE 无 BOM 被按 LE 误读**（`src/config.rs decode_lossy` 无 BOM 兜底部）：
   数据无 BOM 时字节序无法自指，当前按 LE 解码；BE 数据会产生乱码但不 panic。
   有 BOM 的 BE 文件正常（`test_decode_utf16be_bom` 通过）。
2. **`/memos/x` 与 `memos/x` 的 uid 提取不一致**：`/memos/x` 因先 strip `memos/` 失败，
   只去掉前导 `/` → `memos/x`；`memos/x` → `x`。CLI 的 `resolve_id` 会先 strip 前缀再 trim，
   故命令行路径不受影响；直接调 `Memo::uid()` 时行为有差异（测试已标注期望值）。

## 测试质量备注

- 环境变量测试（`config_path`/`load` 优先级）用 `parking_lot::Mutex` 串行化 + 环境还原，避免并行污染。
- 新增 `parking_lot` dev-dependency（仅测试用）。
- mock 集成测试自启动 `TcpListener`，CI 无需真实 memos 即可跑 HTTP 层；`func_help_cmds.rs`
  仍需真实服务（无 token 时自动跳过，见其 `has_token` 逻辑）。