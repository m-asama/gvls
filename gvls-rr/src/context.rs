// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use tokio::sync::mpsc;
use tokio::time::sleep;

use libgvls::{BgpOps, RrVtepRchMsg, TlsRchListener, Vni, Vtep, VtepRrRchMsg, get_ifindex};

use crate::{
    AddVniMsg, AddVtepMsg, AuthVtepRep, AuthVtepReq, Config, DelVniMsg, DelVtepMsg,
    LocAddrChangedMsg, ModVtepVniMsg, RrLchMsg, RrRegisteredMsg, RtnlHandler, UiHandler, UiLchMsg,
    UpdateNeighsMsg, UpdateVtepStateMsg, VtepExitMsg, VtepHandler, VtepLchMsg, VtepRegisteredMsg,
    ddns_renew,
};

#[derive(Debug)]
pub struct Context {
    rr_id: i32,
    rr_name: String,
    rr_pass: String,
    src_ifname: String,
    src_ifindex: u32,
    ui_addr: Ipv4Addr,
    ui_port: u16,
    rch_addr: Ipv6Addr,
    rch_port: u16,
    ddns_hostkey: String,
    bgp_ops: BgpOps,
    bgp_asnum: u32,
    //
    loc_addr: Option<Ipv6Addr>,
    vteps: HashMap<String, Vtep>,
    vnis: HashMap<i32, Vni>,
    //
    tx_lch: mpsc::Sender<RrLchMsg>,
    rx_lch: mpsc::Receiver<RrLchMsg>,
    ui_tx_lch: Option<mpsc::Sender<UiLchMsg>>,
    vtep_tx_lchs: HashMap<Ipv6Addr, mpsc::Sender<VtepLchMsg>>,
}

impl Context {
    pub fn from_conf(conf: Config) -> Result<Self, String> {
        if conf.ui_addr.is_none() {
            return Err(format!("gvls-ui address required"));
        }
        if conf.rr_name == "" {
            return Err(format!("GVLS_RR_NAME required"));
        }
        if conf.rr_pass == "" {
            return Err(format!("GVLS_RR_PASS required"));
        }
        let (tx_lch, rx_lch) = mpsc::channel(1);
        Ok(Self {
            rr_id: -1,
            rr_name: conf.rr_name,
            rr_pass: conf.rr_pass,
            src_ifname: conf.src_ifname,
            src_ifindex: 0,
            ui_addr: conf.ui_addr.unwrap(),
            ui_port: conf.ui_port,
            rch_addr: conf.rch_addr,
            rch_port: conf.rch_port,
            ddns_hostkey: conf.ddns_hostkey,
            bgp_ops: conf.bgp_ops,
            bgp_asnum: conf.bgp_asnum,
            //
            loc_addr: None,
            vteps: HashMap::<String, Vtep>::new(),
            vnis: HashMap::<i32, Vni>::new(),
            //
            tx_lch,
            rx_lch,
            ui_tx_lch: None,
            vtep_tx_lchs: HashMap::<Ipv6Addr, mpsc::Sender<VtepLchMsg>>::new(),
        })
    }

    fn router_id(&self) -> Ipv4Addr {
        let bits: u32 = if self.rr_id > 0 {
            let bits: u32 = self.rr_id as u32 & 0x0fffffffu32;
            0xac100000u32 | bits
        } else {
            let bits: u32 = rand::random_range(0..0x10000);
            0xa9fe0000u32 | bits
        };
        Ipv4Addr::from_bits(bits)
    }

    async fn init_src_ifindex(&mut self) -> Result<(), String> {
        self.src_ifindex = get_ifindex(&self.src_ifname).await?;
        Ok(())
    }

