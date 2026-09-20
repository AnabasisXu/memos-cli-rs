/// 配置模块：管理 memos 服务地址和令牌的加载优先级
/// 命令行参数 > 环境变量 > 配置文件 (~/.config/memos-cli/env)
///
/// 读取对 Windows 编辑产物容错：自动剥离 UTF-8 BOM、解码 UTF-16 LE/BE，
/// 行尾 CRLF 由 trim 处理。写入始终为标准 UTF-8 无 BOM + LF + 0600 权限，
/// 避免 Windows 编辑器（记事本存 UTF-16/ANSI、BOM）污染配置导致解析失效。

use anyhow::{bail, Context, Result};
use std::io::Write as _;
use std::path::PathBuf;

/// 配置来源优先级
#[derive(Debug, Clone)]
pub struct Config {
    pub base: String,
    pub token: String,
    pub page_size: u32,
    /// 常驻默认筛选（对标 TW context 简化版；空白分隔，如 `+work`）
    pub default_filters: Vec<String>,
}

impl Config {
    /// 加载配置：命令行参数覆盖 > 环境变量 > 配置文件
    pub fn load(cli_base: Option<String>, cli_token: Option<String>) -> Result<Self> {
        // 1. 先尝试加载配置文件
        let (file_base, file_token) = Self::load_config_file()?;

        // 2. 环境变量（覆盖配置文件）
        let env_base = std::env::var("MEMOS_BASE").ok();
        let env_token = std::env::var("MEMOS_TOKEN").ok();

        // 3. 优先级：cli > env > file > 默认值
        let base = cli_base
            .or(env_base)
            .or(file_base)
            .unwrap_or_else(|| "http://127.0.0.1:5230".to_string());

        let token = cli_token
            .or(env_token)
            .or(file_token)
            .context("未设置 MEMOS_TOKEN（或用 --token / ~/.config/memos-cli/env）")?;

        let page_size = 20;
        let default_filters = Self::load_default_filters()?;

        Ok(Config {
            base,
            token,
            page_size,
            default_filters,
        })
    }

    /// 解析配置中的默认筛选（MEMOS_DEFAULT_FILTERS，空白分隔）
    pub(crate) fn load_default_filters() -> Result<Vec<String>> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = std::fs::read(&path)
            .with_context(|| format!("读取配置失败: {}", path.display()))?;
        let content = decode_lossy(&bytes)?;
        for line in content.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "MEMOS_DEFAULT_FILTERS" {
                    return Ok(value
                        .trim()
                        .split_whitespace()
                        .map(str::to_string)
                        .collect());
                }
            }
        }
        Ok(Vec::new())
    }

    /// 配置文件绝对路径：$MEMOS_CLI_CONFIG 或 ~/.config/memos-cli/env
    pub fn config_path() -> PathBuf {
        std::env::var("MEMOS_CLI_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
                PathBuf::from(home).join(".config/memos-cli/env")
            })
    }

    /// 从配置文件加载 (base, token)
    pub fn load_config_file() -> Result<(Option<String>, Option<String>)> {
        Self::load_at(&Self::config_path())
    }

    /// 从指定路径解析配置（测试与内部复用；编码容错见 decode_lossy）
    fn load_at(path: &std::path::Path) -> Result<(Option<String>, Option<String>)> {
        if !path.exists() {
            return Ok((None, None));
        }

        let bytes = std::fs::read(path)
            .with_context(|| format!("读取配置失败: {}", path.display()))?;
        let content = decode_lossy(&bytes)
            .with_context(|| format!("配置编码无法识别: {}", path.display()))?;

        let mut base = None;
        let mut token = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "MEMOS_BASE" => base = Some(value.trim().to_string()),
                    "MEMOS_TOKEN" => token = Some(value.trim().to_string()),
                    _ => {}
                }
            }
        }

        Ok((base, token))
    }

    /// 写配置：合并给定字段，其余保留现状；标准 UTF-8 无 BOM + LF + 0600
    pub fn write(base: Option<&str>, token: Option<&str>) -> Result<PathBuf> {
        if base.is_none() && token.is_none() {
            bail!("config 至少需要 --base 或 --token 之一");
        }
        let path = Self::config_path();
        Self::write_at(&path, base, token)?;
        Ok(path)
    }

    /// 写入指定路径（合并更新，只移除本次设置的键行）
    fn write_at(path: &std::path::Path, base: Option<&str>, token: Option<&str>) -> Result<()> {
        // 读取现有配置内容并入，保留用户手写注释/字段
        let mut lines: Vec<String> = if path.exists() {
            let bytes = std::fs::read(path)
                .with_context(|| format!("读取配置失败: {}", path.display()))?;
            decode_lossy(&bytes)
                .with_context(|| format!("配置编码无法识别: {}", path.display()))?
                .lines()
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };

        // 仅移除本次要更新的键的行，其余（注释/手写内容/未更新的键）保留
        lines.retain(|l| {
            let t = l.trim();
            if t.is_empty() || t.starts_with('#') {
                return true;
            }
            let is_base = t.starts_with("MEMOS_BASE=");
            let is_token = t.starts_with("MEMOS_TOKEN=");
            if base.is_some() && is_base {
                return false;
            }
            if token.is_some() && is_token {
                return false;
            }
            true
        });
        if let Some(b) = base {
            if !b.is_empty() {
                lines.push(format!("MEMOS_BASE={}", b.trim()));
            }
        }
        if let Some(t) = token {
            if !t.is_empty() {
                lines.push(format!("MEMOS_TOKEN={}", t.trim()));
            }
        }
        // 规范化换行为 LF，统一结尾
        let content = lines.join("\n") + "\n";

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建配置目录失败: {}", parent.display()))?;
        }
        write_private(path, content.as_bytes())
            .with_context(|| format!("写入配置失败: {}", path.display()))?;

        Ok(())
    }
}

