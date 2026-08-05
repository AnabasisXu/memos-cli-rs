//! usememos REST API 客户端（blocking）

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default, alias = "createTime", alias = "createdTs", alias = "create_time")]
    pub create_time: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub visibility: String,
}

impl Memo {
    pub fn uid(&self) -> &str {
        self.name
            .strip_prefix("memos/")
            .unwrap_or(&self.name)
            .trim_start_matches('/')
    }

    pub fn flat_content(&self, max_len: usize) -> String {
        let s: String = self
            .content
            .chars()
            .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
            .collect();
        if s.chars().count() > max_len {
            s.chars().take(max_len).collect()
        } else {
            s
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    #[serde(default, alias = "items")]
    memos: Vec<Memo>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateRequest {
    content: String,
    visibility: String,
}

#[derive(Debug, Serialize)]
struct PatchRequest {
    content: String,
}

#[derive(Clone)]
pub struct Client {
    base: String,
    token: String,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(base: &str, token: &str) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("创建 HTTP 客户端失败");
        Client {
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
            http,
        }
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>> {
        let url = format!("{}{}", self.base, path);
        let mut req = self
            .http
            .request(method.clone(), &url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json");
        if let Some(b) = body {
            req = req.header("Content-Type", "application/json").json(&b);
        }
        let resp = req
            .send()
            .with_context(|| format!("{} {}", method, path))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            bail!(
                "HTTP {} {} {}: {}",
                status,
                method,
                path,
                &text[..text.len().min(200)]
            );
        }
        if text.is_empty() {
            return Ok(None);
        }
        let val: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("解析响应失败: {}", &text[..text.len().min(100)]))?;
        Ok(Some(val))
    }

    pub fn list(&self, page_size: u32) -> Result<Vec<Memo>> {
        let path = format!("/api/v1/memos?pageSize={}", page_size);
        let val = self
            .request(reqwest::Method::GET, &path, None)?
            .context("空响应")?;
        let resp: ListResponse = serde_json::from_value(val)?;
        Ok(resp.memos)
    }

    pub fn list_all(&self) -> Result<Vec<Memo>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut path = "/api/v1/memos?pageSize=100".to_string();
            if let Some(t) = &token {
                path.push_str(&format!("&pageToken={}", urlencoding_simple(t)));
            }
            let val = self
                .request(reqwest::Method::GET, &path, None)?
                .context("空响应")?;
            let resp: ListResponse = serde_json::from_value(val)?;
            out.extend(resp.memos);
            match resp.next_page_token {
                Some(t) if !t.is_empty() => token = Some(t),
                _ => break,
            }
        }
        Ok(out)
    }

    pub fn get(&self, uid: &str) -> Result<Memo> {
        let path = format!("/api/v1/memos/{}", uid);
        let val = self
            .request(reqwest::Method::GET, &path, None)?
            .context("空响应")?;
        Ok(serde_json::from_value(val)?)
    }

    pub fn create(&self, content: &str, visibility: &str) -> Result<Memo> {
        let body = serde_json::to_value(CreateRequest {
            content: content.to_string(),
            visibility: visibility.to_string(),
        })?;
        let val = self
            .request(reqwest::Method::POST, "/api/v1/memos", Some(body))?
            .context("空响应")?;
        Ok(serde_json::from_value(val)?)
    }

    pub fn patch(&self, uid: &str, content: &str) -> Result<()> {
        let path = format!("/api/v1/memos/{}", uid);
        let body = serde_json::to_value(PatchRequest {
            content: content.to_string(),
        })?;
        self.request(reqwest::Method::PATCH, &path, Some(body))?;
        Ok(())
    }

    pub fn delete(&self, uid: &str) -> Result<()> {
        let path = format!("/api/v1/memos/{}", uid);
        self.request(reqwest::Method::DELETE, &path, None)?;
        Ok(())
    }
}

fn urlencoding_simple(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

#[derive(Debug, Clone)]
pub enum Filter {
    IncludeTag(String),
    ExcludeTag(String),
    Regex(regex::Regex),
}

impl Filter {
    /// 解析筛选：+tag / -tag / /regex/；非筛选返回 Ok(None)
    pub fn parse(arg: &str) -> Result<Option<Self>> {
        if let Some(tag) = arg.strip_prefix('+') {
            if !tag.is_empty() {
                return Ok(Some(Filter::IncludeTag(tag.to_string())));
            }
        }
        if let Some(tag) = arg.strip_prefix('-') {
            if !tag.is_empty() && !tag.chars().next().unwrap_or('0').is_ascii_digit() {
                return Ok(Some(Filter::ExcludeTag(tag.to_string())));
            }
        }
        if arg.starts_with('/') && arg.ends_with('/') && arg.len() > 2 {
            let pattern = &arg[1..arg.len() - 1];
            let re = regex::Regex::new(pattern)
                .with_context(|| format!("非法正则: /{pattern}/"))?;
            return Ok(Some(Filter::Regex(re)));
        }
        Ok(None)
    }

    pub fn matches(&self, memo: &Memo) -> bool {
        match self {
            // ponytail: only API tags[]; usememos fills them from body #tag
            Filter::IncludeTag(tag) => memo.tags.iter().any(|t| t == tag),
            Filter::ExcludeTag(tag) => !memo.tags.iter().any(|t| t == tag),
            Filter::Regex(re) => re.is_match(&memo.content),
        }
    }
}

pub fn apply_filters<'a>(memos: &'a [Memo], filters: &[Filter]) -> Vec<(usize, &'a Memo)> {
    memos
        .iter()
        .enumerate()
        .filter(|(_, m)| filters.iter().all(|f| f.matches(m)))
        .map(|(i, m)| (i + 1, m))
        .collect()
}

