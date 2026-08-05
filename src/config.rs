/// 配置模块：管理 memos 服务地址和令牌的加载优先级
/// 命令行参数 > 环境变量 > 配置文件 (~/.config/memos-cli/env)

use anyhow::{Context, Result};
use std::path::PathBuf;

/// 配置来源优先级
#[derive(Debug, Clone)]
pub struct Config {
    pub base: String,
    pub token: String,
    pub page_size: u32,
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

        Ok(Config { base, token, page_size })
    }

    /// 从 ~/.config/memos-cli/env 加载配置
    fn load_config_file() -> Result<(Option<String>, Option<String>)> {
        let config_path = std::env::var("MEMOS_CLI_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
                PathBuf::from(home).join(".config/memos-cli/env")
            });

        if !config_path.exists() {
            return Ok((None, None));
        }

        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("读取配置失败: {}", config_path.display()))?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config_file() {
        // 使用一个不存在的路径
        std::env::set_var("MEMOS_CLI_CONFIG", "/tmp/nonexistent-memos-env");
        let (base, token) = Config::load_config_file().unwrap();
        assert!(base.is_none());
        assert!(token.is_none());
        std::env::remove_var("MEMOS_CLI_CONFIG");
    }
}