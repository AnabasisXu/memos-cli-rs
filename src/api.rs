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
    // 逐字节编码：非 ASCII 字符必须按完整 UTF-8 序列百分号编码，
    // 不能用 char as u8（只保留最低字节，中文会被截断成错误码元）。
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

#[derive(Debug, Clone)]
pub enum Filter {
    IncludeTag(String),
    ExcludeTag(String),
    Regex(regex::Regex),
    /// 裸词：正文子串筛选（对标 TW description.contains:word）
    Contains(String),
    /// create_time 在给定 epoch 秒之后（不含相等）
    TimeAfter(i64),
    /// create_time 在给定 epoch 秒之前（不含相等）
    TimeBefore(i64),
    /// tags 含指定标签（等价 +tag）
    TagHas(String),
    /// tags 不含指定标签（等价 -tag）
    TagHasnt(String),
    /// 正文以指定前缀开头
    ContentStartsWith(String),
    /// 正文以指定后缀结尾
    ContentEndsWith(String),
}

impl Filter {
    /// 解析筛选：+tag / -tag / /regex/ / attr.op:value / 裸词；空标记、-数字、未闭合正则返回 Ok(None)
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
        // 属性修饰符：time.after:X / time.before:X（智能日期，见 parse_smart_date）
        // 相对量在 time 筛选语境解释为"距今往前 N"（如 time.after:1w = 一周前至今，create_time 均为过去值）
        if let Some(rest) = arg.strip_prefix("time.after:") {
            let t = parse_time_ref(rest)
                .with_context(|| format!("time.after 日期无效: {rest}"))?;
            return Ok(Some(Filter::TimeAfter(t)));
        }
        if let Some(rest) = arg.strip_prefix("time.before:") {
            let t = parse_time_ref(rest)
                .with_context(|| format!("time.before 日期无效: {rest}"))?;
            return Ok(Some(Filter::TimeBefore(t)));
        }
        // 属性修饰符：tags.has / tags.hasnt / content.startswith / content.endswith
        if let Some(rest) = arg.strip_prefix("tags.has:") {
            if rest.is_empty() {
                bail!("tags.has 值为空");
            }
            return Ok(Some(Filter::TagHas(rest.to_string())));
        }
        if let Some(rest) = arg.strip_prefix("tags.hasnt:") {
            if rest.is_empty() {
                bail!("tags.hasnt 值为空");
            }
            return Ok(Some(Filter::TagHasnt(rest.to_string())));
        }
        if let Some(rest) = arg.strip_prefix("content.startswith:") {
            if rest.is_empty() {
                bail!("content.startswith 值为空");
            }
            return Ok(Some(Filter::ContentStartsWith(rest.to_string())));
        }
        if let Some(rest) = arg.strip_prefix("content.endswith:") {
            if rest.is_empty() {
                bail!("content.endswith 值为空");
            }
            return Ok(Some(Filter::ContentEndsWith(rest.to_string())));
        }
        // 带前缀但未命中的（空 +/空 -/空正则/-数字）不是筛选；否则裸词=子串
        if arg.starts_with('+') || arg.starts_with('-') || arg.starts_with('/') || arg.is_empty() {
            return Ok(None);
        }
        Ok(Some(Filter::Contains(arg.to_string())))
    }

    pub fn matches(&self, memo: &Memo) -> bool {
        match self {
            // ponytail: only API tags[]; usememos fills them from body #tag
            Filter::IncludeTag(tag) => memo.tags.iter().any(|t| t == tag),
            Filter::ExcludeTag(tag) => !memo.tags.iter().any(|t| t == tag),
            Filter::Regex(re) => re.is_match(&memo.content),
            Filter::Contains(word) => memo.content.contains(word),
            Filter::TimeAfter(t) => memo_time_epoch(&memo.create_time)
                .map(|e| e > *t)
                .unwrap_or(false),
            Filter::TimeBefore(t) => memo_time_epoch(&memo.create_time)
                .map(|e| e < *t)
                .unwrap_or(false),
            Filter::TagHas(tag) => memo.tags.iter().any(|t| t == tag),
            Filter::TagHasnt(tag) => !memo.tags.iter().any(|t| t == tag),
            Filter::ContentStartsWith(s) => memo.content.starts_with(s),
            Filter::ContentEndsWith(s) => memo.content.ends_with(s),
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

/// 显式目标判定（编号/范围/逗号列表/uid）：与默认命令分派共用同一规则。
/// 真实 memos uid 为 22 字符 base62，故 ≥20 字符视为 uid；短词不在内 → 落到筛选。
pub fn is_id_target(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if s.starts_with("memos/") || s.len() >= 20 {
        return true;
    }
    // 范围 / 逗号列表：形如 2-3、1,4-7、2-3,5（空段容忍，与解析端一致）
    s.split(',')
        .filter(|p| !p.trim().is_empty())
        .all(|part| {
            part.split('-').all(|x| {
                !x.is_empty() && x.chars().all(|c| c.is_ascii_digit())
            })
        })
}

/// 解析删除目标：显式目标（编号/范围/逗号/uid）解析自 id_memos 的当次列表；
/// 筛选词（+tag/-tag//re//time:/裸词）针对 filter_memos 全量展开；两路合并去重。
/// 全部为筛选且零命中 → 报错防误删。
pub fn parse_delete_targets(
    args: &[String],
    id_memos: &[Memo],
    filter_memos: &[Memo],
) -> Result<Vec<(String, String)>> {
    let mut explicit = Vec::new();
    let mut filter_args = Vec::new();
    for tok in args {
        if is_id_target(tok) {
            explicit.push(tok.clone());
        } else {
            filter_args.push(tok);
        }
    }

    let mut result: Vec<(String, String)> = Vec::new();

    if !filter_args.is_empty() {
        let parsed: Vec<Option<Filter>> = filter_args
            .iter()
            .map(|a| Filter::parse(a))
            .collect::<Result<_>>()?;
        let filters: Vec<Filter> = parsed.into_iter().flatten().collect();
        if filters.is_empty() {
            let joined = filter_args
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            bail!("无法解析筛选: {joined}");
        }
        let hits = apply_filters(filter_memos, &filters);
        if hits.is_empty() {
            let joined = filter_args
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            bail!("筛选无匹配: {joined}");
        }
        for (i, m) in hits {
            result.push((format!("#{i}"), m.uid().to_string()));
        }
    }

    for tok in &explicit {
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
                        // 编号从 1 起；0 及下溢统一报"超出范围"，不依赖 debug/release
                        // 溢出检查差异
                        let idx = i
                            .checked_sub(1)
                            .with_context(|| format!("编号 {i} 超出范围"))?;
                        let memo = id_memos
                            .get(idx)
                            .with_context(|| format!("编号 {i} 超出范围"))?;
                        result.push((i.to_string(), memo.uid().to_string()));
                    }
                    continue;
                }
            }
            if let Ok(n) = part.parse::<usize>() {
                let idx = n
                    .checked_sub(1)
                    .with_context(|| format!("编号 {n} 超出范围"))?;
                let memo = id_memos
                    .get(idx)
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
    // 去重：筛选展开与显式目标可能重叠
    let mut seen = std::collections::HashSet::new();
    result.retain(|(_, uid)| seen.insert(uid.clone()));
    Ok(result)
}

/// 智能日期 → epoch 秒（UTC）。
/// 支持：now/today/yesterday/tomorrow、sow/som/soy、eow/eom/eoy、
///       N[d|w|m|y]（含 day/week/month 拼写；m=30d、y=365d 精度，文档化）、
///       YYYY-MM-DD（当日 00:00 UTC）、RFC3339（YYYY-MM-DDTHH:MM:SS[.fff][Z|±HH:MM]）。
pub fn parse_smart_date(s: &str) -> Result<i64> {
    let s = s.trim();
    let now = now_epoch();
    let today = now - now.rem_euclid(86400); // 今日 00:00 UTC
    match s {
        "now" => return Ok(now),
        "today" => return Ok(today),
        "yesterday" => return Ok(today - 86400),
        "tomorrow" => return Ok(today + 86400),
        "sow" => return Ok(sow(today)),
        "eow" => return Ok(sow(today) + 4 * 86400 + 86399),
        "som" => {
            let (y, m, _) = civil_from_days(today / 86400);
            return Ok(days_from_civil(y, m, 1) * 86400);
        }
        "eom" => {
            let (y, m, _) = civil_from_days(today / 86400);
            let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            return Ok(days_from_civil(ny, nm, 1) * 86400 - 1);
        }
        "soy" => {
            let (y, _, _) = civil_from_days(today / 86400);
            return Ok(days_from_civil(y, 1, 1) * 86400);
        }
        "eoy" => {
            let (y, _, _) = civil_from_days(today / 86400);
            return Ok(days_from_civil(y + 1, 1, 1) * 86400 - 1);
        }
        _ => {}
    }
    // 相对量：N[d|w|m|y]
    if let Some((n, mult)) = parse_duration(s) {
        return Ok(now + n * mult);
    }
    // 绝对日期
    if let Some(t) = memo_time_epoch(s) {
        return Ok(t);
    }
    bail!("无法解析日期: {s}")
}

/// time.* 筛选的日期解析：相对量（1w/3d/2mo/1y）解释为"距今往前 N"
/// （create_time 是过去时间，after:1w 直觉语义为"最近一周"）；其余走 parse_smart_date 原义
fn parse_time_ref(rest: &str) -> Result<i64> {
    if let Some((n, mult)) = parse_duration(rest) {
        return Ok(now_epoch() - n * mult);
    }
    parse_smart_date(rest)
}

/// 相对时长：N[d|w|m|y] 及全称（day/week/month/year 复数）
fn parse_duration(s: &str) -> Option<(i64, i64)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i == bytes.len() {
        return None;
    }
    let n: i64 = s[..i].parse().ok()?;
    let mult = match &s[i..] {
        "d" | "day" | "days" => 86400,
        "w" | "wk" | "wks" | "week" | "weeks" => 7 * 86400,
        "m" | "mo" | "mon" | "month" | "months" => 30 * 86400,
        "y" | "yr" | "yrs" | "year" | "years" => 365 * 86400,
        _ => return None,
    };
    Some((n, mult))
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 本周一 00:00 UTC（周一起始；1970-01-01 为周四）
fn sow(today: i64) -> i64 {
    let days = today / 86400;
    let wd = (days + 4).rem_euclid(7); // 0=周日 … 6=周六
    (days - (wd + 6) % 7) * 86400
}

/// RFC3339 / 纯日期 → epoch 秒（UTC）
fn memo_time_epoch(s: &str) -> Option<i64> {
    let s = s.trim();
    // 纯日期 YYYY-MM-DD → 当日 00:00 UTC
    if s.len() == 10 && s.as_bytes().get(4) == Some(&b'-') && s.as_bytes().get(7) == Some(&b'-') {
        let y: i64 = s[0..4].parse().ok()?;
        let m: u32 = s[5..7].parse().ok()?;
        let d: u32 = s[8..10].parse().ok()?;
        return Some(days_from_civil(y, m, d) * 86400);
    }
    // RFC3339: YYYY-MM-DDTHH:MM:SS[.fff][Z|±HH:MM]
    if s.len() < 19 {
        return None;
    }
    let (date, rest) = s.split_at(10);
    if date.as_bytes().get(4) != Some(&b'-') || date.as_bytes().get(7) != Some(&b'-') {
        return None;
    }
    let y: i64 = date[0..4].parse().ok()?;
    let m: u32 = date[5..7].parse().ok()?;
    let d: u32 = date[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let rest = rest.strip_prefix('T')?;
    if rest.len() < 8 {
        return None;
    }
    let h: u32 = rest[0..2].parse().ok()?;
    let min: u32 = rest[3..5].parse().ok()?;
    let sec: u32 = rest[6..8].parse().ok()?;
    if h > 23 || min > 59 || sec > 60 {
        return None;
    }
    // 跳过小数秒，剩时区
    let tail = &rest[8..];
    let mut tz_start = 0;
    while tz_start < tail.len() && tail.as_bytes()[tz_start].is_ascii_digit() {
        tz_start += 1;
    }
    let tz = &tail[tz_start..];
    let offset: i64 = if tz.is_empty() || tz == "Z" {
        0
    } else if (tz.starts_with('+') || tz.starts_with('-')) && tz.len() == 6 {
        let sign = if tz.starts_with('-') { -1 } else { 1 };
        let oh: i64 = tz[1..3].parse().ok()?;
        let om: i64 = tz[4..6].parse().ok()?;
        sign * (oh * 3600 + om * 60)
    } else {
        return None;
    };
    Some(
        days_from_civil(y, m, d) * 86400
            + (h as i64) * 3600
            + (min as i64) * 60
            + sec as i64
            - offset,
    )
}

/// days-from-civil（Howard Hinnant 算法）：公历日期 → 1970-01-01 起天数
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as i64 + 2) / 5 + (d as i64 - 1);
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// civil-from-days：天数 → (年, 月, 日)
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
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
        assert!(matches!(
            Filter::parse("plain").unwrap(),
            Some(Filter::Contains(w)) if w == "plain"
        ));
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
        // 真实 uid ≥20 字符视为显式目标；短词落入筛选
        let uid_long = "uid-abcdefghijklmnopqrs";
        let t = parse_delete_targets(
            &["1".into(), "3-4".into(), uid_long.into()],
            &memos,
            &memos,
        )
        .unwrap();
        assert_eq!(
            t,
            vec![
                ("1".into(), "u1".into()),
                ("3".into(), "u3".into()),
                ("4".into(), "u4".into()),
                (uid_long.into(), uid_long.into()),
            ]
        );
    }

    #[test]
    fn test_parse_delete_targets_filter_expansion() {
        let memos: Vec<Memo> = (1..=5)
            .map(|i| Memo {
                name: format!("memos/u{i}"),
                content: format!("body{i}"),
                create_time: String::new(),
                tags: if i % 2 == 0 {
                    vec!["work".into()]
                } else {
                    vec![]
                },
                visibility: String::new(),
            })
            .collect();
        // +tag 展开：work 标签 → u2/u4
        let t = parse_delete_targets(&["+work".into()], &memos, &memos).unwrap();
        assert_eq!(
            t,
            vec![("#2".into(), "u2".into()), ("#4".into(), "u4".into())]
        );
        // 裸词筛选 + 编号合并去重（u4 两路都命中）
        let t = parse_delete_targets(
            &["body4".into(), "4".into()],
            &memos,
            &memos,
        )
        .unwrap();
        assert_eq!(t, vec![("#4".into(), "u4".into())]);
        // 筛选零命中 → 报错防误删
        assert!(parse_delete_targets(&["+nonexistent".into()], &memos, &memos).is_err());
    }

    #[test]
    fn test_is_id_target() {
        assert!(is_id_target("3"));
        assert!(is_id_target("2-3"));
        assert!(is_id_target("1,4-7"));
        assert!(is_id_target("memos/u1"));
        assert!(is_id_target("uid-abcdefghijklmnopqrs")); // 22 字符
        assert!(!is_id_target("u5")); // 短词 → 筛选
        assert!(!is_id_target("+work"));
        assert!(!is_id_target("/re/"));
        assert!(!is_id_target("-1"));
        assert!(!is_id_target(""));
    }

    #[test]
    fn test_urlencoding_simple_ascii() {
        assert_eq!(urlencoding_simple("abc123"), "abc123");
        assert_eq!(urlencoding_simple("-_.~"), "-_.~");
        assert_eq!(urlencoding_simple("a b"), "a%20b");
        assert_eq!(urlencoding_simple("/x/y"), "%2Fx%2Fy");
    }

    #[test]
    fn test_urlencoding_simple_multibyte() {
        // 按 RFC 3986：非保留字符按 UTF-8 字节逐字节百分号编码
        assert_eq!(urlencoding_simple("你好"), "%E4%BD%A0%E5%A5%BD");
        assert_eq!(urlencoding_simple("!*'()"), "%21%2A%27%28%29");
    }

    #[test]
    fn test_apply_filters_empty() {
        let memos: Vec<Memo> = (1..=3)
            .map(|i| Memo {
                name: format!("memos/u{i}"),
                content: format!("c{i}"),
                create_time: String::new(),
                tags: vec![],
                visibility: String::new(),
            })
            .collect();
        let r = apply_filters(&memos, &[]);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].0, 1);
        assert_eq!(r[2].0, 3);
    }

    #[test]
    fn test_apply_filters_and_semantics() {
        let memos = vec![
            Memo {
                name: "memos/a".into(),
                content: "meeting notes".into(),
                create_time: String::new(),
                tags: vec!["work".into()],
                visibility: String::new(),
            },
            Memo {
                name: "memos/b".into(),
                content: "meeting invite".into(),
                create_time: String::new(),
                tags: vec!["home".into()],
                visibility: String::new(),
            },
            Memo {
                name: "memos/c".into(),
                content: "nothing".into(),
                create_time: String::new(),
                tags: vec!["work".into()],
                visibility: String::new(),
            },
        ];
        let filters = vec![
            Filter::IncludeTag("work".into()),
            Filter::Regex(regex::Regex::new("meeting").unwrap()),
        ];
        let r = apply_filters(&memos, &filters);
        // 只有同时满足 tag=work 且正文含 meeting 的 a
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, 1);
        assert_eq!(r[0].1.name, "memos/a");
    }

    #[test]
    fn test_apply_filters_no_match() {
        let memos = vec![Memo {
            name: "memos/a".into(),
            content: "x".into(),
            create_time: String::new(),
            tags: vec![],
            visibility: String::new(),
        }];
        let filters = vec![Filter::IncludeTag("nope".into())];
        assert!(apply_filters(&memos, &filters).is_empty());
    }

    #[test]
    fn test_filter_parse_empty_markers() {
        // 空 + / 空 - / 空正则 / 单字符 / → 都不是合法筛选
        assert!(Filter::parse("+").unwrap().is_none());
        assert!(Filter::parse("-").unwrap().is_none());
        assert!(Filter::parse("//").unwrap().is_none());
        assert!(Filter::parse("/").unwrap().is_none());
    }

    #[test]
    fn test_filter_parse_digit_leading_minus() {
        // - 后跟数字视为负数值参数（如 -1），不是排除标签
        assert!(Filter::parse("-1").unwrap().is_none());
        assert!(Filter::parse("-0x").unwrap().is_none());
        assert!(Filter::parse("-1tag").unwrap().is_none());
        // 非数字开头才是排除标签
        assert!(matches!(
            Filter::parse("-abc").unwrap(),
            Some(Filter::ExcludeTag(t)) if t == "abc"
        ));
    }

    #[test]
    fn test_filter_parse_unicode_tag() {
        assert!(matches!(
            Filter::parse("+工作").unwrap(),
            Some(Filter::IncludeTag(t)) if t == "工作"
        ));
    }

    #[test]
    fn test_filter_parse_multiline_regex() {
        // 正则命中多行正文（(?m) 内联多行模式；/re/ 语法不支持尾随标记）
        let re = Filter::parse("/(?m)^AB$/").unwrap().unwrap();
        let m = Memo {
            name: "memos/a".into(),
            content: "AB\nline".into(),
            create_time: String::new(),
            tags: vec![],
            visibility: String::new(),
        };
        assert!(re.matches(&m));
    }

    #[test]
    fn test_filter_parse_time_attr() {
        assert!(matches!(
            Filter::parse("time.after:1w").unwrap(),
            Some(Filter::TimeAfter(_))
        ));
        assert!(matches!(
            Filter::parse("time.before:today").unwrap(),
            Some(Filter::TimeBefore(_))
        ));
        assert!(Filter::parse("time.after:").is_err());
        assert!(Filter::parse("time.after:banana").is_err());
        // 相对量语义：距今往前 N（time.after:1w 的基准在过去）
        if let Some(Filter::TimeAfter(t)) = Filter::parse("time.after:1w").unwrap() {
            assert!(t < parse_smart_date("now").unwrap());
        } else {
            panic!("time.after:1w 应解析为 TimeAfter");
        }
    }

    #[test]
    fn test_parse_smart_date_absolute_and_relative() {
        // 绝对纯日期（2026-09-01 00:00 UTC）
        let d = parse_smart_date("2026-09-01").unwrap();
        assert_eq!(d, days_from_civil(2026, 9, 1) * 86400);
        // RFC3339
        let t = parse_smart_date("2026-09-01T00:00:00Z").unwrap();
        assert_eq!(t, d);
        let t8 = parse_smart_date("2026-09-01T08:00:00+08:00").unwrap();
        assert_eq!(t8, d);
        // 相对量在 now 之后
        let now = parse_smart_date("now").unwrap();
        assert!(parse_smart_date("1w").unwrap() > now);
        assert!(parse_smart_date("3d").unwrap() > now);
        // 命名期初/期末：eow-sow = 4 天 + 23:59:59；som ≤ eom 恒真
        assert_eq!(
            parse_smart_date("eow").unwrap() - parse_smart_date("sow").unwrap(),
            4 * 86400 + 86399
        );
        assert!(
            parse_smart_date("som").unwrap() <= parse_smart_date("eom").unwrap()
        );
        // yesterday < today < tomorrow
        assert!(parse_smart_date("yesterday").unwrap() < parse_smart_date("today").unwrap());
        assert!(parse_smart_date("today").unwrap() < parse_smart_date("tomorrow").unwrap());
        // 非法
        assert!(parse_smart_date("banana").is_err());
        assert!(parse_smart_date("").is_err());
    }

    #[test]
    fn test_filter_attr_modifiers() {
        let m = Memo {
            name: "memos/a".into(),
            content: "https://example.com/x".into(),
            create_time: String::new(),
            tags: vec!["work".into(), "urgent".into()],
            visibility: String::new(),
        };
        let mk = |s: &str| Filter::parse(s).unwrap().unwrap();
        assert!(mk("tags.has:work").matches(&m));
        assert!(!mk("tags.has:nope").matches(&m));
        assert!(mk("tags.hasnt:nope").matches(&m));
        assert!(!mk("tags.hasnt:work").matches(&m));
        assert!(mk("content.startswith:https://").matches(&m));
        assert!(!mk("content.startswith:ftp").matches(&m));
        assert!(mk("content.endswith:/x").matches(&m));
        assert!(!mk("content.endswith:y").matches(&m));
        // 空值报错
        assert!(Filter::parse("tags.has:").is_err());
        assert!(Filter::parse("tags.hasnt:").is_err());
        assert!(Filter::parse("content.startswith:").is_err());
        assert!(Filter::parse("content.endswith:").is_err());
    }

    #[test]
    fn test_filter_time_matches_strict_boundary() {
        let m = Memo {
            name: "memos/a".into(),
            content: "x".into(),
            create_time: "2026-09-18T05:29:48Z".into(),
            tags: vec![],
            visibility: String::new(),
        };
        // 严格不含相等：early → after 命中；same → after 不命中；late → before 命中
        let early = parse_smart_date("2026-09-18T05:29:47Z").unwrap();
        let same = parse_smart_date("2026-09-18T05:29:48Z").unwrap();
        let late = parse_smart_date("2026-09-18T05:29:49Z").unwrap();
        assert!(Filter::TimeAfter(early).matches(&m));
        assert!(!Filter::TimeAfter(same).matches(&m));
        assert!(Filter::TimeBefore(late).matches(&m));
        assert!(!Filter::TimeBefore(same).matches(&m));
    }

    #[test]
    fn test_memo_uid_prefix_variants() {
        // 注意：/memos/x 因 strip 失败只去掉前导斜杠 → memos/x（与 memos/x → x 不一致，记录为已知行为）
        for (name, want) in [
            ("memos/abc-123", "abc-123"),
            ("/memos/abc-123", "memos/abc-123"),
            ("memos/", ""),
            ("/", ""),
            ("abc-123", "abc-123"),
        ] {
            let m = Memo {
                name: name.to_string(),
                content: String::new(),
                create_time: String::new(),
                tags: vec![],
                visibility: String::new(),
            };
            assert_eq!(m.uid(), want, "name={name:?}");
        }
    }

    #[test]
    fn test_flat_content_zero_and_boundary() {
        let empty = Memo {
            name: String::new(),
            content: String::new(),
            create_time: String::new(),
            tags: vec![],
            visibility: String::new(),
        };
        assert_eq!(empty.flat_content(0), "");
        let m = Memo {
            name: String::new(),
            content: "a".repeat(100),
            create_time: String::new(),
            tags: vec![],
            visibility: String::new(),
        };
        assert_eq!(m.flat_content(100).chars().count(), 100);
        assert_eq!(m.flat_content(5).chars().count(), 5);
    }

    #[test]
    fn test_flat_content_unicode_not_split() {
        // 按 char 截断：中文每个字符是完整 char，不产生半个字
        let m = Memo {
            name: String::new(),
            content: "你好世界".to_string(),
            create_time: String::new(),
            tags: vec![],
            visibility: String::new(),
        };
        assert_eq!(m.flat_content(2), "你好");
    }

    #[test]
    fn test_parse_delete_targets_ranges_and_prefix() {
        let memos: Vec<Memo> = (1..=5)
            .map(|i| Memo {
                name: format!("memos/u{i}"),
                content: format!("c{i}"),
                create_time: String::new(),
                tags: vec![],
                visibility: String::new(),
            })
            .collect();
        // 范围 + memos/ 前缀 + 逗号分隔
        let t = parse_delete_targets(&["memos/2-3".into(), "1,4".into()], &memos, &memos).unwrap();
        assert_eq!(
            t,
            vec![
                ("2".into(), "u2".into()),
                ("3".into(), "u3".into()),
                ("1".into(), "u1".into()),
                ("4".into(), "u4".into()),
            ]
        );
        // 反向范围报错
        assert!(parse_delete_targets(&["3-2".into()], &memos, &memos).is_err());
        // 越界报错
        assert!(parse_delete_targets(&["99".into()], &memos, &memos).is_err());
        // 空 token 跳过
        let t = parse_delete_targets(&["1,,3".into()], &memos, &memos).unwrap();
        assert_eq!(t.len(), 2);
        // 空入参 → 报错
        assert!(parse_delete_targets(&[], &memos, &memos).is_err());
    }

    #[test]
    fn test_parse_delete_targets_zero_must_not_panic() {
        // 编号 0 应报"超出范围"错误，而不是下溢 panic（单编号与范围分支都测）
        let memos: Vec<Memo> = std::iter::once(Memo {
            name: "memos/u1".into(),
            content: String::new(),
            create_time: String::new(),
            tags: vec![],
            visibility: String::new(),
        })
        .collect();
        for arg in ["0", "0-2"] {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                parse_delete_targets(&[arg.into()], &memos, &memos)
            }));
            let parsed = res.expect("编号 0 不应 panic（0usize-1 下溢）");
            let err = parsed.expect_err("编号 0 应报超出范围错误");
            assert!(err.to_string().contains("超出范围"), "{arg}: {err}");
        }
    }
}
