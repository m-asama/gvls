// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::str::FromStr;

use libgvls::{
    AddVniReqMsg, DelVniReqMsg, VTEP_RCH_PATH, VtepCtrlVtepRchMsg, VtepVtepCtrlRchMsg,
    rch_connect_path,
};

fn usage() {
    println!("Usage: gvls-vtep-ctrl [OPTIONS] add-vni vni ifname");
    println!("       gvls-vtep-ctrl [OPTIONS] del-vni vni");
    println!("");
    println!("Options:");
    println!("    --rch-path <RCH_PATH>          RCH path (Default: {VTEP_RCH_PATH})");
    println!("");
}

async fn add_vni(path: String, args: Vec<String>) {
    if args.len() != 3 {
        usage();
        return;
    }
    let (mut tx_rch, mut rx_rch) =
        match rch_connect_path::<VtepCtrlVtepRchMsg, VtepVtepCtrlRchMsg>(path).await {
            Ok((tx_rch, rx_rch)) => (tx_rch, rx_rch),
            Err(e) => {
                println!("Rch connect error: {e}");
                return;
            }
        };
    let vni = match u32::from_str(&args[1]) {
        Ok(vni) => vni,
        Err(e) => {
            println!("VNI invalid: {e}");
            return;
        }
    };
    let ifname = args[2].clone();
    let req = VtepCtrlVtepRchMsg::AddVniReq(AddVniReqMsg {
        vni: vni,
        ifname: ifname,
    });
    if let Err(e) = tx_rch.send(req).await {
        println!("Rch send error: {e}");
        return;
    }
    let rep = match rx_rch.recv().await {
        Ok(Some(VtepVtepCtrlRchMsg::AddVniRep(rep))) => rep,
        Ok(_) => {
            println!("Invalid reply");
            return;
        }
        Err(e) => {
            println!("Error reply {e}");
            return;
        }
    };
    if let Err(e) = rep.result {
        println!("ERROR: {e}");
    }
}

async fn del_vni(path: String, args: Vec<String>) {
    if args.len() != 2 {
        usage();
        return;
    }
    let (mut tx_rch, mut rx_rch) =
        match rch_connect_path::<VtepCtrlVtepRchMsg, VtepVtepCtrlRchMsg>(path).await {
            Ok((tx_rch, rx_rch)) => (tx_rch, rx_rch),
            Err(e) => {
                println!("Rch connect error: {e}");
                return;
            }
        };
    let vni = match u32::from_str(&args[1]) {
        Ok(vni) => vni,
        Err(e) => {
            println!("VNI invalid: {e}");
            return;
        }
    };
    let req = VtepCtrlVtepRchMsg::DelVniReq(DelVniReqMsg { vni: vni });
    if let Err(e) = tx_rch.send(req).await {
        println!("Rch send error: {e}");
        return;
    }
    let rep = match rx_rch.recv().await {
        Ok(Some(VtepVtepCtrlRchMsg::DelVniRep(rep))) => rep,
        Ok(_) => {
            println!("Invalid reply");
            return;
        }
        Err(e) => {
            println!("Error reply {e}");
            return;
        }
    };
    if let Err(e) = rep.result {
        println!("ERROR: {e}");
    }
}

#[tokio::main]
async fn main() {
    //let args: Vec<String> = std::env::args().skip(1).collect();
    let mut path = VTEP_RCH_PATH.to_string();
    let mut argsv = Vec::<String>::new();
    let mut argsi = std::env::args().skip(1);
    while let Some(arg) = argsi.next() {
        match arg.as_ref() {
            "--rch-path" => {
                path = match argsi.next() {
                    Some(path) => path,
                    None => {
                        println!("RCH path required");
                        return;
                    }
                };
            }
            _ => {
                argsv.push(arg);
            }
        }
    }
    match argsv[0].as_str() {
        "add-vni" => add_vni(path, argsv).await,
        "del-vni" => del_vni(path, argsv).await,
        _ => println!("Unknown command {}", argsv[0]),
    }
}
