// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::net::Ipv6Addr;

use tokio::process::Command;

pub async fn ddns_renew(hostkey: &str, newaddr: &Option<Ipv6Addr>) {
    if hostkey == "" {
        return;
    }
    let newaddr = match newaddr {
        Some(newaddr) => newaddr,
        None => return,
    };
    let url = format!("https://ddnsapi-v6.open.ad.jp/api/renew/?{hostkey}={newaddr}");
    if let Err(e) = Command::new("curl").arg(&url).output().await {
        println!("curl execute error: {e}");
    }
}
