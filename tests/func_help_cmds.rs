//! 覆盖 `memos-cli --help` 列出的全部命令的功能测试。
//! 需要本机 usememos + ~/.config/memos-cli/env（或 MEMOS_TOKEN）。

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_memos-cli"))
}

fn run(args: &[&str]) -> Output {
    bin()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn failed: {e}"))
}

fn run_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = bin()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}
fn assert_ok(o: &Output, ctx: &str) {
    assert!(
        o.status.success(),
        "{ctx}\nstatus={:?}\nstdout={}\nstderr={}",
        o.status.code(),
        stdout(o),
        stderr(o)
    );
}

fn has_token() -> bool {
    if std::env::var("MEMOS_TOKEN").ok().filter(|s| !s.is_empty()).is_some() {
        return true;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let p = std::path::Path::new(&home).join(".config/memos-cli/env");
    std::fs::read_to_string(p)
        .map(|c| c.lines().any(|l| l.starts_with("MEMOS_TOKEN=") && l.len() > "MEMOS_TOKEN=".len()))
        .unwrap_or(false)
}

fn require_live() {
    if !has_token() {
        panic!("需要 MEMOS_TOKEN 或 ~/.config/memos-cli/env 才能跑功能测试");
    }
}

#[test]
fn help_lists_all_commands() {
    let o = run(&["--help"]);
    assert_ok(&o, "help");
    let s = stdout(&o);
    // 全英文顶级 about
    assert!(
        s.contains("Unofficial usememos CLI + TUI"),
        "help about should be English:\n{s}"
    );
    // Usage 子命令可选（默认分派）
    assert!(s.contains("[COMMAND]"), "Usage should show optional command:\n{s}");
    // 完整命令名：含 delete 主名、不含符号命令（- / +）
    for cmd in ["add", "list", "ls", "edit", "delete", "export", "import", "tui", "help", "version"] {
        assert!(s.contains(cmd), "help missing `{cmd}`:\n{s}");
    }
    assert!(
        s.lines().any(|l| l.starts_with("  delete")),
        "delete should be a top-level command line:\n{s}"
    );
    assert!(
        !s.lines().any(|l| l.starts_with("  - ") || l.starts_with("  + ")),
        "symbol commands should not appear in Commands list:\n{s}"
    );
    assert!(!s.contains("[alias"), "aliases moved out of help:\n{s}");
    // get 已删除
    assert!(!s.contains("get"), "get should be removed:\n{s}");
    // --alias 选项
    assert!(s.contains("--alias"), "--alias option missing:\n{s}");
}

#[test]
fn alias_flag() {
    let o = run(&["--alias"]);
    assert_ok(&o, "--alias");
    let s = stdout(&o);
    assert!(s.contains("list:"), "missing list row:\n{s}");
    assert!(s.contains("delete: -, del, rm"), "missing delete row:\n{s}");
    assert!(s.contains("add:    +"), "missing add row:\n{s}");
    assert!(s.contains("edit:   e"), "missing edit row:\n{s}");
    assert!(s.contains("config: cfg"), "missing config row:\n{s}");
}

#[test]
fn version_flag() {
    let o = run(&["--version"]);
    assert_ok(&o, "version");
    assert!(stdout(&o).contains("memos-cli"), "{}", stdout(&o));
    assert!(stdout(&o).contains("built"), "missing build time: {}", stdout(&o));

    let o = run(&["version"]);
    assert_ok(&o, "version subcommand");
    assert!(stdout(&o).contains("memos-cli"), "{}", stdout(&o));
    assert!(stdout(&o).contains("built"), "missing build time: {}", stdout(&o));

    // help 子命令（英文）
    let o = run(&["help"]);
    assert_ok(&o, "help subcommand");
    assert!(stdout(&o).to_lowercase().contains("usage"), "{}", stdout(&o));
    assert!(
        stdout(&o).contains("Show help"),
        "help should be English: {}",
        stdout(&o)
    );
}

#[test]
fn subcommand_helps() {
    for cmd in ["add", "list", "ls", "edit", "+", "-", "export", "import", "tui"] {
        let o = run(&[cmd, "--help"]);
        assert_ok(&o, &format!("{cmd} --help"));
        assert!(
            stdout(&o).contains("Usage:") || stdout(&o).to_lowercase().contains("usage"),
            "{cmd} help bare"
        );
    }
    // 英文 help 子命令：help [子命令]；未知子命令报错
    let o = run(&["help", "list"]);
    assert_ok(&o, "help list");
    assert!(
        stdout(&o).contains("List memos"),
        "help list should be English: {}",
        stdout(&o)
    );
    // delete --help 显示主命令名（非 -）
    let o = run(&["help", "delete"]);
    assert_ok(&o, "help delete");
    let s = stdout(&o);
    assert!(
        s.contains("Usage: delete [OPTIONS] [TARGETS]...")
            || s.contains("Usage: memos-cli delete [OPTIONS] [TARGETS]..."),
        "delete usage should show full name: {s}"
    );
    // 直接 delete --help 同样显示完整命令名
    let o = run(&["delete", "--help"]);
    assert_ok(&o, "delete --help");
    let s = stdout(&o);
    assert!(
        s.contains("Usage: delete [OPTIONS] [TARGETS]...")
            || s.contains("Usage: memos-cli delete [OPTIONS] [TARGETS]..."),
        "delete --help usage: {s}"
    );
    let o = run(&["help", "badcmd"]);
    assert!(!o.status.success(), "help badcmd should fail");
}

#[test]
fn live_crud_list_get_filter_export_import_delete() {
    require_live();
    let marker = format!("func-test-{}", std::process::id());

    // add 新增 + 标签（TW 同款：mct add 正文 +tag）
    let o = run(&["add", &marker, "alpha", "+functest"]);
    assert_ok(&o, "add");
    let created = stdout(&o);
    assert!(created.starts_with("created\t"), "{created}");
    let uid = created.trim().split('\t').nth(1).unwrap().to_string();

    // + 别名兼容（快速往返，不留残留）
    let o2 = run(&["+", &format!("{marker}-plusalias"), "+functest"]);
    assert_ok(&o2, "plus alias add");
    let uid3 = stdout(&o2).trim().split('\t').nth(1).unwrap().to_string();
    let o2 = run(&["del", &uid3, "-y"]);
    assert_ok(&o2, "plus alias del");

    // list / li / ls
    for sub in ["list", "li", "ls"] {
        let o = run(&[sub]);
        assert_ok(&o, sub);
        assert!(stdout(&o).contains(&marker), "{sub} missing marker");
    }

    // list 全列（ID/DATE/TAG/DESCRIPTION 表头 + 数据行）/ --raw / -n
    let o = run(&["li"]);
    assert_ok(&o, "li");
    let list_out = stdout(&o);
    assert!(
        list_out.starts_with("Id\tDate\tTag\tDescription"),
        "li header: {list_out}"
    );
    let list_line = list_out
        .lines()
        .find(|l| l.contains(&marker))
        .expect("li marker line")
        .to_string();
    assert_eq!(
        list_line.split('\t').count(),
        4,
        "li should be 4 cols (id/date/tag/content): {list_line}"
    );
    let o = run(&["li", "--raw"]);
    assert_ok(&o, "li --raw");
    assert!(stdout(&o).contains(&marker));
    let o = run(&["li", "-n", "1"]);
    assert_ok(&o, "li -n 1");
    let listed = stdout(&o);
    let data_lines: Vec<_> = listed
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with("Id\t"))
        .collect();
    assert_eq!(data_lines.len(), 1, " -n 1 should show one data line: {listed}");
    let o = run(&["list", "--help"]);
    assert_ok(&o, "list help -n");
    assert!(stdout(&o).contains("-n") || stdout(&o).contains("limit"));
    assert!(!stdout(&o).contains("--full"), "list should not expose --full");
    // ls 短列表三列：编号\t标签\t内容（表头 ID/TAG/DESCRIPTION）
    let o = run(&["ls"]);
    assert_ok(&o, "ls");
    let ls_out = stdout(&o);
    assert!(ls_out.starts_with("Id\tTag\tDescription"), "ls header: {ls_out}");
    let ls_line = ls_out
        .lines()
        .find(|l| l.contains(&marker))
        .expect("ls marker line")
        .to_string();
    assert_eq!(ls_line.split('\t').count(), 3, "ls should be 3 cols: {ls_line}");

    // 筛选 +tag
    let o = run(&["li", "+functest"]);
    assert_ok(&o, "+functest filter");
    assert!(stdout(&o).contains(&marker));

    // 筛选 /regex/（真正则）
    let o = run(&["li", &format!("/{marker}/")]);
    assert_ok(&o, "regex filter");
    assert!(stdout(&o).contains(&marker));
    let o = run(&["li", &format!("/^{}/", regex::escape(&marker))]);
    assert_ok(&o, "anchored regex");
    assert!(stdout(&o).contains(&marker));

    // 首参直接 +tag / /re/ 重写为 list（默认分派）
    let o = run(&["+functest"]);
    assert_ok(&o, "bare +tag");
    assert!(stdout(&o).contains(&marker));
    // -tag 首参同样重写（排除 functest → marker 不应出现）
    let o = run(&["-functest"]);
    assert_ok(&o, "bare -tag");
    assert!(
        !stdout(&o).contains(&marker),
        "-tag should exclude marker"
    );

    // 默认命令按 uid 查看全文（原 get 行为 → 完整信息：uid/date/tags/content）
    let o = run(&[&uid]);
    assert_ok(&o, "default show uid");
    let body = stdout(&o);
    for key in ["uid:", "date:", "tags:", "content:"] {
        assert!(body.contains(key), "missing `{key}` in full info: {body}");
    }
    assert!(body.contains(&marker), "full info should contain content: {body}");
    let o = run(&[&uid, "--raw"]);
    assert_ok(&o, "default show --raw");
    assert!(stdout(&o).contains("content") || stdout(&o).contains(&marker));

    // 默认命令按 list 编号查看全文
    let o = run(&["li"]);
    assert_ok(&o, "li for index");
    let idx = stdout(&o)
        .lines()
        .find(|l| l.contains(&marker))
        .and_then(|l| l.split('\t').next())
        .expect("index line")
        .to_string();
    let o = run(&[&idx]);
    assert_ok(&o, "default show by index");
    assert!(stdout(&o).contains(&marker));

    // edit via EDITOR script
    let dir = tempfile::tempdir().unwrap();
    let editor = dir.path().join("ed.sh");
    std::fs::write(
        &editor,
        format!(
            "#!/bin/sh\nprintf '%s' '{marker} edited' > \"$1\"\n"
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let o = bin()
        .args(["e", &uid])
        .env("EDITOR", &editor)
        .env("VISUAL", &editor)
        .output()
        .unwrap();
    assert_ok(&o, "edit");
    assert!(stdout(&o).contains("updated"), "{}", stdout(&o));
    let o = run(&[&uid]);
    assert_ok(&o, "show after edit");
    assert!(stdout(&o).contains("edited"));

    // export
    let o = run(&["export"]);
    assert_ok(&o, "export");
    assert!(stdout(&o).contains(&uid) || stdout(&o).contains("edited"));

    // import one line TSV
    let import_body = format!("x\timport-{marker}-line\t");
    let o = run_stdin(&["import"], &import_body);
    assert_ok(&o, "import");
    let imported = stdout(&o);
    assert!(imported.contains("imported\t"), "{imported}");
    let import_uid = imported.trim().split('\t').nth(1).unwrap().to_string();

    // 幂等：同一 uid 再导入 → already exists（不重复创建）
    let dup = format!("{import_uid}\tdup-line\t");
    let o = run_stdin(&["import"], &dup);
    assert_ok(&o, "import duplicate");
    assert!(stdout(&o).contains("already exists"), "{}", stdout(&o));

    // del force by uid（-y 在 targets 里也要吃掉）
    let o = run(&["-", &import_uid, "-y"]);
    assert_ok(&o, "del import");
    assert!(stdout(&o).contains("deleted"));

    // del alias
    let o = run(&["+", &format!("{marker}-delalias")]);
    assert_ok(&o, "add delalias");
    let uid2 = stdout(&o).trim().split('\t').nth(1).unwrap().to_string();
    let o = run(&["del", &uid2, "--force"]);
    assert_ok(&o, "del alias");
    assert!(stdout(&o).contains("deleted"));

    // 删主测试条
    let o = run(&["delete", &uid, "-y"]);
    assert_ok(&o, "delete main");
    assert!(stdout(&o).contains("deleted"));

    // 确认已不在 list
    let o = run(&["li", &format!("/{marker}/")]);
    assert_ok(&o, "list after delete");
    assert!(
        !stdout(&o).contains(&marker),
        "marker still listed: {}",
        stdout(&o)
    );
}

#[test]
fn tui_is_wired_in_help_and_rejects_bad_token_fast() {
    // tui 交互不在 CI 里跑；只验证子命令存在 + 无 token 时失败路径
    let o = run(&["tui", "--help"]);
    assert_ok(&o, "tui help");
    // 坏 token 应快速失败（连不上或 401），不要 hang
    let o = bin()
        .args(["tui", "--token", "invalid-token-for-test", "--base", "http://127.0.0.1:1"])
        .output()
        .unwrap();
    assert!(!o.status.success(), "tui should fail on bad base");
}
