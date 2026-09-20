//! CLI 命令：对标 bash memos-cli

use crate::api::{self, Client, Filter, Memo};
use crate::config::Config;
use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use std::io::{self, IsTerminal, Read, Write};

#[derive(Parser, Debug)]
#[command(
    name = "memos-cli",
    version = concat!(env!("CARGO_PKG_VERSION"), " (built ", env!("BUILD_TIME"), ")"),
    about = "Unofficial usememos CLI + TUI",
    disable_help_subcommand = true,
    // 无命令时智能分派（classify_default）兜底，故子命令为可选
    subcommand_required = false,
    arg_required_else_help = false
)]
pub struct Cli {
    /// Override server base URL
    #[arg(long, global = true)]
    pub base: Option<String>,
    /// Override access token
    #[arg(long, global = true)]
    pub token: Option<String>,
    /// Raw JSON output (global)
    #[arg(long, global = true)]
    pub raw: bool,
    /// Skip default filters from configuration
    #[arg(long, global = true)]
    pub no_default: bool,
    /// List command aliases
    #[arg(long)]
    pub alias: bool,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List memos with all columns
    #[command(alias = "li")]
    List {
        /// API fetch size (default 20)
        #[arg(long, default_value_t = 20)]
        page_size: u32,
        /// Limit displayed rows (default: same as page-size)
        #[arg(short = 'n', long = "limit")]
        limit: Option<u32>,
        /// Filters: +tag / -tag / /regex/ / bare word / time.after:X
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        filters: Vec<String>,
    },
    /// Short list: id, tag, description
    Ls {
        /// API fetch size (default 20)
        #[arg(long, default_value_t = 20)]
        page_size: u32,
        /// Limit displayed rows (default: same as page-size)
        #[arg(short = 'n', long = "limit")]
        limit: Option<u32>,
        /// Filters: +tag / -tag / /regex/ / bare word / time.after:X
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        filters: Vec<String>,
    },
    /// List all tags with counts
    Tags {
        /// Filters: +tag / -tag / /regex/ / bare word / time.after:X
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        filters: Vec<String>,
    },
    /// Output matching memo uids (one per line)
    Ids {
        /// Filters: +tag / -tag / /regex/ / bare word / time.after:X
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        filters: Vec<String>,
    },
    /// Edit with $EDITOR
    #[command(alias = "e")]
    Edit { id: String },
    /// Add a memo (use +tag for tags)
    #[command(alias = "+")]
    Add {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        words: Vec<String>,
    },
    /// Delete memos
    #[command(name = "delete", alias = "-", alias = "del", alias = "rm")]
    Del {
        #[arg(short = 'y', long = "force")]
        force: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        targets: Vec<String>,
    },
    /// Export all memos as TSV
    Export,
    /// Import TSV from stdin
    Import,
    /// Interactive TUI mode
    Tui,
    /// View or write configuration
    #[command(alias = "cfg")]
    Config {
        /// Set server base URL
        #[arg(long)]
        base: Option<String>,
        /// Set access token
        #[arg(long)]
        token: Option<String>,
        /// Show effective configuration (token masked)
        #[arg(long)]
        show: bool,
    },
    /// Print version and build time
    Version,
    /// Show help for a command
    Help {
        /// Subcommand name (e.g. list, add, delete, tags)
        #[arg(value_name = "SUBCOMMAND")]
        name: Option<String>,
    },
}

pub fn run() -> Result<()> {
    // Rust std 默认忽略 SIGPIPE 导致 println 遇管道截断 panic（脏输出）；
    // 恢复默认：| head 截断时进程被 SIGPIPE 静默终止（shell 语义），无 panic 信息
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let argv: Vec<String> = std::env::args().collect();
    match classify_default(&argv[1..]) {
        Some(DefaultAction::List) => {
            // bash 风格：首参是 +tag / -tag / /regex/ / 裸词 / time:X 时当作 list 筛选
            let mut v = Vec::with_capacity(argv.len() + 1);
            v.push(argv[0].clone());
            v.push("list".to_string());
            v.extend(argv[1..].iter().cloned());
            run_with_args(v)
        }
        Some(DefaultAction::ShowId(id)) | Some(DefaultAction::ShowUid(id)) => {
            // 无子命令单目标：查看完整信息（原 get 行为）；--raw 只支持参数在目标之后
            let raw = argv.iter().any(|a| a == "--raw");
            show_default(&id, raw)
        }
        Some(DefaultAction::Alias) => {
            print_aliases();
            Ok(())
        }
        None => run_with_args(argv),
    }
}

