// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

mod config;
mod context;
mod db;
mod messages;
mod rr_handler;
mod ui_handler;

use config::*;
use context::*;
use messages::*;
use rr_handler::*;
use ui_handler::*;

#[tokio::main]
async fn main() {
    let conf = match Config::from_args() {
        Ok(conf) => conf,
        Err(e) => {
            if e.len() > 0 {
                println!("Config parse error: {e}");
            }
            usage();
            return;
        }
    };
    let mut ctx = match Context::from_conf(conf) {
        Ok(ctx) => ctx,
        Err(e) => {
            println!("Context init error: {e}");
            return;
        }
    };
    ctx.run().await;
}
