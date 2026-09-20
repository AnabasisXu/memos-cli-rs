# PRD: 无效删除编号处理（invalid-delete-index）

- Slug: `p0-invalid-delete-index`
- 状态: 已实现（2026-09-20）
- 类型: Bug 修复（P0）
- 关联: `docs/test-report-v0.2.0.md` P0-1、`docs/fix-plan-v0.2.0.md` P0-1

## 背景与问题

`memos-cli del 0`（或 `memos-cli - 0`）在 debug 构建下崩溃：

```
thread panicked at src/api.rs:277: attempt to subtract with overflow
```

根因：`parse_delete_targets` 中 `memos.get(n - 1)` 在 `n == 0` 时对 `usize` 执行下溢减法。
- debug 构建（`overflow-checks = on`，`cargo test` 默认）：panic，进程崩溃
- release 构建（`overflow-checks = off`，用户安装的二进制）：回绕为 `usize::MAX`，`get()` 返回 `None`，经 `with_context` 报"编号 0 超出范围"

即同一命令在不同构建模式下行为不一致：一个崩溃、一个报错。这是依赖未定义/未约定行为的隐性缺陷。

范围分支同样存在：`memos-cli del 0-2` 中 `for i in start..=end { memos.get(i - 1) }` 的 `i = 0` 触发相同下溢。

## 目标

1. 编号 0（含范围起始 0）在 debug 与 release 下行为一致：一律返回明确错误"编号 X 超出范围"，退出码非 0，不崩溃
2. 合法编号（1..=N）与既有行为完全不变：正常解析、删除预览、编号映射
3. 以回归测试锁定该契约

## 非目标

- 不改变删除交互流程（确认提示、force 语义）
- 不引入新的错误类型；沿用 `anyhow::Result` 错误链
- 不重构 `parse_delete_targets` 其余逻辑（逗号、前缀、uid 直传等均不动）

## 需求与验收标准

| # | 需求 | 验收 |
|---|---|---|
| R1 | `del 0` 不 panic，报"编号 0 超出范围" | `cargo test` 全绿；`memos-cli del 0` 退出码非 0、stderr 含"超出范围" |
| R2 | `del 0-2` 不 panic，报"编号 0 超出范围" | 同上，输入为范围形式 |
| R3 | debug 与 release 行为一致 | `cargo test` 与 `cargo test --release --lib test_parse_delete_targets_zero` 均通过 |
| R4 | 合法编号行为不变 | 既有 `test_parse_delete_targets`、`_ranges_and_prefix` 通过 |
| R5 | 回归保护 | 红线测试覆盖单编号与范围两分支 |

## 用例

```
$ memos-cli del 0
memos-cli: 编号 0 超出范围
$ memos-cli del 0-2
memos-cli: 编号 0 超出范围
$ memos-cli del 1
（删除预览或直接删除，正常流程）
```

## 方案概述

用 `checked_sub(1)` 替代裸 `- 1`，`None` 时经 `with_context` 报"编号超出范围"：
- `usize::checked_sub` 在 `0 - 1` 时返回 `None`（不 panic、不回绕），与构建模式无关
- 错误消息与既有越界路径（`get()` 返回 `None` 时）一致："编号 X 超出范围"

## 验证状态

- debug 全量: 62 通过 / 0 失败（lib 49 + client_mock 8 + func_help 5）
- release 红线: 通过
- 实现: `src/api.rs` 两处 `get(n - 1)` / `get(i - 1)` → `checked_sub`；测试扩展 `test_parse_delete_targets_zero_must_not_panic` 覆盖 `"0"` 与 `"0-2"`

## 回滚与风险

- 风险: 极低。改动局限在单函数两行索引计算，合法输入路径不变
- 回滚: 还原 `src/api.rs` 中 `checked_sub` 两处即可；测试可保留（修复前红线红、修复后绿，作回归证据）