/// 默认命令分派结果
#[derive(Debug, Clone, PartialEq, Eq)]
enum DefaultAction {
    List,
    ShowId(String),
    ShowUid(String),
    /// `memos-cli --alias`: 打印别名表
    Alias,
}

/// 命令别名表（`memos-cli --alias`）；与 Commands 枚举的 clap alias 定义同步维护
fn print_aliases() {
    println!("Aliases:");
    println!("  list:   li");
    println!("  add:    +");
    println!("  delete: -, del, rm");
    println!("  edit:   e");
    println!("  config: cfg");
}

/// 已知子命令白名单：命中则交给 clap，不重写
fn is_known_command(s: &str) -> bool {
    matches!(
        s,
        "list" | "li" | "ls" | "edit" | "e" | "add" | "+" | "-" | "del" | "delete" | "rm"
            | "tags" | "ids" | "export" | "import" | "tui" | "config" | "cfg" | "version"
            | "help"
    )
}

/// 已知顶层 flag：命中则不重写，避免 `-h` 被误当 `-tag`
fn is_known_flag(s: &str) -> bool {
    matches!(
        s,
        "-h" | "-V" | "--help" | "--version" | "--base" | "--token" | "--raw" | "--no-default"
            | "--alias"
    )
}

/// 无子命令时的智能分派（对标 TW `task <filter>` 隐含默认命令）：
/// 无参 → list；单个纯数字 → 查看编号全文；单个 memos/ 前缀或 ≥20 字符 → 按 uid 查看；
/// 多目标或筛选词 → list+筛选；纯数据 flag（--raw/--no-default）无命令 → list（flag 透传）。
/// `--` 前缀（如 --raw）视为 flag 不计入目标数，故 `<uid> --raw` 仍走全文查看。
fn classify_default(args: &[String]) -> Option<DefaultAction> {
    let Some(first) = args.first() else {
        return Some(DefaultAction::List);
    };
    if is_known_command(first) {
        return None;
    }
    if is_known_flag(first) {
        // `--alias` 单独出现 → 打印别名表（不与其他 flag 混用）；
        // --raw/--no-default → 默认 list；其余 flag 交给 clap 处理
        if args.len() == 1 && first == "--alias" {
            return Some(DefaultAction::Alias);
        }
        if args.iter().all(|a| matches!(a.as_str(), "--raw" | "--no-default")) {
            return Some(DefaultAction::List);
        }
        return None;
    }
    let non_flag = args.iter().filter(|a| !a.starts_with("--")).count();
    if non_flag == 1 {
        if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) {
            return Some(DefaultAction::ShowId(first.clone()));
        }
        if first.starts_with("memos/") || first.len() >= 20 {
            return Some(DefaultAction::ShowUid(first.clone()));
        }
    }
    Some(DefaultAction::List)
}
fn run_with_args(args: Vec<String>) -> Result<()> {
    let cli = Cli::parse_from(args);
    // --alias 已由 classify_default 在"单独出现"时拦截；走到这里说明与其它参数混用 → 报错
    if cli.alias {
        bail!("--alias cannot be combined with other options or commands");
    }
    // 纯 flag 无命令（如 --raw、--no-default、--base X）：默认 list，flag 透传
    let Some(command) = cli.command else {
        let cfg = Config::load(cli.base.clone(), cli.token.clone())?;
        let client = Client::new(&cfg.base, &cfg.token);
        let raw = cli.raw;
        let no_default = cli.no_default;
        return cmd_list(
            &client,
            20,
            None,
            false,
            raw,
            &with_defaults(&cfg, no_default, &[]),
        );
    };
    // version / config / help 不碰网络、不要求 token
    match &command {
        Commands::Version => {
            println!(
                "memos-cli {} (built {})",
                env!("CARGO_PKG_VERSION"),
                env!("BUILD_TIME")
            );
            return Ok(());
        }
        Commands::Config { base, token, show } => {
            return cmd_config(base.clone(), token.clone(), *show);
        }
        Commands::Help { name } => return cmd_help(name.as_deref()),
        _ => {}
    }

    let cfg = Config::load(cli.base.clone(), cli.token.clone())?;
    let client = Client::new(&cfg.base, &cfg.token);
    let raw = cli.raw;
    let no_default = cli.no_default;

    match command {
        Commands::List {
            page_size,
            limit,
            filters,
        } => cmd_list(
            &client,
            page_size,
            limit,
            false,
            raw,
            &with_defaults(&cfg, no_default, &filters),
        ),
        Commands::Ls {
            page_size,
            limit,
            filters,
        } => cmd_list(
            &client,
            page_size,
            limit,
            true,
            raw,
            &with_defaults(&cfg, no_default, &filters),
        ),
        Commands::Tags { filters } => cmd_tags(&client, &with_defaults(&cfg, no_default, &filters)),
        Commands::Ids { filters } => cmd_ids(&client, &with_defaults(&cfg, no_default, &filters)),
        Commands::Edit { id } => cmd_edit(&client, &cfg, &id),
        Commands::Add { words } => cmd_add(&client, &words),
        Commands::Del { force, targets } => {
            let mut force = force;
            let mut clean = Vec::new();
            for t in targets {
                match t.as_str() {
                    "-y" | "--force" => force = true,
                    "--confirm" => force = false,
                    _ => clean.push(t),
                }
            }
            if clean.is_empty() {
                bail!("用法: memos-cli - <目标...> 或 memos-cli del <目标...>");
            }
            cmd_delete(&client, &cfg, &clean, force)
        }
        Commands::Export => cmd_export(&client),
        Commands::Import => cmd_import(&client),
        Commands::Tui => crate::tui::run(&client),
        Commands::Config { .. } => unreachable!(),
        Commands::Version => unreachable!(),
        Commands::Help { .. } => unreachable!(),
    }
}

