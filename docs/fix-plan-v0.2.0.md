# memos-cli 修复计划

基于测试结果报告（docs/test-report-v0.2.0.md，2026-09-19）。优先级：P0 崩溃级，P1 功能错误，P2 一致性/加固。

## P0-1：parse_delete_targets 编号 0 下溢 panic（真实的崩溃）

- **状态**：确认失败，测试红线 `api::tests::test_parse_delete_targets_zero_must_not_panic` 锁定
- **位置**：`src/api.rs:277` `memos.get(n - 1)`
- **复现**：`memos-cli del 0` 或 `memos-cli - 0`（debug 构建 panic；release 构建报"编号 0 超出范围"，行为不一致）
- **根因**：`0usize - 1` 在 debug 下溢出 panic；编号 0 规范上应作为"超出范围"错误处理
- **修复方案**（二选一）：
  1. 最小：解析数值成功后先校验 `n == 0 → bail!("编号 0 超出范围")`，再 `get(n - 1)`
  2. 更稳：改 `n.checked_sub(1)`，`None` 则 bail（同时覆盖 0 与将来其他下界）
- **验收**：`cargo test` 红线转绿；`memos-cli - 0` 输出"编号 0 超出范围"且退出码非 0，不 crash
- **风险**：无。行为从"panic/偶发错误"统一为"明确错误"

## P1-1：list_all 分页 pageToken 编码缺陷（已修复，记录留档）

- **状态**：已修复已验证（红→绿闭环完成）
- **位置**：`src/api.rs` `urlencoding_simple`
- **问题**：`c as u8` 截断多字节字符，中文 token 编码错误导致分页 URL 失效
- **后续建议**：若 `reqwest` 升级，可换用其 URL 序列化（当前无外部依赖需求，保持手写最小实现）

## P2-1：/memos/x 与 memos/x 的 uid 提取不一致（一致性加固）

- **状态**：测试已固化当前行为（`docs/test-report-v0.2.0.md` 已知限制 2）
- **位置**：`src/api.rs` `Memo::uid()`
- **问题**：`uid()` 先 `strip_prefix("memos/")` 再 `trim_start_matches('/')`，导致
  `/memos/x` → `memos/x`，`memos/x` → `x`。当前调用链（`resolve_id`）先 strip 前缀再 trim，
  实际不受影响；但作为公开 API，语义不统一是隐患
- **修复方案**（如采纳）：统一先 `trim_start_matches('/')` 再 `strip_prefix("memos/")`，
  即 `self.name.trim_start_matches('/').strip_prefix("memos/").unwrap_or(...)`，使
  `/memos/x` 与 `memos/x` 均 → `x`
- **验收**：更新 `test_memo_uid_prefix_variants` 期望值；`cargo test` 全绿
- **风险**：低。需确认 `resolve_id` 与 `parse_delete_targets` 的调用路径无相反依赖
- **建议**：不阻塞，短期可只记录；若改动需一并审计 `cli` 与 `tui` 的 uid 消费者

## P2-2：UTF-16BE 无 BOM 数据被按 LE 误读（已知限制）

- **状态**：行为已测试记录（`test_decode_utf16be_no_bom_is_le_misread`），暂不改动
- **位置**：`src/config.rs` `decode_lossy` 无 BOM 兜底分支
- **背景**：无 BOM 的 UTF-16 无法自指字节序；当前启发式为"含 NUL 且偶数长度 → LE"。
  BE 无 BOM 文件会解码出乱码（不崩溃）。配置场景中文件由本工具 UTF-8 写出，无 BOM 由外部
  工具生成时两端字节序通常一致，实际触发概率低
- **可选加固**：兜底前检查"字节对高字节为 0x00 的比例"，两者皆高时优先 LE（现状），
  否则维持；或文档明确"配置仅支持 UTF-8 / 带 BOM UTF-16"
- **风险**：改动启发式可能反转当前"恰好正确"的 LE 无 BOM 文件，需回归 `test_decode_utf16le_no_bom`

## 执行顺序建议

1. P0-1（必须）：修复编号 0，红线转绿，`cargo test` 全绿
2. 检查 P2-1 是否影响当前 CLI 流程：跑 `cargo test --test func_help_cmds`（实网）确认
3. 如需统一 uid 语义再动 P2-1；否则合入测试红线留档

## 当前基线（修复前）

- lib：48 过 / 1 失败（失败=P0-1）
- client_mock：8 过
- func_help_cmds：5 过（依赖本机 memos）
- 合计：61 过 / 1 失败