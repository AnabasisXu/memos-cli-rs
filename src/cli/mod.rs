//! CLI 命令：对标 bash memos-cli

use crate::api::{self, Client, Filter, Memo};
use crate::config::Config;
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::io::{self, IsTerminal, Read, Write};

#[derive(Parser, Debug)]
#[command(
    name = "memos-cli",
    version = "0.1.0",
    about = "usememos API 薄封装（非官方）"
)]
pub struct Cli {
    #[arg(long, global = true)]
    pub base: Option<String>,
    #[arg(long, global = true)]
    pub token: Option<String>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 列表（短命令 l / ls）
    #[command(visible_alias = "l", visible_alias = "ls")]
    List {
        #[arg(short = 'f', long)]
        full: bool,
        /// API 拉取条数（默认 20）
        #[arg(long, default_value_t = 20)]
        page_size: u32,
        /// 限制显示条数（默认与 page_size 相同）
        #[arg(short = 'n', long = "limit")]
        limit: Option<u32>,
        #[arg(long)]
        raw: bool,
        /// 筛选：+tag / -tag / /regex/
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        filters: Vec<String>,
    },
    /// 查看正文
    #[command(visible_alias = "show", visible_alias = "cat")]
    Get {
        id: String,
        #[arg(long)]
        raw: bool,
    },
    /// 用 $EDITOR 编辑
    #[command(visible_alias = "e")]
    Edit { id: String },
    /// 新增（正文可多词；+tag 打标签）
    #[command(name = "+")]
    Add {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        words: Vec<String>,
    },
    /// 批量删除
    #[command(name = "-", visible_alias = "del", visible_alias = "delete", visible_alias = "rm")]
    Del {
        #[arg(short = 'y', long = "force")]
        force: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        targets: Vec<String>,
    },
    /// 导出 TSV
    Export,
    /// 从 stdin TSV 导入
    Import,
    /// TUI 模式
    Tui,
}

pub fn run() -> Result<()> {
    // 支持 bash 风格：首参是 +tag / /re/ 时当作 list 筛选
    let mut raw: Vec<String> = std::env::args().collect();
    if raw.len() >= 2 {
        let first = &raw[1];
        if (first.starts_with('+') && first.len() > 1) || (first.starts_with('/') && first.ends_with('/') && first.len() > 2)
        {
            let mut rewritten = vec![raw[0].clone(), "list".into()];
            rewritten.extend(raw.into_iter().skip(1));
            raw = rewritten;
            return run_with_args(raw);
        }
    }
    run_with_args(raw)
}

fn run_with_args(args: Vec<String>) -> Result<()> {
    let cli = Cli::parse_from(args);
    let cfg = Config::load(cli.base.clone(), cli.token.clone())?;
    let client = Client::new(&cfg.base, &cfg.token);

    match cli.command {
        Commands::List {
            full,
            page_size,
            limit,
            raw,
            filters,
        } => cmd_list(&client, page_size, limit, full, raw, &filters),
        Commands::Get { id, raw } => cmd_get(&client, &cfg, &id, raw),
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
    }
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
    full: bool,
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
        println!("{}", serde_json::to_string_pretty(slice)?);
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
    let color = io::stdout().is_terminal();
    for (i, m) in rows {
        let c = m.flat_content(72);
        if full {
            print_row(color, i, Some(&m.create_time), &c);
        } else {
            print_row(color, i, None, &c);
        }
    }
    Ok(())
}

fn print_row(color: bool, i: usize, time: Option<&str>, content: &str) {
    if color {
        print!("\x1b[36m{i}\x1b[0m");
    } else {
        print!("{i}");
    }
    if let Some(t) = time {
        print!("\t{t}");
    }
    println!("\t{content}");
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

fn cmd_get(client: &Client, cfg: &Config, id: &str, raw: bool) -> Result<()> {
    let uid = resolve_id(client, cfg, id)?;
    let memo = client.get(&uid)?;
    if raw {
        println!("{}", serde_json::to_string_pretty(&memo)?);
    } else {
        print!("{}", memo.content);
        if !memo.content.ends_with('\n') {
            println!();
        }
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
    let memos = client.list(cfg.page_size)?;
    let pairs = api::parse_delete_targets(targets, &memos)?;
    if !force {
        for (label, uid) in &pairs {
            let preview = memos
                .iter()
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
        match client.create(&content, "PRIVATE") {
            Ok(m) => println!("imported\t{}", m.uid()),
            Err(e) => eprintln!("error\t{}: {e}", parts[0]),
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn _unused_memo_hint(_: &Memo) {}