pub fn parse_delete_targets(args: &[String], memos: &[Memo]) -> Result<Vec<(String, String)>> {
    let mut result = Vec::new();
    for tok in args {
        let tok = tok
            .strip_prefix("memos/")
            .unwrap_or(tok)
            .trim_start_matches('/');
        for part in tok.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((a, b)) = part.split_once('-') {
                if let (Ok(start), Ok(end)) = (a.parse::<usize>(), b.parse::<usize>()) {
                    if end < start {
                        bail!("范围非法: {part}");
                    }
                    for i in start..=end {
                        let memo = memos
                            .get(i - 1)
                            .with_context(|| format!("编号 {i} 超出范围"))?;
                        result.push((i.to_string(), memo.uid().to_string()));
                    }
                    continue;
                }
            }
            if let Ok(n) = part.parse::<usize>() {
                let memo = memos
                    .get(n - 1)
                    .with_context(|| format!("编号 {n} 超出范围"))?;
                result.push((n.to_string(), memo.uid().to_string()));
                continue;
            }
            result.push((part.to_string(), part.to_string()));
        }
    }
    if result.is_empty() {
        bail!("没有可删除的目标");
    }
    Ok(result)
}

pub fn edit_in_editor(old: &str) -> Result<Option<String>> {
    let editor = pick_editor().context("找不到编辑器（设置 $EDITOR 或用 vim/nano）")?;
    let mut tmp = tempfile::Builder::new()
        .suffix(".md")
        .tempfile()
        .context("创建临时文件失败")?;
    use std::io::Write;
    tmp.write_all(old.as_bytes())?;
    tmp.flush()?;
    let path = tmp.path().to_owned();
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{} {}", editor, path.display()))
        .status()
        .context("启动编辑器失败")?;
    if !status.success() {
        bail!("编辑器退出码非零");
    }
    let new = std::fs::read_to_string(&path).context("读取编辑结果失败")?;
    if new == old {
        Ok(None)
    } else {
        Ok(Some(new))
    }
}

fn pick_editor() -> Option<String> {
    for e in [
        std::env::var("VISUAL").ok(),
        std::env::var("EDITOR").ok(),
        Some("vi".to_string()),
        Some("vim".to_string()),
        Some("nano".to_string()),
    ]
    .into_iter()
    .flatten()
    {
        if e.is_empty() || e == "true" || e == ":" || e == "false" {
            continue;
        }
        let cmd = e.split_whitespace().next().unwrap_or(&e);
        if std::process::Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(e);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_parse() {
        assert!(matches!(
            Filter::parse("+work").unwrap(),
            Some(Filter::IncludeTag(t)) if t == "work"
        ));
        assert!(matches!(
            Filter::parse("-test").unwrap(),
            Some(Filter::ExcludeTag(t)) if t == "test"
        ));
        let re = Filter::parse("/hel+o/").unwrap().unwrap();
        assert!(matches!(&re, Filter::Regex(_)));
        let m = Memo {
            name: String::new(),
            content: "helo hello helllo".into(),
            create_time: String::new(),
            tags: vec![],
            visibility: String::new(),
        };
        assert!(re.matches(&m));
        assert!(Filter::parse("/[unclosed/").is_err());
        assert!(Filter::parse("-1").unwrap().is_none());
        assert!(Filter::parse("plain").unwrap().is_none());
    }

    #[test]
    fn tag_filter_matches_tags_only_not_body_hash() {
        let body_only = Memo {
            name: "memos/a".into(),
            content: "note #work".into(),
            create_time: String::new(),
            tags: vec![],
            visibility: String::new(),
        };
        let tagged = Memo {
            name: "memos/b".into(),
            content: "no hash here".into(),
            create_time: String::new(),
            tags: vec!["work".into()],
            visibility: String::new(),
        };
        let inc = Filter::parse("+work").unwrap().unwrap();
        let exc = Filter::parse("-work").unwrap().unwrap();
        assert!(!inc.matches(&body_only));
        assert!(inc.matches(&tagged));
        assert!(exc.matches(&body_only));
        assert!(!exc.matches(&tagged));
    }

    #[test]
    fn test_memo_uid() {
        let m = Memo {
            name: "memos/abc-123".to_string(),
            content: "hello".to_string(),
            create_time: String::new(),
            tags: vec![],
            visibility: String::new(),
        };
        assert_eq!(m.uid(), "abc-123");
    }

    #[test]
    fn test_flat_content() {
        let m = Memo {
            name: String::new(),
            content: "line1\nline2\tline3".to_string(),
            create_time: String::new(),
            tags: vec![],
            visibility: String::new(),
        };
        assert_eq!(m.flat_content(100), "line1 line2 line3");
        assert_eq!(m.flat_content(5).chars().count(), 5);
    }

    #[test]
    fn test_parse_delete_targets() {
        let memos: Vec<Memo> = (1..=5)
            .map(|i| Memo {
                name: format!("memos/u{i}"),
                content: format!("c{i}"),
                create_time: String::new(),
                tags: vec![],
                visibility: String::new(),
            })
            .collect();
        let t = parse_delete_targets(&["1".into(), "3-4".into(), "u5".into()], &memos).unwrap();
        assert_eq!(
            t,
            vec![
                ("1".into(), "u1".into()),
                ("3".into(), "u3".into()),
                ("4".into(), "u4".into()),
                ("u5".into(), "u5".into()),
            ]
        );
    }
}