/// 把文本字节按 BOM/编码识别后转成 String，尽量宽容。
/// 支持：UTF-8 无 BOM、UTF-8 BOM、UTF-16 LE/BE（含 BOM）、
/// 以及误读 UTF-8 却含 NUL 的 UTF-16 数据（无 BOM 兜底）。
fn decode_lossy(bytes: &[u8]) -> Result<String> {
    // UTF-8 BOM：直接去掉 EF BB BF
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return String::from_utf8(bytes[3..].to_vec())
            .map_err(|_| anyhow::anyhow!("UTF-8 内容含非法字节"));
    }
    // UTF-16 LE BOM：FF FE
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE
        && bytes.len() >= 4 && bytes[2] == 0x00 && bytes[3] == 0x00
    {
        return decode_utf16_chars(bytes[4..].iter().step_by(2).copied()
            .zip(bytes[5..].iter().step_by(2).copied())
            .map(|(lo, hi)| lo as u16 | ((hi as u16) << 8)));
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE
        && !(bytes.len() >= 4 && bytes[2] == 0x00 && bytes[3] == 0x00)
    {
        return decode_utf16_chars(bytes[2..].iter().step_by(2).copied()
            .zip(bytes[3..].iter().step_by(2).copied())
            .map(|(lo, hi)| lo as u16 | ((hi as u16) << 8)));
    }
    // UTF-16 BE BOM：FE FF
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        return decode_utf16_chars(bytes[2..].iter().step_by(2).copied()
            .zip(bytes[3..].iter().step_by(2).copied())
            .map(|(hi, lo)| (hi as u16) << 8 | lo as u16));
    }
    // 无 BOM：先按 UTF-8；若解析失败或含大量 NUL，按 UTF-16LE 兜底
    match String::from_utf8(bytes.to_vec()) {
        Ok(s) if !s.contains('\u{0}') => Ok(s),
        _ => {
            // 偶数长度且出现 NUL → 大概率是 UTF-16LE 无 BOM
            if bytes.len() % 2 == 0 && bytes.chunks(2).any(|c| c[0] == 0 || c[1] == 0) {
                decode_utf16_chars(bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])))
            } else {
                String::from_utf8(bytes.to_vec()).map_err(|e| anyhow::anyhow!("unknown encoding: {e}"))
            }
        }
    }
}

/// 从 u16 迭代器解码为 String（用 std::char::decode_utf16，替换非法项）
fn decode_utf16_chars<I: IntoIterator<Item = u16>>(units: I) -> Result<String> {
    let mut s = String::new();
    for r in std::char::decode_utf16(units) {
        match r {
            Ok(c) => s.push(c),
            Err(_) => s.push('\u{FFFD}'),
        }
    }
    Ok(s)
}