    async fn sync_bgp(
        &self,
        old: &HashMap<String, Vtep>,
        new: &HashMap<String, Vtep>,
    ) -> Result<(), String> {
        let old_keys: HashSet<String> = old.keys().cloned().collect();
        let new_keys: HashSet<String> = new.keys().cloned().collect();
        for name in old_keys.difference(&new_keys) {
            let old_vtep = old.get(name).unwrap();
            if let Some(addr) = &old_vtep.ipv6_addr {
                self.bgp_ops.del_neighbor(self.bgp_asnum, &addr).await;
            }
            self.bgp_ops.del_route_map(name).await;
        }
        for name in old_keys.intersection(&new_keys) {
            let old_vtep = old.get(name).unwrap();
            let new_vtep = new.get(name).unwrap();
            if old_vtep.vnis != new_vtep.vnis {
                self.bgp_ops.rep_route_map(name, &new_vtep.vnis).await;
            }
            if old_vtep.ipv6_addr == new_vtep.ipv6_addr {
                if let Some(addr) = &new_vtep.ipv6_addr {
                    self.bgp_ops
                        .upd_neighbor_pass(self.bgp_asnum, &addr, &new_vtep.bgp_pass)
                        .await;
                }
            } else {
                if let Some(addr) = &old_vtep.ipv6_addr {
                    self.bgp_ops.del_neighbor(self.bgp_asnum, &addr).await;
                }
                if let Some(rem_addr) = &new_vtep.ipv6_addr {
                    self.bgp_ops
                        .add_neighbor(
                            self.bgp_asnum,
                            &rem_addr,
                            &self.loc_addr,
                            &new_vtep.name,
                            &new_vtep.bgp_pass,
                            true,
                        )
                        .await;
                }
            }
        }
        for name in new_keys.difference(&old_keys) {
            let new_vtep = new.get(name).unwrap();
            self.bgp_ops.rep_route_map(name, &new_vtep.vnis).await;
            if let Some(rem_addr) = &new_vtep.ipv6_addr {
                self.bgp_ops
                    .add_neighbor(
                        self.bgp_asnum,
                        &rem_addr,
                        &self.loc_addr,
                        &new_vtep.name,
                        &new_vtep.bgp_pass,
                        true,
                    )
                    .await;
            }
        }
        Ok(())
    }

    async fn send_update_vtep_state(
        &self,
        name: String,
        ipv6_addr: Option<Ipv6Addr>,
        last_update: Instant,
    ) {
        let last_update = format!("{:?}", last_update);
        let msg = UiLchMsg::UpdateVtepState(UpdateVtepStateMsg {
            name,
            ipv6_addr,
            last_update,
        });
        if let Some(ui_tx_lch) = &self.ui_tx_lch {
            if let Err(e) = ui_tx_lch.send(msg).await {
                println!("Send update VTEP state failed: {e}");
            }
        }
    }

    async fn send_update_neighs(&mut self, changed_vtep_name: &str) {
        for (name, vtep) in &self.vteps {
            if name == changed_vtep_name {
                continue;
            }
            let vtep_addr = match &vtep.ipv6_addr {
                Some(addr) => addr,
                None => continue,
            };
            let mut neighs = HashSet::<Ipv6Addr>::new();
            for vni in &vtep.vnis {
                for peer_vtep in self.vteps.values() {
                    if peer_vtep.vnis.contains(&vni) {
                        if let Some(peer_addr) = &peer_vtep.ipv6_addr {
                            neighs.insert(peer_addr.clone());
                        }
                    }
                }
            }
            neighs.remove(vtep_addr);
            if let Some(vtep_tx_lch) = &self.vtep_tx_lchs.get_mut(vtep_addr) {
                println!(
                    "Send neighbor update to VTEP {} ({}): neighs={}",
                    vtep.name,
                    vtep_addr,
                    neighs.len()
                );
                let msg = VtepLchMsg::UpdateNeighs(UpdateNeighsMsg { neighs });
                if let Err(e) = vtep_tx_lch.send(msg).await {
                    println!("Send neighbor update to VTEP {} failed: {e}", vtep.name);
                }
            }
        }
    }

    async fn rr_registered(&mut self, msg: RrRegisteredMsg) -> Result<(), String> {
        println!(
            "Registered to gvls-ui: vteps={} vnis={}",
            msg.vteps.len(),
            msg.vnis.len()
        );
        if msg.rr_id != self.rr_id {
            self.rr_id = msg.rr_id;
            self.bgp_ops
                .set_router_id(self.bgp_asnum, self.router_id())
                .await;
        }
        let mut vteps = msg.vteps;
        let vnis = msg.vnis;
        for (name, new) in &mut vteps {
            if let Some(old) = self.vteps.get(name) {
                new.ipv6_addr = old.ipv6_addr.clone();
                new.last_update = old.last_update.clone();
            }
        }
        for (name, old) in &mut self.vteps {
            if vteps.contains_key(name) {
                continue;
            }
            if let Some(ipv6_addr) = &old.ipv6_addr {
                self.vtep_tx_lchs.remove(ipv6_addr);
            }
        }
        if let Err(e) = self.sync_bgp(&self.vteps, &vteps).await {
            println!("Sync BGP after UI registration failed: {e}");
        }
        self.vteps = vteps;
        self.vnis = vnis;
        for (_, vtep) in &self.vteps {
            if let Some(last_update) = &vtep.last_update {
                self.send_update_vtep_state(
                    vtep.name.clone(),
                    vtep.ipv6_addr.clone(),
                    last_update.clone(),
                )
                .await;
            }
        }
        Ok(())
    }

