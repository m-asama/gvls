// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use tokio::process::Command;

pub async fn exec(args: Vec<&str>) {
    if args.len() == 0 {
        return;
    }
    let mut cmd = &mut Command::new(args[0]);
    for i in 1..args.len() {
        cmd = cmd.arg(args[i]);
    }
    if let Err(e) = cmd.output().await {
        println!("exec error: {e}");
    }
}

pub async fn sysctl(val: &str) {
    exec(vec!["sysctl", "-w", val]).await;
}
