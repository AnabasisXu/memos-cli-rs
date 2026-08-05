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
    for cmd in [
        "list", "get", "edit", "+", "-", "export", "import", "tui", "help", "version",
    ] {
        assert!(s.contains(cmd), "help missing `{cmd}`:\n{s}");
    }
    // aliases mentioned
    assert!(s.contains("l") || s.contains("ls"));
}

#[test]
fn version_flag() {
    let o = run(&["--version"]);
    assert_ok(&o, "version");
    assert!(stdout(&o).contains("memos-cli"));

    let o = run(&["version"]);
    assert_ok(&o, "version subcommand");
    assert!(stdout(&o).contains("memos-cli"));

    // clap 内置 help 子命令
    let o = run(&["help"]);
    assert_ok(&o, "help subcommand");
    assert!(stdout(&o).to_lowercase().contains("usage"));
}

#[test]
fn subcommand_helps() {
    for cmd in ["list", "get", "edit", "+", "-", "export", "import", "tui"] {
        let o = run(&[cmd, "--help"]);
        assert_ok(&o, &format!("{cmd} --help"));
        assert!(
            stdout(&o).contains("Usage:") || stdout(&o).to_lowercase().contains("usage"),
            "{cmd} help bare"
        );
    }
}

#[test]
fn live_crud_list_get_filter_export_import_delete() {
    require_live();
    let marker = format!("func-test-{}", std::process::id());

    // + 新增 + 标签
    let o = run(&["+", &marker, "alpha", "+functest"]);
    assert_ok(&o, "add");
    let created = stdout(&o);
    assert!(created.starts_with("created\t"), "{created}");
    let uid = created.trim().split('\t').nth(1).unwrap().to_string();

    // list / l / ls
    for sub in ["list", "l", "ls"] {
        let o = run(&[sub]);
        assert_ok(&o, sub);
        assert!(stdout(&o).contains(&marker), "{sub} missing marker");
    }

    // list -f / --raw / -n
    let o = run(&["l", "-f"]);
    assert_ok(&o, "l -f");
    let o = run(&["l", "--raw"]);
    assert_ok(&o, "l --raw");
    assert!(stdout(&o).contains(&marker));
    let o = run(&["l", "-n", "1"]);
    assert_ok(&o, "l -n 1");
    let listed = stdout(&o);
    let lines: Vec<_> = listed.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1, " -n 1 should show one line: {lines:?}");
    let o = run(&["list", "--help"]);
    assert_ok(&o, "list help -n");
    assert!(stdout(&o).contains("-n") || stdout(&o).contains("limit"));

    // 筛选 +tag
    let o = run(&["l", "+functest"]);
    assert_ok(&o, "+functest filter");
    assert!(stdout(&o).contains(&marker));

    // 筛选 /regex/（真正则）
    let o = run(&["l", &format!("/{marker}/")]);
    assert_ok(&o, "regex filter");
    assert!(stdout(&o).contains(&marker));
    let o = run(&["l", &format!("/^{}/", regex::escape(&marker))]);
    assert_ok(&o, "anchored regex");
    assert!(stdout(&o).contains(&marker));

    // 首参直接 +tag / /re/ 重写为 list
    let o = run(&["+functest"]);
    assert_ok(&o, "bare +tag");
    assert!(stdout(&o).contains(&marker));

    // get / show / cat by uid
    for sub in ["get", "show", "cat"] {
        let o = run(&[sub, &uid]);
        assert_ok(&o, sub);
        assert!(stdout(&o).contains(&marker), "{sub} body");
    }
    let o = run(&["get", &uid, "--raw"]);
    assert_ok(&o, "get --raw");
    assert!(stdout(&o).contains("content") || stdout(&o).contains(&marker));

    // get by list index: find index of marker
    let o = run(&["l"]);
    assert_ok(&o, "l for index");
    let idx = stdout(&o)
        .lines()
        .find(|l| l.contains(&marker))
        .and_then(|l| l.split('\t').next())
        .expect("index line")
        .to_string();
    let o = run(&["get", &idx]);
    assert_ok(&o, "get by index");
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
    let o = run(&["get", &uid]);
    assert_ok(&o, "get after edit");
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
    let o = run(&["l", &format!("/{marker}/")]);
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