    async fn loc_addr_changed(&mut self, msg: LocAddrChangedMsg) -> Result<(), String> {
        println!(
            "Local address changed: {:?} -> {:?}",
            self.loc_addr, msg.loc_addr
        );
        self.vtep_tx_lchs.clear();
        for (_, vtep) in &mut self.vteps {
            if let Some(addr) = &vtep.ipv6_addr {
                self.bgp_ops.del_neighbor(self.bgp_asnum, addr).await;
            }
            vtep.ipv6_addr = None;
            vtep.last_update = Some(Instant::now());
        }
        let ddns_hostkey = self.ddns_hostkey.clone();
        let loc_addr = msg.loc_addr.clone();
        tokio::spawn(async move {
            sleep(Duration::from_secs(5)).await;
            ddns_renew(&ddns_hostkey, &loc_addr).await;
        });
        self.loc_addr = msg.loc_addr;
        Ok(())
    }

    async fn auth_vtep(&mut self, msg: AuthVtepReq) -> Result<(), String> {
        println!("Authenticating VTEP {} from {}", msg.name, msg.rem_addr);
        let mut vtep_name: Result<String, String> = Err(format!("Auth failed"));
        let mut vtep_id = -1;
        let mut bgp_pass = String::new();
        let mut neighs = HashSet::<Ipv6Addr>::new();
        if let Some(vtep) = self.vteps.get(&msg.name) {
            if let Ok(parsed_hash) = PasswordHash::new(&vtep.password) {
                if Argon2::default()
                    .verify_password(msg.password.as_bytes(), &parsed_hash)
                    .is_ok()
                {
                    vtep_name = Ok(vtep.name.clone());
                    vtep_id = vtep.id;
                    bgp_pass = vtep.bgp_pass.clone();
                }
            }
            for vni in &vtep.vnis {
                for (_, vtep) in &self.vteps {
                    if vtep.vnis.contains(&vni) {
                        if let Some(addr) = &vtep.ipv6_addr {
                            neighs.insert(addr.clone());
                        }
                    }
                }
            }
        }
        neighs.remove(&msg.rem_addr);
        if let Err(e) = msg
            .rep_tx
            .send(AuthVtepRep {
                vtep_name: vtep_name,
                vtep_id,
                bgp_pass,
                neighs: neighs,
            })
            .await
        {
            println!("Send AuthVtepRep for {} failed: {e}", msg.name);
        }
        Ok(())
    }

    async fn vtep_registered(&mut self, msg: VtepRegisteredMsg) -> Result<(), String> {
        println!("VTEP registered: {} from {}", msg.name, msg.rem_addr);
        let vtep = match self.vteps.get_mut(&msg.name) {
            Some(vtep) => vtep,
            None => {
                println!("VTEP registration rejected: {} is not configured", msg.name);
                return Ok(());
            }
        };
        if let Some(addr) = &vtep.ipv6_addr
            && *addr != msg.rem_addr
        {
            self.vtep_tx_lchs.remove(addr);
            self.bgp_ops.del_neighbor(self.bgp_asnum, addr).await;
        }
        self.bgp_ops
            .add_neighbor(
                self.bgp_asnum,
                &msg.rem_addr,
                &self.loc_addr,
                &vtep.name,
                &vtep.bgp_pass,
                true,
            )
            .await;
        let now = Instant::now();
        vtep.ipv6_addr = Some(msg.rem_addr.clone());
        vtep.last_update = Some(now.clone());
        let name = vtep.name.clone();
        let ipv6_addr = vtep.ipv6_addr.clone();
        self.send_update_vtep_state(name, ipv6_addr, now).await;
        self.send_update_neighs(&msg.name).await;
        Ok(())
    }

    async fn vtep_exit(&mut self, msg: VtepExitMsg) -> Result<(), String> {
        println!("VTEP disconnected: {} from {}", msg.name, msg.rem_addr);
        self.vtep_tx_lchs.remove(&msg.rem_addr);
        self.bgp_ops
            .del_neighbor(self.bgp_asnum, &msg.rem_addr)
            .await;
        let vtep = match self.vteps.get_mut(&msg.name) {
            Some(vtep) => vtep,
            None => {
                println!("VTEP exit ignored: {} is not configured", msg.name);
                return Ok(());
            }
        };
        if let Some(ipv6_addr) = &vtep.ipv6_addr
            && ipv6_addr == &msg.rem_addr
        {
            let now = Instant::now();
            vtep.ipv6_addr = None;
            vtep.last_update = Some(now.clone());
            let name = vtep.name.clone();
            let ipv6_addr = vtep.ipv6_addr.clone();
            self.send_update_vtep_state(name, ipv6_addr, now).await;
        }
        self.send_update_neighs(&msg.name).await;
        Ok(())
    }