/// config 子命令：写入或展示配置
fn cmd_config(base: Option<String>, token: Option<String>, show: bool) -> Result<()> {
    let path = Config::config_path();
    let path_str = path.display().to_string();

    if show || (base.is_none() && token.is_none()) {
        println!("配置文件: {}", path_str);
        match path.exists() {
            false => println!("（不存在）"),
            true => {
                // 复用 load_config_file 的容错解析
                match Config::load_config_file() {
                    Ok((b, t)) => {
                        if let Some(b) = b {
                            println!("MEMOS_BASE={}", b);
                        }
                        match t {
                            Some(t) => println!("MEMOS_TOKEN={}", mask(&t)),
                            None => println!("MEMOS_TOKEN=（未设置）"),
                        }
                        match Config::load_default_filters() {
                            Ok(v) if !v.is_empty() => {
                                println!("MEMOS_DEFAULT_FILTERS={}", v.join(" "))
                            }
                            _ => {}
                        }
                    }
                    Err(e) => println!("解析失败: {e:#}"),
                }
            }
        }
        return Ok(());
    }

    let written = Config::write(base.as_deref(), token.as_deref())?;
    println!("已写入配置: {}", written.display());

    // 回读校验
    let (b, t) = Config::load_config_file()?;
    if let Some(t) = &token {
        if t.trim().is_empty() {
            bail!("token 不能为空");
        }
    }
    if b.is_none() && t.is_none() {
        bail!("校验失败: 写入后回读为空");
    }
    println!("校验通过 ✓");
    Ok(())
}

