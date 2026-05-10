// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

use libgvls::UI_RCH_PORT;

pub const DEFAULT_HTTP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
pub const DEFAULT_HTTP_PORT: u16 = 3000;

pub const DEFAULT_RCH_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

pub const DEFAULT_DB_USERNAME: &str = "gvls";
pub const DEFAULT_DB_PASSWORD: &str = "gvls";
pub const DEFAULT_DB_HOST: &str = "127.0.0.1";
pub const DEFAULT_DB_PORT: &str = "5432";
pub const DEFAULT_DB_NAME: &str = "gvls";

pub fn usage() {
    println!("Usage: gvls-ui [OPTIONS]");
    println!("");
    println!("Options:");
    println!("    --http-addr <HTTP_ADDR>        HTTP address (Default: {DEFAULT_HTTP_ADDR})");
    println!("    --http-port <HTTP_PORT>        HTTP port (Default: {DEFAULT_HTTP_PORT})");
    println!("    --rch-addr <RCH_ADDR>          RCH address (Default: {DEFAULT_RCH_ADDR})");
    println!("    --rch-port <RCH_PORT>          RCH port (Default: {UI_RCH_PORT})");
    println!("");
}

#[derive(Debug)]
pub struct Config {
    pub http_addr: IpAddr,
    pub http_port: u16,
    pub rch_addr: IpAddr,
    pub rch_port: u16,
    pub db_username: String,
    pub db_password: String,
    pub db_host: String,
    pub db_port: String,
    pub db_name: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            http_addr: DEFAULT_HTTP_ADDR.clone(),
            http_port: DEFAULT_HTTP_PORT,
            rch_addr: DEFAULT_RCH_ADDR.clone(),
            rch_port: UI_RCH_PORT,
            db_username: DEFAULT_DB_USERNAME.to_string(),
            db_password: DEFAULT_DB_PASSWORD.to_string(),
            db_host: DEFAULT_DB_HOST.to_string(),
            db_port: DEFAULT_DB_PORT.to_string(),
            db_name: DEFAULT_DB_NAME.to_string(),
        }
    }
}

impl Config {
    pub fn from_args() -> Result<Self, String> {
        let mut conf = Self::default();
        if let Ok(db_username) = std::env::var("GVLS_UI_DB_USERNAME") {
            conf.db_username = db_username;
        }
        if let Ok(db_password) = std::env::var("GVLS_UI_DB_PASSWORD") {
            conf.db_password = db_password;
        }
        if let Ok(db_host) = std::env::var("GVLS_UI_DB_HOST") {
            conf.db_host = db_host;
        }
        if let Ok(db_port) = std::env::var("GVLS_UI_DB_PORT") {
            conf.db_port = db_port;
        }
        if let Ok(db_name) = std::env::var("GVLS_UI_DB_NAME") {
            conf.db_name = db_name;
        }
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_ref() {
                "--http-addr" => {
                    let http_addr_str = match args.next() {
                        Some(http_addr_str) => http_addr_str,
                        None => return Err(format!("HTTP address required")),
                    };
                    let http_addr = match IpAddr::from_str(&http_addr_str) {
                        Ok(http_addr) => http_addr,
                        Err(e) => return Err(format!("HTTP address format error: {e}")),
                    };
                    conf.http_addr = http_addr;
                }
                "--http-port" => {
                    let http_port_str = match args.next() {
                        Some(http_port_str) => http_port_str,
                        None => return Err(format!("HTTP port required")),
                    };
                    let http_port = match u16::from_str(&http_port_str) {
                        Ok(http_port) => http_port,
                        Err(e) => return Err(format!("HTTP port format error: {e}")),
                    };
                    conf.http_port = http_port;
                }
                "--rch-addr" => {
                    let rch_addr_str = match args.next() {
                        Some(rch_addr_str) => rch_addr_str,
                        None => return Err(format!("RCH address required")),
                    };
                    let rch_addr = match IpAddr::from_str(&rch_addr_str) {
                        Ok(rch_addr) => rch_addr,
                        Err(e) => return Err(format!("RCH address format error: {e}")),
                    };
                    conf.rch_addr = rch_addr;
                }
                "--rch-port" => {
                    let rch_port_str = match args.next() {
                        Some(rch_port_str) => rch_port_str,
                        None => return Err(format!("RCH port required")),
                    };
                    let rch_port = match u16::from_str(&rch_port_str) {
                        Ok(rch_port) => rch_port,
                        Err(e) => return Err(format!("RCH port format error: {e}")),
                    };
                    conf.rch_port = rch_port;
                }
                "-h" | "--help" => {
                    return Err(String::new());
                }
                _ => {
                    return Err(format!("Unknown argument"));
                }
            }
        }
        Ok(conf)
    }
}