/// 写入文件并尽量设 0600（Windows 上 chmod 静默忽略）
fn write_private(path: &std::path::Path, content: &[u8]) -> Result<()> {
    // 不用原子写：Windows 上替换已存在文件可能因权限复制问题失败，
    // 直接 truncate 写入，配合 0600 已满足"不暴露 token"目标。
    let mut f = std::fs::File::create(path)?;
    f.write_all(content)?;
    f.sync_all().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_load_at_missing() {
        let p = Path::new("/tmp/nonexistent-memos-env-xyz");
        let (base, token) = Config::load_at(p).unwrap();
        assert!(base.is_none());
        assert!(token.is_none());
    }

    #[test]
    fn test_decode_utf8_bom() {
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice(b"MEMOS_BASE=http://h:5230\n");
        assert_eq!(decode_lossy(&b).unwrap(), "MEMOS_BASE=http://h:5230\n");
    }

    #[test]
    fn test_decode_utf16le_bom() {
        let text = "MEMOS_BASE=http://h:5230\nMEMOS_TOKEN=abc\n";
        let mut b = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode_lossy(&b).unwrap(), text);
    }

    #[test]
    fn test_decode_utf16be_bom() {
        let text = "MEMOS_TOKEN=xyz\n";
        let mut b = vec![0xFE, 0xFF];
        for u in text.encode_utf16() {
            b.extend_from_slice(&u.to_be_bytes());
        }
        assert_eq!(decode_lossy(&b).unwrap(), text);
    }

    #[test]
    fn test_parse_crlf_and_bom() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path();

        let mut content = vec![0xEF, 0xBB, 0xBF];
        content.extend_from_slice(b"MEMOS_BASE=http://srv:5230\r\nMEMOS_TOKEN=secret\r\n");
        std::fs::write(path, &content).unwrap();

        let (base, token) = Config::load_at(path).unwrap();
        assert_eq!(base.as_deref(), Some("http://srv:5230"));
        assert_eq!(token.as_deref(), Some("secret"));
    }

    #[test]
    fn test_write_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("env");

        Config::write_at(&path, Some("http://w:5230"), Some("tok123")).unwrap();
        let (base, token) = Config::load_at(&path).unwrap();
        assert_eq!(base.as_deref(), Some("http://w:5230"));
        assert_eq!(token.as_deref(), Some("tok123"));

        // 只更新 token，base 保留
        Config::write_at(&path, None, Some("tok456")).unwrap();
        let (base, token) = Config::load_at(&path).unwrap();
        assert_eq!(base.as_deref(), Some("http://w:5230"));
        assert_eq!(token.as_deref(), Some("tok456"));
    }

    #[test]
    fn test_write_utf8_nobom_lf() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("env");
        Config::write_at(&path, Some("http://x"), Some("tok")).unwrap();
        let raw = std::fs::read(&path).unwrap();
        // 无 UTF-8 BOM
        assert!(raw.len() < 3 || raw[0] != 0xEF);
        let s = String::from_utf8(raw).unwrap();
        assert!(!s.contains('\r'), "不应出现 CRLF");
        assert!(s.ends_with('\n'));
    }

    // ---- decode_lossy 更多编码变体 ----

    fn utf16le_bytes(s: &str) -> Vec<u8> {
        let mut b = Vec::new();
        for u in s.encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        b
    }

    fn utf16be_bytes(s: &str) -> Vec<u8> {
        let mut b = Vec::new();
        for u in s.encode_utf16() {
            b.extend_from_slice(&u.to_be_bytes());
        }
        b
    }

    #[test]
    fn test_decode_plain_utf8() {
        assert_eq!(decode_lossy(b"plain bytes").unwrap(), "plain bytes");
    }

    #[test]
    fn test_decode_empty() {
        assert_eq!(decode_lossy(b"").unwrap(), "");
    }

    #[test]
    fn test_decode_utf8_bom_only() {
        assert_eq!(decode_lossy(&[0xEF, 0xBB, 0xBF]).unwrap(), "");
    }

    #[test]
    fn test_decode_utf8_bom_then_invalid() {
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.push(0xFF); // 非法 UTF-8 字节
        assert!(decode_lossy(&b).is_err());
    }

    #[test]
    fn test_decode_invalid_utf8_no_nul() {
        // 0xFF 非法、非 BOM 起始、无 NUL 特征 → 兜底也失败 → Err
        assert!(decode_lossy(&[0x41, 0xFF]).is_err());
        assert!(decode_lossy(&[0xC0, 0xAF]).is_err()); // overlong 编码
    }

    #[test]
    fn test_decode_utf16le_no_bom() {
        // 纯 ASCII 数据以 UTF-16LE 编码：每字节对含一个 0x00 NUL → 触兜底
        let b = utf16le_bytes("MEMOS_BASE=http://h:5230\n");
        assert_eq!(decode_lossy(&b).unwrap(), "MEMOS_BASE=http://h:5230\n");
    }

    #[test]
    fn test_decode_utf16be_no_bom_is_le_misread() {
        // 无 BOM 时无法区分字节序；当前实现按 LE 兜底，BE 数据被错解成不同字符。
        // 契约：不 panic、返回 Ok，但结果不等于原文（记录 misread 行为）。
        let b = utf16be_bytes("Aa");
        let out = decode_lossy(&b).unwrap();
        assert_ne!(out, "Aa", "BE 无 BOM 数据应按 LE 误读，而非原样返回");
    }

    #[test]
    fn test_decode_odd_nul_utf8_passthrough() {
        // 含 NUL 但长度奇数：不满足 UTF-16LE 偶数条件 → 按 UTF-8 原样返回
        assert_eq!(decode_lossy(b"a\x00b").unwrap(), "a\u{0}b");
    }

    #[test]
    fn test_decode_utf16le_invalid_surrogate_replaced() {
        // 孤立高代理 → U+FFFD
        let mut b = vec![0xFF, 0xFE];
        b.extend_from_slice(&0xD800u16.to_le_bytes());
        assert_eq!(decode_lossy(&b).unwrap(), "\u{FFFD}");
    }

    // ---- config_path / 环境变量 ----
    // 环境变量是进程全局：这些测试串行执行（static Mutex），结束恢复。

    static ENV_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    #[test]
    fn test_config_path_env_override() {
        let _g = ENV_LOCK.lock();
        let prev = std::env::var("MEMOS_CLI_CONFIG").ok();
        std::env::set_var("MEMOS_CLI_CONFIG", "/custom/path/env");
        assert_eq!(Config::config_path(), PathBuf::from("/custom/path/env"));
        match prev {
            Some(v) => std::env::set_var("MEMOS_CLI_CONFIG", v),
            None => std::env::remove_var("MEMOS_CLI_CONFIG"),
        }
    }

    #[test]
    fn test_config_path_home_default() {
        let _g = ENV_LOCK.lock();
        let prev_home = std::env::var("HOME").ok();
        let prev_cfg = std::env::var("MEMOS_CLI_CONFIG").ok();
        std::env::remove_var("MEMOS_CLI_CONFIG");
        std::env::set_var("HOME", "/home/tester");
        assert_eq!(
            Config::config_path(),
            PathBuf::from("/home/tester/.config/memos-cli/env")
        );
        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        match prev_cfg {
            Some(v) => std::env::set_var("MEMOS_CLI_CONFIG", v),
            None => std::env::remove_var("MEMOS_CLI_CONFIG"),
        }
    }

    // ---- load 优先级：cli > env > file > 默认 ----

    fn with_env_clean<F: FnOnce()>(f: F) {
        let _g = ENV_LOCK.lock();
        // 记录并清空，由测试内自行设置
        let vars = [
            "MEMOS_CLI_CONFIG",
            "MEMOS_BASE",
            "MEMOS_TOKEN",
            "HOME",
        ]
        .map(|k| (k, std::env::var(k).ok()));
        for (k, _) in &vars {
            std::env::remove_var(k);
        }
        f();
        for (k, v) in vars {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }

    #[test]
    fn test_load_defaults_when_nothing_configured() {
        with_env_clean(|| {
            let err = Config::load(None, None).unwrap_err();
            let msg = format!("{err:#}");
            assert!(msg.contains("未设置 MEMOS_TOKEN"), "{msg}");
        });
    }

    #[test]
    fn test_load_file_only() {
        with_env_clean(|| {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("env");
            std::env::set_var("MEMOS_CLI_CONFIG", &path);
            std::fs::write(&path, "MEMOS_BASE=http://file:5230\nMEMOS_TOKEN=filetok\n").unwrap();
            let c = Config::load(None, None).unwrap();
            assert_eq!(c.base, "http://file:5230");
            assert_eq!(c.token, "filetok");
            assert!(c.default_filters.is_empty());
        });
    }

    #[test]
    fn test_load_default_filters_parsing() {
        with_env_clean(|| {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("env");
            std::env::set_var("MEMOS_CLI_CONFIG", &path);
            std::fs::write(
                &path,
                "MEMOS_BASE=http://x:5230\nMEMOS_TOKEN=t\nMEMOS_DEFAULT_FILTERS=+work /urgent/ -test\n",
            )
            .unwrap();
            let c = Config::load(None, None).unwrap();
            assert_eq!(c.default_filters, vec!["+work", "/urgent/", "-test"]);
        });
    }

    #[test]
    fn test_load_default_filters_missing_or_empty() {
        with_env_clean(|| {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("env");
            std::env::set_var("MEMOS_CLI_CONFIG", &path);
            // 无该行 → 空
            std::fs::write(&path, "MEMOS_BASE=http://x:5230\nMEMOS_TOKEN=t\n").unwrap();
            assert!(Config::load(None, None).unwrap().default_filters.is_empty());
            // 值为空 → 空
            std::fs::write(
                &path,
                "MEMOS_BASE=http://x:5230\nMEMOS_TOKEN=t\nMEMOS_DEFAULT_FILTERS=\n",
            )
            .unwrap();
            assert!(Config::load(None, None).unwrap().default_filters.is_empty());
        });
    }

    #[test]
    fn test_load_env_overrides_file() {
        with_env_clean(|| {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("env");
            std::env::set_var("MEMOS_CLI_CONFIG", &path);
            std::env::set_var("MEMOS_BASE", "http://env:5230");
            std::env::set_var("MEMOS_TOKEN", "envtok");
            std::fs::write(&path, "MEMOS_BASE=http://file:5230\nMEMOS_TOKEN=filetok\n").unwrap();
            let c = Config::load(None, None).unwrap();
            assert_eq!(c.base, "http://env:5230");
            assert_eq!(c.token, "envtok");
        });
    }

    #[test]
    fn test_load_cli_overrides_env() {
        with_env_clean(|| {
            std::env::set_var("MEMOS_BASE", "http://env:5230");
            std::env::set_var("MEMOS_TOKEN", "envtok");
            let c = Config::load(Some("http://cli:5230".into()), Some("clitok".into())).unwrap();
            assert_eq!(c.base, "http://cli:5230");
            assert_eq!(c.token, "clitok");
        });
    }

    #[test]
    fn test_load_base_default_with_token() {
        with_env_clean(|| {
            std::env::set_var("MEMOS_TOKEN", "tok");
            let c = Config::load(None, None).unwrap();
            assert_eq!(c.base, "http://127.0.0.1:5230");
            assert_eq!(c.token, "tok");
            assert_eq!(c.page_size, 20);
        });
    }

    // ---- write_at：合并保留注释/未知键 ----

    #[test]
    fn test_write_at_preserves_comments_and_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("env");
        std::fs::write(
            &path,
            "# 手写注释\nFOO=bar\nMEMOS_BASE=http://old:1\nMEMOS_TOKEN=oldtok\n",
        )
        .unwrap();
        Config::write_at(&path, Some("http://new:1"), None).unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains("# 手写注释"));
        assert!(out.contains("FOO=bar"));
        assert!(out.contains("MEMOS_BASE=http://new:1"));
        assert!(!out.contains("http://old"));
        assert!(out.contains("MEMOS_TOKEN=oldtok"), "{out}"); // token 未更新 → 保留旧行
    }

    #[test]
    fn test_write_at_skips_empty_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("env");
        Config::write_at(&path, Some(""), Some("tok")).unwrap();
        std::env::set_var("MEMOS_CLI_CONFIG", &path);
        let (base, token) = Config::load_at(&path).unwrap();
        std::env::remove_var("MEMOS_CLI_CONFIG");
        assert_eq!(base, None);
        assert_eq!(token.as_deref(), Some("tok"));
    }

    #[test]
    fn test_write_at_removes_duplicate_key_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("env");
        std::fs::write(&path, "MEMOS_BASE=http://a\nMEMOS_BASE=http://b\n").unwrap();
        Config::write_at(&path, Some("http://c"), None).unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert_eq!(out.matches("MEMOS_BASE=").count(), 1, "{out}");
        assert!(out.contains("http://c"));
    }
}