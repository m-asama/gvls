// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use libgvls::{BgpOps, BgpOpsFrr, BgpOpsZebraRs, RR_RCH_PORT, UI_RCH_PORT};

pub const DEFAULT_RCH_ADDR: Ipv6Addr = Ipv6Addr::LOCALHOST;
pub const DEFAULT_DDNS_HOSTKEY: &str = "";
pub const DEFAULT_BGP_BACKEND: BgpOps = BgpOps::Frr(BgpOpsFrr {});
pub const DEFAULT_BGP_ASNUM: u32 = 64512;

pub fn usage() {
    println!("Usage: gvls-rr [OPTIONS]");
    println!("");
    println!("Options:");
    println!("    --src-ifname <SRC_IFNAME>      Source interface name (Required)");
    println!("    --ui-addr <UI_ADDR>            gvls-ui address (Required)");
    println!("    --ui-port <UI_PORT>            gvls-ui port (Default: {UI_RCH_PORT})");
    println!("    --rch-addr <RCH_ADDR>          RCH address (Default: {DEFAULT_RCH_ADDR})");
    println!("    --rch-port <RCH_PORT>          RCH port (Default: {RR_RCH_PORT})");
    println!("    --bgp-backend (frr|zebra-rs)   BGP backend (Default: {DEFAULT_BGP_BACKEND})");
    println!("    --bgp-asnum <BGP_ASNUM>        BGP ASNUM (Default: {DEFAULT_BGP_ASNUM})");
    println!("");
}

#[derive(Debug)]
pub struct Config {
    pub rr_name: String,
    pub rr_pass: String,
    pub src_ifname: String,
    pub ui_addr: Option<Ipv4Addr>,
    pub ui_port: u16,
    pub rch_addr: Ipv6Addr,
    pub rch_port: u16,
    pub ddns_hostkey: String,
    pub bgp_ops: BgpOps,
    pub bgp_asnum: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rr_name: String::new(),
            rr_pass: String::new(),
            src_ifname: String::new(),
            ui_addr: None,
            ui_port: UI_RCH_PORT,
            rch_addr: DEFAULT_RCH_ADDR.clone(),
            rch_port: RR_RCH_PORT,
            ddns_hostkey: DEFAULT_DDNS_HOSTKEY.to_string(),
            bgp_ops: DEFAULT_BGP_BACKEND,
            bgp_asnum: DEFAULT_BGP_ASNUM,
        }
    }
}

impl Config {
    pub fn from_args() -> Result<Self, String> {
        let mut conf = Self::default();
        if let Ok(rr_name) = std::env::var("GVLS_RR_NAME") {
            conf.rr_name = rr_name;
        }
        if let Ok(rr_pass) = std::env::var("GVLS_RR_PASS") {
            conf.rr_pass = rr_pass;
        }
        if let Ok(ddns_hostkey) = std::env::var("GVLS_RR_DDNS_HOSTKEY") {
            conf.ddns_hostkey = ddns_hostkey;
        }
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_ref() {
                "--src-ifname" => {
                    let src_ifname = match args.next() {
                        Some(src_ifname) => src_ifname,
                        None => return Err(format!("Source interface name required")),
                    };
                    conf.src_ifname = src_ifname;
                }
                "--ui-addr" => {
                    let ui_addr_str = match args.next() {
                        Some(ui_addr_str) => ui_addr_str,
                        None => return Err(format!("gvls-ui address required")),
                    };
                    let ui_addr = match Ipv4Addr::from_str(&ui_addr_str) {
                        Ok(ui_addr) => ui_addr,
                        Err(e) => return Err(format!("gvls-ui address format error: {e}")),
                    };
                    conf.ui_addr = Some(ui_addr);
                }
                "--ui-port" => {
                    let ui_port_str = match args.next() {
                        Some(ui_port_str) => ui_port_str,
                        None => return Err(format!("gvls-ui port required")),
                    };
                    let ui_port = match u16::from_str(&ui_port_str) {
                        Ok(ui_port) => ui_port,
                        Err(e) => return Err(format!("gvls-ui port format error: {e}")),
                    };
                    conf.ui_port = ui_port;
                }
                "--rch-addr" => {
                    let rch_addr_str = match args.next() {
                        Some(rch_addr_str) => rch_addr_str,
                        None => return Err(format!("RCH address required")),
                    };
                    let rch_addr = match Ipv6Addr::from_str(&rch_addr_str) {
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
                "--bgp-backend" => {
                    let bgp_backend_str = match args.next() {
                        Some(bgp_backend_str) => bgp_backend_str,
                        None => return Err(format!("BGP backend required")),
                    };
                    match bgp_backend_str.as_str() {
                        "frr" => {
                            conf.bgp_ops = BgpOps::Frr(BgpOpsFrr {});
                        }
                        "zebra-rs" => {
                            conf.bgp_ops = BgpOps::ZebraRs(BgpOpsZebraRs {});
                        }
                        _ => return Err(format!("BGP bakend not supported")),
                    }
                }
                "--bgp-asnum" => {
                    let bgp_asnum_str = match args.next() {
                        Some(bgp_asnum_str) => bgp_asnum_str,
                        None => return Err(format!("BGP ASNUM required")),
                    };
                    let bgp_asnum = match u32::from_str(&bgp_asnum_str) {
                        Ok(bgp_asnum) => bgp_asnum,
                        Err(e) => return Err(format!("BGP ASNUM format error: {e}")),
                    };
                    conf.bgp_asnum = bgp_asnum;
                }
                "-h" | "--help" => {
                    return Err(String::new());
                }
                _ => {
                    return Err(format!("Unknown argument"));
                }
            }
        }
        if conf.src_ifname == "" {
            return Err(format!("Source interface name required"));
        }
        if conf.ui_addr.is_none() {
            return Err(format!("gvls-ui address required"));
        }
        if conf.rr_name == "" {
            return Err(format!("GVLS_RR_NAME required"));
        }
        if conf.rr_pass == "" {
            return Err(format!("GVLS_RR_PASS required"));
        }
        Ok(conf)
    }
}