/// 脱敏 token：仅显示前 4 + 后 4 字符
fn mask(t: &str) -> String {
    let chars: Vec<char> = t.chars().collect();
    if chars.len() <= 8 {
        return "****".to_string();
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{}...{}", head, tail)
}

/// 叠加配置默认筛选：--no-default 跳过；否则 cmd 自带筛选排在默认之后（同为 AND）
fn with_defaults(cfg: &Config, no_default: bool, filters: &[String]) -> Vec<String> {
    if no_default {
        return filters.to_vec();
    }
    let mut v = cfg.default_filters.clone();
    v.extend(filters.iter().cloned());
    v
}

fn parse_filters(args: &[String]) -> Result<Vec<Filter>> {
    let mut out = Vec::new();
    for a in args {
        match Filter::parse(a)? {
            Some(f) => out.push(f),
            None => bail!("无法解析筛选: {a}"),
        }
    }
    Ok(out)
}

fn cmd_list(
    client: &Client,
    page_size: u32,
    limit: Option<u32>,
    short: bool,
    raw: bool,
    filter_args: &[String],
) -> Result<()> {
    let fetch = limit.map(|n| n.max(page_size)).unwrap_or(page_size);
    let memos = client.list(fetch)?;
    if raw {
        let show = limit.unwrap_or(page_size) as usize;
        let slice = if memos.len() > show {
            &memos[..show]
        } else {
            &memos[..]
        };
        // bash: jq . on full list body → wrap as API-shaped object
        let body = serde_json::json!({ "memos": slice });
        println!("{}", serde_json::to_string_pretty(&body)?);
        return Ok(());
    }
    let filters = parse_filters(filter_args)?;
    let mut rows = if filters.is_empty() {
        memos
            .iter()
            .enumerate()
            .map(|(i, m)| (i + 1, m))
            .collect::<Vec<_>>()
    } else {
        api::apply_filters(&memos, &filters)
    };
    let show = limit.unwrap_or(page_size) as usize;
    if rows.len() > show {
        rows.truncate(show);
    }
    let tty = io::stdout().is_terminal();
    let mut table: Vec<Vec<String>> = Vec::new();
    if short {
        table.push(vec!["Id".into(), "Tag".into(), "Description".into()]);
    } else {
        table.push(vec![
            "Id".into(),
            "Date".into(),
            "Tag".into(),
            "Description".into(),
        ]);
    }
    for (i, m) in rows {
        let tags = if m.tags.is_empty() {
            "-".to_string()
        } else {
            m.tags.join(" ")
        };
        let id_cell = if tty {
            format!("\x1b[36m{i}\x1b[0m")
        } else {
            i.to_string()
        };
        if short {
            // ls：id、标签、内容三列（list 子集，无时间列）
            table.push(vec![id_cell, tags, m.flat_content(40)]);
        } else {
            // list：全列 = 编号、创建日期、标签、内容
            table.push(vec![id_cell, short_date(&m.create_time).into(), tags, m.flat_content(72)]);
        }
    }
    print_table(&table, tty);
    Ok(())
}

/// 表格渲染：TTY 用空格对齐（末列不 pad）；非 TTY 输出纯净 TSV（tab 分隔，机器可解析）。
/// 列宽按去 ANSI 后的字符数计算，编号着色不影响对齐。
fn print_table(rows: &[Vec<String>], tty: bool) {
    if rows.is_empty() {
        return;
    }
    if tty {
        let cols = rows[0].len();
        let widths: Vec<usize> = (0..cols)
            .map(|c| rows.iter().map(|r| vis_len(&r[c])).max().unwrap_or(0))
            .collect();
        for row in rows {
            let mut line = String::new();
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    line.push_str("  ");
                }
                line.push_str(cell);
                if i + 1 < cols {
                    let pad = widths[i].saturating_sub(vis_len(cell));
                    if pad > 0 {
                        line.push_str(&" ".repeat(pad));
                    }
                }
            }
            println!("{line}");
        }
    } else {
        for row in rows {
            println!("{}", row.join("\t"));
        }
    }
}

/// 去 ANSI 转义序列后的显示字符数（用于对齐宽度）
fn vis_len(s: &str) -> usize {
    let mut n = 0;
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        n += 1;
    }
    n
}

