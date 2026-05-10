// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::str::FromStr;

use libgvls::{BgpOps, BgpOpsFrr, BgpOpsZebraRs, RR_RCH_PORT};

pub const DEFAULT_BGP_BACKEND: BgpOps = BgpOps::Frr(BgpOpsFrr {});
pub const DEFAULT_BGP_ASNUM: u32 = 64512;

pub fn usage() {
    println!("Usage: gvls-vtep [OPTIONS]");
    println!("");
    println!("Options:");
    println!("    --src-ifname <SRC_IFNAME>      Source interface name (Required)");
    println!("    --rr1-host <RR1_HOST>          gvls-rr #1 host (Required)");
    println!("    --rr1-port <RR1_PORT>          gvls-rr #1 port (Default: {RR_RCH_PORT})");
    println!("    --rr2-host <RR2_HOST>          gvls-rr #2 host (Required)");
    println!("    --rr2-port <RR2_PORT>          gvls-rr #2 port (Default: {RR_RCH_PORT})");
    println!("    --vni <VNI>:<IF_NAME>          VNI and interface name pair (Optional)");
    println!("    --bgp-backend (frr|zebra-rs)   BGP backend (Default: {DEFAULT_BGP_BACKEND})");
    println!("    --bgp-asnum <BGP_ASNUM>        BGP ASNUM (Default: {DEFAULT_BGP_ASNUM})");
    println!("");
}

#[derive(Debug)]
pub struct Config {
    pub vtep_name: String,
    pub vtep_pass: String,
    pub src_ifname: String,
    pub rr_hosts: [String; 2],
    pub rr_ports: [u16; 2],
    pub vnis: Vec<(u32, String)>,
    pub bgp_ops: BgpOps,
    pub bgp_asnum: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vtep_name: String::new(),
            vtep_pass: String::new(),
            src_ifname: String::new(),
            rr_hosts: [String::new(), String::new()],
            rr_ports: [RR_RCH_PORT, RR_RCH_PORT],
            vnis: Vec::<(u32, String)>::new(),
            bgp_ops: DEFAULT_BGP_BACKEND,
            bgp_asnum: DEFAULT_BGP_ASNUM,
        }
    }
}

impl Config {
    pub fn from_args() -> Result<Self, String> {
        let mut conf = Self::default();
        if let Ok(vtep_name) = std::env::var("GVLS_VTEP_NAME") {
            conf.vtep_name = vtep_name;
        }
        if let Ok(vtep_pass) = std::env::var("GVLS_VTEP_PASS") {
            conf.vtep_pass = vtep_pass;
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
                "--rr1-host" => {
                    let rr1_host = match args.next() {
                        Some(rr1_host) => rr1_host,
                        None => return Err(format!("gvls-rr #1 host required")),
                    };
                    conf.rr_hosts[0] = rr1_host;
                }
                "--rr1-port" => {
                    let rr1_port_str = match args.next() {
                        Some(rr1_port_str) => rr1_port_str,
                        None => return Err(format!("gvls-rr #1 port required")),
                    };
                    let rr1_port = match u16::from_str(&rr1_port_str) {
                        Ok(rr1_port) => rr1_port,
                        Err(e) => return Err(format!("gvls-rr #1 port format error: {e}")),
                    };
                    conf.rr_ports[0] = rr1_port;
                }
                "--rr2-host" => {
                    let rr2_host = match args.next() {
                        Some(rr2_host) => rr2_host,
                        None => return Err(format!("gvls-rr #2 host required")),
                    };
                    conf.rr_hosts[1] = rr2_host;
                }
                "--rr2-port" => {
                    let rr2_port_str = match args.next() {
                        Some(rr2_port_str) => rr2_port_str,
                        None => return Err(format!("gvls-rr #2 port required")),
                    };
                    let rr2_port = match u16::from_str(&rr2_port_str) {
                        Ok(rr2_port) => rr2_port,
                        Err(e) => return Err(format!("gvls-rr #2 port format error: {e}")),
                    };
                    conf.rr_ports[1] = rr2_port;
                }
                "--vni" => {
                    let vni_ifname_pair = match args.next() {
                        Some(vni_ifname_pair) => vni_ifname_pair,
                        None => return Err(format!("VNI and interface name error")),
                    };
                    let vni_ifname_pair: Vec<&str> = vni_ifname_pair.split(':').collect();
                    if vni_ifname_pair.len() != 2 {
                        return Err(format!("VNI and interface name format error"));
                    }
                    let vni = match u32::from_str(&vni_ifname_pair[0]) {
                        Ok(vni) => vni,
                        Err(e) => return Err(format!("VNI format error: {e}")),
                    };
                    if !(1u32..=16777214u32).contains(&vni) {
                        return Err(format!("VNI out of range"));
                    }
                    let ifname = vni_ifname_pair[1];
                    conf.vnis.push((vni, ifname.to_string()));
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
        if conf.rr_hosts[0] == "" {
            return Err(format!("gvls-rr #1 host required"));
        }
        if conf.rr_hosts[1] == "" {
            return Err(format!("gvls-rr #2 host required"));
        }
        if conf.vtep_name == "" {
            return Err(format!("GVLS_VTEP_NAME required"));
        }
        if conf.vtep_pass == "" {
            return Err(format!("GVLS_VTEP_PASS required"));
        }
        Ok(conf)
    }
}
