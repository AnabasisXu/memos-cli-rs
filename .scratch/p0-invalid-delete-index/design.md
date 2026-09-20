# 设计文档: 无效删除编号处理

对应 PRD: `.scratch/p0-invalid-delete-index/PRD-v2.0.md`
状态: 已实现（2026-09-20）

## 1. 现状分析

`parse_delete_targets(args, memos)` 将用户输入解析为 `Vec<(编号标签, uid)>` 删除目标，
输入形式：数字、`memos/<uid>`、裸 uid、逗号列表、`a-b` 范围。

索引映射逻辑（修复前 `src/api.rs`）：

```rust
// 范围分支
for i in start..=end {
    let memo = memos.get(i - 1) ...   // i = 0 时下溢
}
// 单编号分支
if let Ok(n) = part.parse::<usize>() {
    let memo = memos.get(n - 1) ...   // n = 0 时下溢
}
```

### 1.1 缺陷机理

- 列表编号从 1 开始（UI 展示 `1..=N`），内存索引从 0 开始，故用 `n - 1` 映射
- `usize` 减法没有下界检查：`0usize - 1`
  - debug（`overflow-checks = true`）：立即 panic
  - release（`overflow-checks = false`）：回绕为 `usize::MAX`，`get` 越界返回 `None`
- 语义上编号 0 是"不存在的位置"，应走既有的"编号超出范围"错误路径

### 1.2 影响面

| 调用点 | 行为 |
|---|---|
| `src/cli/mod.rs` `cmd_delete` | 删除前解析，单/范围编号均受影响 |
| CLI 别名 `del` / `delete` / `rm` / `-` | 同一入口 |
| TUI `do_delete` | 不经过 `parse_delete_targets`（TUI 用 uid 列表），不受影响 |

## 2. 方案选择

### 方案 A: `checked_sub`（采用）

```rust
let idx = n
    .checked_sub(1)
    .with_context(|| format!("编号 {n} 超出范围"))?;
let memo = memos.get(idx).with_context(...)?;
```

- `usize::checked_sub` 是标准库显式溢出安全算子：`0 - 1` → `None`，与构建模式无关
- `with_context` 复用既有错误消息格式"编号 X 超出范围"
- 改动量：单函数两处索引计算，无 API 变更

### 方案 B: 先判断再减

```rust
if n == 0 { bail!("编号 0 超出范围"); }
let idx = n - 1;
```

- 同样消除 panic，但把边界判断内联在业务代码中，语义不如 `checked_sub` 显式
- 需要手动保证与 `get` 越界路径消息一致

### 方案 C: `saturating_sub(1)`

```rust
let idx = n.saturating_sub(1);
```

- `0 - 1` → `0`，会把编号 0 错误映射到第 1 条 memo——**静默删错目标**，否决

### 决策

采用方案 A。理由：
1. 显式表达"0 无合法索引"，错误路径与既有越界一致
2. 不依赖构建配置，debug/release 行为统一（满足 PRD R3）
3. 方案 C 会引入数据破坏风险（删错 memo），明确排除

## 3. 实现

`src/api.rs` `parse_delete_targets` 两处改动：

```rust
// 范围分支（i 为 0 时同样安全）
let idx = i.checked_sub(1).with_context(|| format!("编号 {i} 超出范围"))?;
let memo = memos.get(idx).with_context(|| format!("编号 {i} 超出范围"))?;

// 单编号分支
let idx = n.checked_sub(1).with_context(|| format!("编号 {n} 超出范围"))?;
let memo = memos.get(idx).with_context(|| format!("编号 {n} 超出范围"))?;
```

行为矩阵：

| 输入 | 修复前 debug | 修复前 release | 修复后（两种构建） |
|---|---|---|---|
| `0` | panic | 编号 0 超出范围 | 编号 0 超出范围 |
| `0-2` | panic | 编号 0 超出范围 | 编号 0 超出范围 |
| `1` | 正常 | 正常 | 正常（不变） |
| `99`（越界） | 编号 99 超出范围 | 同左 | 同左（不变） |
| `3-2`（反向） | 范围非法 | 同左 | 同左（不变） |
| `memos/uX` / 裸 uid | 直传 | 同左 | 同左（不变） |

## 4. 测试设计

### 回归红线（修复前红、修复后绿）

`api::tests::test_parse_delete_targets_zero_must_not_panic` 扩展为遍历 `["0", "0-2"]`：
- `catch_unwind` 包裹调用，断言不 panic
- 断言返回 `Err` 且消息含"超出范围"

同时覆盖两个下溢分支（单编号、范围），防止只修一处遗漏另一处。

### 既有测试守护不回归

- `test_parse_delete_targets`：合法编号 + uid 直传
- `test_parse_delete_targets_ranges_and_prefix`：范围、前缀、逗号、反向、越界、空入参

## 5. 验证

```
cargo test          # debug：lib 49 + client_mock 8 + func_help 5 = 62 通过
cargo test --release --lib test_parse_delete_targets_zero   # release：红线通过
```

手工验证（本机 memos）：

```
$ memos-cli del 0
memos-cli: 编号 0 超出范围
$ memos-cli del 0-2
memos-cli: 编号 0 超出范围
```

## 6. 风险与后续

- 已说明改动范围：单函数内，无外部 API/数据结构变更
- 后续可选（非本次范围）：TUI 删除路径无此问题（走 uid），无需改动
- 若将来引入 `u32`/`i32` 外部输入，需在解析层统一做下界校验，`checked_sub` 模式可复用