/// RFC3339 创建时间 → 仅日期 YYYY-MM-DD（截取前 10 字符，全部 ASCII 安全）
fn short_date(s: &str) -> &str {
    if s.len() >= 10 {
        &s[..10]
    } else {
        s
    }
}

fn resolve_id(client: &Client, cfg: &Config, id: &str) -> Result<String> {
    let id = id
        .strip_prefix("memos/")
        .unwrap_or(id)
        .trim_start_matches('/');
    if id.parse::<usize>().is_ok() {
        let n: usize = id.parse()?;
        let memos = client.list(cfg.page_size)?;
        let memo = memos
            .get(n - 1)
            .with_context(|| format!("编号 {n} 超出范围"))?;
        Ok(memo.uid().to_string())
    } else {
        Ok(id.to_string())
    }
}

/// 默认命令分派到"查看全文"（原 get 行为）
fn show_default(id: &str, raw: bool) -> Result<()> {
    let cfg = Config::load(None, None)?;
    let client = Client::new(&cfg.base, &cfg.token);
    let uid = resolve_id(&client, &cfg, id)?;
    let memo = client.get(&uid)?;
    print_memo(&memo, raw);
    Ok(())
}

fn print_memo(memo: &api::Memo, raw: bool) {
    if raw {
        println!("{}", serde_json::to_string_pretty(memo).unwrap_or_default());
        return;
    }
    // 完整信息（对标 TW `task <id> information`）：键值字段展示
    let tags = if memo.tags.is_empty() {
        "-".to_string()
    } else {
        memo.tags.join(" ")
    };
    println!("uid: {}", memo.uid());
    println!("date: {}", short_date(&memo.create_time));
    println!("tags: {tags}");
    println!("content: {}", memo.content);
}

/// help 子命令（英文）：无参输出主帮助；带参数输出指定子命令帮助
fn cmd_help(name: Option<&str>) -> Result<()> {
    let mut cmd = Cli::command();
    match name {
        None => cmd.print_help()?,
        Some(n) => match cmd.find_subcommand_mut(n) {
            Some(sc) => sc.print_help()?,
            None => bail!("Unknown subcommand: {n}"),
        },
    }
    Ok(())
}

fn cmd_edit(client: &Client, cfg: &Config, id: &str) -> Result<()> {
    let uid = resolve_id(client, cfg, id)?;
    let memo = client.get(&uid)?;
    match api::edit_in_editor(&memo.content)? {
        Some(new) => {
            client.patch(&uid, &new)?;
            println!("updated\t{uid}");
        }
        None => println!("no change\t{uid}"),
    }
    Ok(())
}

