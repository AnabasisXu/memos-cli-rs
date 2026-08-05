fn main() {
    if let Err(e) = memos_cli::cli::run() {
        eprintln!("memos-cli: {e:#}");
        std::process::exit(1);
    }
}