    fn debug_dump(&self) {
        println!("vteps = {:?}", self.vteps);
        println!("vnis = {:?}", self.vnis);
    }

    async fn add_vtep(&mut self, msg: AddVtepMsg) -> Result<(), String> {
        println!("AddVtep {msg:?}");
        if msg.vtep.vnis.len() != 0 {
            println!("VTEP must not have VNI {}", msg.vtep.name);
            return Ok(());
        }
        if self.vteps.contains_key(&msg.vtep.name) {
            println!("VTEP already exists {}", msg.vtep.name);
            return Ok(());
        }
        let mut vteps = self.vteps.clone();
        vteps.insert(msg.vtep.name.clone(), msg.vtep);
        if let Err(e) = self.sync_bgp(&self.vteps, &vteps).await {
            println!("Sync BGP error: {e}");
        }
        self.vteps = vteps;
        self.debug_dump();
        Ok(())
    }

    async fn del_vtep(&mut self, msg: DelVtepMsg) -> Result<(), String> {
        println!("DelVtep {msg:?}");
        if msg.vtep.vnis.len() != 0 {
            println!("VTEP must not have VNI {}", msg.vtep.name);
            return Ok(());
        }
        if !self.vteps.contains_key(&msg.vtep.name) {
            println!("VTEP not exists {}", msg.vtep.name);
            return Ok(());
        }
        let mut vteps = self.vteps.clone();
        if let Some(removed) = vteps.remove(&msg.vtep.name) {
            if let Some(ipv6_addr) = &removed.ipv6_addr {
                self.vtep_tx_lchs.remove(ipv6_addr);
            }
        }
        if let Err(e) = self.sync_bgp(&self.vteps, &vteps).await {
            println!("Sync BGP error: {e}");
        }
        self.vteps = vteps;
        self.debug_dump();
        Ok(())
    }

    async fn add_vni(&mut self, msg: AddVniMsg) -> Result<(), String> {
        println!("AddVni {msg:?}");
        if msg.vni.vteps.len() != 0 {
            println!("VNI must not have VTEP {}", msg.vni.vni);
            return Ok(());
        }
        if self.vnis.contains_key(&msg.vni.vni) {
            println!("VNI already exists {}", msg.vni.vni);
            return Ok(());
        }
        let mut vnis = self.vnis.clone();
        vnis.insert(msg.vni.vni, msg.vni);
        self.vnis = vnis;
        self.debug_dump();
        Ok(())
    }

    async fn del_vni(&mut self, msg: DelVniMsg) -> Result<(), String> {
        println!("DelVni {msg:?}");
        if msg.vni.vteps.len() != 0 {
            println!("VNI must not have VTEP {}", msg.vni.vni);
            return Ok(());
        }
        if !self.vnis.contains_key(&msg.vni.vni) {
            println!("VNI not exists {}", msg.vni.vni);
            return Ok(());
        }
        let mut vnis = self.vnis.clone();
        vnis.remove(&msg.vni.vni);
        self.vnis = vnis;
        self.debug_dump();
        Ok(())
    }

    async fn mod_vtep_vni(&mut self, msg: ModVtepVniMsg) -> Result<(), String> {
        println!("ModVtepVni {msg:?}");
        let mut vteps = self.vteps.clone();
        let mut vnis = self.vnis.clone();
        match vteps.get_mut(&msg.vtep.name) {
            Some(vtep) => {
                let ipv6_addr = vtep.ipv6_addr.clone();
                let last_update = vtep.last_update.clone();
                *vtep = msg.vtep;
                vtep.ipv6_addr = ipv6_addr;
                vtep.last_update = last_update;
            }
            None => {
                println!("VTEP not exists {}", msg.vtep.name);
                return Ok(());
            }
        }
        match vnis.get_mut(&msg.vni.vni) {
            Some(vni) => {
                *vni = msg.vni;
            }
            None => {
                println!("VNI not exists {}", msg.vni.vni);
                return Ok(());
            }
        }
        if let Err(e) = self.sync_bgp(&self.vteps, &vteps).await {
            println!("Sync BGP error: {e}");
        }
        self.vteps = vteps;
        self.vnis = vnis;
        self.debug_dump();
        Ok(())
    }