fn cmd_add(client: &Client, words: &[String]) -> Result<()> {
    let mut text = String::new();
    let mut tags: Vec<String> = Vec::new();
    let mut skip_tag = false;
    for a in words {
        if skip_tag {
            tags.push(a.clone());
            skip_tag = false;
            continue;
        }
        if a == "--tag" {
            skip_tag = true;
            continue;
        }
        if let Some(tag) = a.strip_prefix('+') {
            if !tag.is_empty() {
                tags.push(tag.to_string());
                continue;
            }
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(a);
    }
    if text.is_empty() {
        // stdin
        io::stdin().read_to_string(&mut text)?;
        text = text.trim_end().to_string();
    }
    if text.is_empty() {
        bail!("+ 号后需正文（标签用 +word）");
    }
    for t in &tags {
        text.push_str(&format!(" #{t}"));
    }
    let memo = client.create(&text, "PRIVATE")?;
    println!("created\t{}", memo.uid());
    Ok(())
}

fn cmd_delete(client: &Client, cfg: &Config, targets: &[String], force: bool) -> Result<()> {
    // 编号/uid 直通解析自当次 page_size 列表（现状语义）；
    // 含筛选词时额外拉全量用于展开，避免只删 page_size 窗口内的假象
    let id_memos = client.list(cfg.page_size)?;
    let needs_filter = targets.iter().any(|t| !api::is_id_target(t));
    let filter_memos = if needs_filter {
        client.list_all()?
    } else {
        id_memos.clone()
    };
    let pairs = api::parse_delete_targets(targets, &id_memos, &filter_memos)?;
    if !force {
        for (label, uid) in &pairs {
            let preview = id_memos
                .iter()
                .chain(filter_memos.iter())
                .find(|m| m.uid() == uid)
                .map(|m| m.flat_content(40))
                .unwrap_or_else(|| "（无内容）".into());
            println!("确认删除 {label} {preview} ？");
        }
        print!("确认删除？ [y/N] ");
        io::stdout().flush()?;
        let mut ans = String::new();
        io::stdin().read_line(&mut ans)?;
        let ans = ans.trim();
        if ans != "y" && ans != "Y" {
            bail!("已取消");
        }
    }
    for (_, uid) in pairs {
        client.delete(&uid)?;
        println!("deleted\t{uid}");
    }
    Ok(())
}

/// tags：列出全部标签及命中数（受筛选影响；全量拉取）
fn cmd_tags(client: &Client, filter_args: &[String]) -> Result<()> {
    let all = client.list_all()?;
    let filters = parse_filters(filter_args)?;
    let rows: Vec<&api::Memo> = if filters.is_empty() {
        all.iter().collect()
    } else {
        api::apply_filters(&all, &filters)
            .into_iter()
            .map(|(_, m)| m)
            .collect()
    };
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for m in rows {
        for t in &m.tags {
            *counts.entry(t.clone()).or_insert(0) += 1;
        }
    }
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    // 计数降序、同数字典序
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    for (tag, c) in v {
        println!("{tag}\t{c}");
    }
    Ok(())
}

/// ids：输出匹配 memo 的 uid（每行一个，稳定；供脚本组合）
fn cmd_ids(client: &Client, filter_args: &[String]) -> Result<()> {
    let all = client.list_all()?;
    let filters = parse_filters(filter_args)?;
    let rows: Vec<&api::Memo> = if filters.is_empty() {
        all.iter().collect()
    } else {
        api::apply_filters(&all, &filters)
            .into_iter()
            .map(|(_, m)| m)
            .collect()
    };
    for m in rows {
        println!("{}", m.uid());
    }
    Ok(())
}

fn cmd_export(client: &Client) -> Result<()> {
    let memos = client.list_all()?;
    for m in memos {
        // TSV: uid \t content \t create_time — content escapes newlines as \n
        let content = m.content.replace('\\', "\\\\").replace('\n', "\\n").replace('\t', "\\t");
        println!("{}\t{}\t{}", m.uid(), content, m.create_time);
    }
    Ok(())
}

fn cmd_import(client: &Client) -> Result<()> {
    // 幂等：uid 已存在则跳过（对标 TW import 按 UUID 更新；memos 无法指定 uid，降级为跳过）
    let mut existing: std::collections::HashSet<String> = client
        .list_all()?
        .iter()
        .map(|m| m.uid().to_string())
        .collect();
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    for line in input.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.first() == Some(&"uid") {
            continue;
        }
        let content = if parts.len() >= 2 {
            parts[1].replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\")
        } else {
            parts[0].to_string()
        };
        let uid0 = parts[0].trim();
        if !uid0.is_empty() && existing.contains(uid0) {
            println!("already exists\t{uid0}");
            continue;
        }
        match client.create(&content, "PRIVATE") {
            Ok(m) => {
                existing.insert(m.uid().to_string());
                println!("imported\t{}", m.uid());
            }
            Err(e) => eprintln!("error\t{uid0}: {e}"),
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn _unused_memo_hint(_: &Memo) {}

#[cfg(test)]
mod tests {
    use super::mask;
    use super::parse_filters;

    #[test]
    fn test_mask_short() {
        assert_eq!(mask(""), "****");
        assert_eq!(mask("1234"), "****");
        assert_eq!(mask("12345678"), "****");
    }

    #[test]
    fn test_mask_long() {
        assert_eq!(mask("123456789"), "1234...6789");
        assert_eq!(mask("1234567890abcdef"), "1234...cdef");
    }

    #[test]
    fn test_mask_multibyte() {
        // 按 char 计宽，多字节字符不应被截断成半个
        assert_eq!(mask("一二三四五六七八九"), "一二三四...六七八九");
    }

    #[test]
    fn test_parse_filters_valid_and_invalid() {
        let ok = parse_filters(&["+work".into(), "/note/".into(), "plain".into()]).unwrap();
        assert_eq!(ok.len(), 3);
        assert!(parse_filters(&["+".into()]).is_err());
        assert!(parse_filters(&["/[bad/".into()]).is_err());
    }

    #[test]
    fn test_short_date_truncates_to_day() {
        use super::short_date;
        assert_eq!(short_date("2026-09-20T11:23:50Z"), "2026-09-20");
        assert_eq!(short_date("2026-09-20T00:00:00+08:00"), "2026-09-20");
        assert_eq!(short_date("2026-09-20"), "2026-09-20");
        assert_eq!(short_date("x"), "x");
        assert_eq!(short_date(""), "");
    }

    #[test]
    fn test_classify_default() {
        use super::{classify_default, DefaultAction};
        // 无参 → list
        assert_eq!(classify_default(&[]), Some(DefaultAction::List));
        // 纯数字 → 查看编号全文
        assert_eq!(
            classify_default(&["3".into()]),
            Some(DefaultAction::ShowId("3".into()))
        );
        assert_eq!(
            classify_default(&["0".into()]),
            Some(DefaultAction::ShowId("0".into()))
        );
        // 长 uid / memos/ 前缀 → 按 uid 查看
        assert_eq!(
            classify_default(&["i3HeAUTgvAyksy7FkhP6K3".into()]),
            Some(DefaultAction::ShowUid("i3HeAUTgvAyksy7FkhP6K3".into()))
        );
        assert_eq!(
            classify_default(&["memos/abc-123".into()]),
            Some(DefaultAction::ShowUid("memos/abc-123".into()))
        );
        // --flag 不计入目标数：uid + --raw 仍走全文查看
        assert_eq!(
            classify_default(&["i3HeAUTgvAyksy7FkhP6K3".into(), "--raw".into()]),
            Some(DefaultAction::ShowUid("i3HeAUTgvAyksy7FkhP6K3".into()))
        );
        // 短非数字词、筛选词、多参数 → list
        assert_eq!(classify_default(&["u5".into()]), Some(DefaultAction::List));
        assert_eq!(classify_default(&["foo".into()]), Some(DefaultAction::List));
        for f in ["+work", "-work", "/re/", "time.after:1w"] {
            assert_eq!(classify_default(&[f.into()]), Some(DefaultAction::List), "{f}");
        }
        assert_eq!(
            classify_default(&["3".into(), "4".into()]),
            Some(DefaultAction::List)
        );
        // 已知命令 → 不重写
        for cmd in [
            "list", "li", "ls", "edit", "e", "add", "+", "-", "del", "delete", "rm",
            "export", "import", "tui", "config", "cfg", "version", "help",
        ] {
            assert_eq!(classify_default(&[cmd.into()]), None, "{cmd}");
        }
        // 纯数据 flag 无命令 → 默认 list（flag 透传）；帮助/版本/覆盖 flag → clap
        assert_eq!(classify_default(&["--raw".into()]), Some(DefaultAction::List));
        assert_eq!(
            classify_default(&["--no-default".into()]),
            Some(DefaultAction::List)
        );
        assert_eq!(
            classify_default(&["--raw".into(), "--no-default".into()]),
            Some(DefaultAction::List)
        );
        // --alias 单独 → Alias；与其他 flag 混用 → 交 clap（不应混用）
        assert_eq!(classify_default(&["--alias".into()]), Some(DefaultAction::Alias));
        for mixed in [vec!["--alias", "--raw"], vec!["--alias", "--no-default"]] {
            assert_eq!(
                classify_default(&mixed.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
                None,
                "{mixed:?}"
            );
        }
        for flag in ["-h", "-V", "--help", "--version", "--base", "--token"] {
            assert_eq!(classify_default(&[flag.into()]), None, "{flag}");
        }
    }
}
