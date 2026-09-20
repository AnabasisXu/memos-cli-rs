//! 构建脚本：注入编译时间戳（UTC ISO-8601），供 --version / version 显示。
//! 构建期依赖系统 date 命令；失败回退 "unknown"，不影响构建。

use std::process::Command;

fn main() {
    let ts = match Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]).output() {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    };
    println!("cargo:rustc-env=BUILD_TIME={ts}");
    println!("cargo:rerun-if-changed=build.rs");
}