    async fn lch(&mut self, msg: Option<RrLchMsg>) -> Result<(), String> {
        match msg {
            Some(RrLchMsg::LocAddrChanged(msg)) => self.loc_addr_changed(msg).await,
            Some(RrLchMsg::RrRegistered(msg)) => self.rr_registered(msg).await,
            Some(RrLchMsg::AddVtep(msg)) => self.add_vtep(msg).await,
            Some(RrLchMsg::DelVtep(msg)) => self.del_vtep(msg).await,
            Some(RrLchMsg::AddVni(msg)) => self.add_vni(msg).await,
            Some(RrLchMsg::DelVni(msg)) => self.del_vni(msg).await,
            Some(RrLchMsg::ModVtepVni(msg)) => self.mod_vtep_vni(msg).await,
            Some(RrLchMsg::AuthVtep(msg)) => self.auth_vtep(msg).await,
            Some(RrLchMsg::VtepRegistered(msg)) => self.vtep_registered(msg).await,
            Some(RrLchMsg::VtepExit(msg)) => self.vtep_exit(msg).await,
            None => Err(format!("Received none lch")),
        }
    }

    async fn wait(&self) -> Result<(), String> {
        let mut bgp_ready: bool = false;
        let mut src_if_ready: bool = false;
        for _ in 0..60 {
            if !bgp_ready {
                if let Ok(_) = self.bgp_ops.ready().await {
                    bgp_ready = true;
                }
            }
            if !src_if_ready {
                if let Ok(_) = get_ifindex(&self.src_ifname).await {
                    src_if_ready = true;
                }
            }
            if bgp_ready && src_if_ready {
                return Ok(());
            }
            sleep(Duration::from_secs(1)).await;
        }
        Err(format!(
            "bgp_ready = {bgp_ready} src_if_ready = {src_if_ready}"
        ))
    }

    pub async fn run(&mut self) {
        if let Err(e) = self.wait().await {
            println!("Waiting BGP backend and source interface error: {e}");
            return;
        }
        if let Err(e) = self.init_src_ifindex().await {
            println!("Get source interface index failed: {e}");
            return;
        }
        self.bgp_ops.init(self.bgp_asnum).await;

        // RtnlHandler
        let mut rtnl_handler = RtnlHandler::new(
            self.src_ifname.clone(),
            self.src_ifindex,
            self.tx_lch.clone(),
        );
        tokio::spawn(async move {
            rtnl_handler.run().await;
        });

        // UiHandler
        let (ui_tx_lch, ui_rx_lch) = mpsc::channel(1);
        self.ui_tx_lch = Some(ui_tx_lch);
        let mut ui_handler = UiHandler::new(
            self.ui_addr.clone(),
            self.ui_port,
            self.rr_name.clone(),
            self.rr_pass.clone(),
            self.tx_lch.clone(),
            ui_rx_lch,
        );
        tokio::spawn(async move {
            ui_handler.run().await;
        });

        // TlsRchListener
        let rch_addr = IpAddr::V6(self.rch_addr);
        let mut rch_listener = match TlsRchListener::new(rch_addr, self.rch_port).await {
            Ok(rch_listener) => rch_listener,
            Err(e) => {
                println!("Rch listener new error: {e}");
                return;
            }
        };

        loop {
            tokio::select! {
                ret = rch_listener.rch_accept::<RrVtepRchMsg, VtepRrRchMsg>() => {
                    match ret {
                        Ok((tx_rch, rx_rch, IpAddr::V6(addr))) => {
                            let (vtep_tx_lch, vtep_rx_lch) = mpsc::channel(1);
                            let _ = self.vtep_tx_lchs.insert(addr.clone(), vtep_tx_lch);
                            let mut vtep_handler = VtepHandler::new(
                                addr,
                                tx_rch,
                                rx_rch,
                                self.tx_lch.clone(),
                                vtep_rx_lch,
                            );
                            tokio::spawn(async move {
                                vtep_handler.run().await;
                            });
                        }
                        Ok((_, _, IpAddr::V4(_))) => {
                            println!("Address family mismatch");
                        }
                        Err(e) => {
                            println!("Rch accept error: {e}");
                        }
                    }
                }
                msg = self.rx_lch.recv() => {
                    if let Err(e) = self.lch(msg).await {
                        println!("lch error: {e}");
                        break;
                    }
                }
            };
        }

        println!("context exit");
    }
}
