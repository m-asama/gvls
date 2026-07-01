// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::Ipv6Addr;
use std::time::{Duration, Instant};

use remoc::rch;
use tokio::sync::mpsc;

use libgvls::{
    HELLO_INTERVAL, HELLO_TIMEOUT, HelloMsg, NeighsUpdatedMsg, RegisterVtepReqMsg, RrVtepRchMsg,
    VtepRrRchMsg, rch_connect_host,
};

use crate::{
    LocAddrChangedMsg, RemAddrChangedMsg, RrLchMsg, UpdateNeighsMsg, VtepLchMsg, VtepRegisteredMsg,
};

pub struct RrHandler {
    vtep_name: String,
    vtep_pass: String,
    rr_index: usize,
    rr_host: String,
    rr_port: u16,
    loc_addr: Option<Ipv6Addr>,
    conn_try_count: u32,
    conn_try_next: Instant,
    hello_next: Instant,
    hello_last: Instant,
    tx_rch: Option<rch::base::Sender<VtepRrRchMsg>>,
    rx_rch: Option<rch::base::Receiver<RrVtepRchMsg>>,
    tx_lch: mpsc::Sender<VtepLchMsg>,
    rx_lch: mpsc::Receiver<RrLchMsg>,
}

impl RrHandler {
    pub fn new(
        vtep_name: String,
        vtep_pass: String,
        rr_index: usize,
        rr_host: String,
        rr_port: u16,
        tx_lch: mpsc::Sender<VtepLchMsg>,
        rx_lch: mpsc::Receiver<RrLchMsg>,
    ) -> Self {
        Self {
            vtep_name,
            vtep_pass,
            rr_index,
            rr_host,
            rr_port,
            loc_addr: None,
            conn_try_count: 0,
            conn_try_next: Instant::now(),
            hello_next: Instant::now(),
            hello_last: Instant::now(),
            tx_rch: None,
            rx_rch: None,
            tx_lch,
            rx_lch,
        }
    }

    fn reset(&mut self, fast: bool) {
        let wait = if fast { 5 } else { 60 };
        self.tx_rch = None;
        self.rx_rch = None;
        self.conn_try_count = 0;
        self.conn_try_next = Instant::now() + Duration::from_secs(wait);
    }

    fn retry(&mut self) {
        self.tx_rch = None;
        self.rx_rch = None;
        self.conn_try_count += 1;
        let n = if self.conn_try_count < 10 {
            self.conn_try_count
        } else {
            10
        };
        self.conn_try_next = Instant::now() + Duration::from_secs(2u64.pow(n));
    }

    async fn send_rem_addr_changed(&mut self, rem_addr: Option<Ipv6Addr>) {
        let msg = VtepLchMsg::RemAddrChanged(RemAddrChangedMsg {
            rr_index: self.rr_index,
            rem_addr: rem_addr,
        });
        let _ = self.tx_lch.send(msg).await;
    }

    async fn send_hello(&mut self) -> Result<(), String> {
        if Instant::now() < self.hello_next {
            return Ok(());
        }
        if let Some(tx_rch) = &mut self.tx_rch {
            let req = VtepRrRchMsg::Hello(HelloMsg {});
            println!("Sending hello {}", self.rr_host);
            if let Err(e) = tx_rch.send(req).await {
                return Err(format!("Send hello error: {e}"));
            }
            println!("Sent hello {}", self.rr_host);
            self.hello_next = Instant::now() + Duration::from_secs(HELLO_INTERVAL);
        }
        Ok(())
    }

    fn hello_timeout(&self) -> bool {
        if self.hello_last < Instant::now() - Duration::from_secs(HELLO_TIMEOUT) {
            true
        } else {
            false
        }
    }

    async fn conn_try(&mut self) {
        if Instant::now() < self.conn_try_next {
            return;
        }
        if self.loc_addr.is_none() {
            return;
        }

        println!("Starting RR connection attempt ({})", self.rr_host);

        // rch connect
        let rr_host = self.rr_host.clone();
        let rr_port = self.rr_port;
        let loc_addr = &self.loc_addr.unwrap().clone();
        let (tx_rch, rx_rch, rem_addr) =
            match rch_connect_host::<VtepRrRchMsg, RrVtepRchMsg>(rr_host, rr_port, *loc_addr).await
            {
                Ok((tx_rch, rx_rch, rem_addr)) => (tx_rch, rx_rch, rem_addr),
                Err(e) => {
                    println!("RR remoc connect ({}) failed: {e}", self.rr_host);
                    self.retry();
                    return;
                }
            };
        self.tx_rch = Some(tx_rch);
        self.rx_rch = Some(rx_rch);
        self.conn_try_count = 0;
        self.conn_try_next = Instant::now();

        // RegisterVtepReq
        let (rep_tx, mut rep_rx) = rch::mpsc::channel(1);
        let req = VtepRrRchMsg::RegisterVtepReq(RegisterVtepReqMsg {
            name: self.vtep_name.clone(),
            password: self.vtep_pass.clone(),
            rep_tx,
        });
        println!("Sending RegisterVtepReq to RR ({})", self.rr_host);
        if let Err(e) = self.tx_rch.as_mut().unwrap().send(req).await {
            println!("Send RegisterVtepReq to RR ({}) failed: {e}", self.rr_host);
            self.retry();
            return;
        }
        println!("Sent RegisterVtepReq to RR ({})", self.rr_host);

        // RegisterVtepRep
        println!("Receiving RegisterVtepRep from RR ({})", self.rr_host);
        let mut rep = match rep_rx.recv().await {
            Ok(Some(rep)) => rep,
            Ok(None) => {
                println!("RegisterVtepRep channel closed by RR ({})", self.rr_host);
                self.retry();
                return;
            }
            Err(e) => {
                println!(
                    "Receive RegisterVtepRep from RR ({}) failed: {e}",
                    self.rr_host
                );
                self.retry();
                return;
            }
        };
        println!("Received RegisterVtepRep from RR ({})", self.rr_host);
        let vtep_id = rep.vtep_id;
        let mut neighs = HashSet::<Ipv6Addr>::new();
        while let Ok(Some(neigh)) = rep.neigh_rx.recv().await {
            neighs.insert(neigh);
        }
        let bgp_pass = rep.bgp_pass.clone();
        drop(rep);

        // VtepRegistered
        let msg = VtepLchMsg::VtepRegistered(VtepRegisteredMsg {
            vtep_id: vtep_id,
            rr_index: self.rr_index,
            bgp_pass,
            neighs: neighs,
        });
        if let Err(e) = self.tx_lch.send(msg).await {
            println!(
                "Send VtepRegistered to context ({}) failed: {e}",
                self.rr_host
            );
            self.retry();
            return;
        }

        self.send_rem_addr_changed(Some(rem_addr)).await;
        self.hello_last = Instant::now();
    }

    async fn neighs_updated(&mut self, mut msg: NeighsUpdatedMsg) -> Result<(), String> {
        let mut neighs = HashSet::<Ipv6Addr>::new();
        while let Ok(Some(neigh)) = msg.neigh_rx.recv().await {
            neighs.insert(neigh);
        }
        let msg = VtepLchMsg::UpdateNeighs(UpdateNeighsMsg {
            rr_index: self.rr_index,
            neighs: neighs,
        });
        if let Err(e) = self.tx_lch.send(msg).await {
            println!(
                "Send UpdateNeighs to context ({}) failed: {e}",
                self.rr_host
            );
        }
        Ok(())
    }

    async fn loc_addr_changed(&mut self, msg: LocAddrChangedMsg) -> Result<(), String> {
        let tx_rch_is_some = self.tx_rch.is_some();
        let rx_rch_is_some = self.rx_rch.is_some();
        self.reset(msg.loc_addr.is_some());
        if tx_rch_is_some || rx_rch_is_some {
            self.send_rem_addr_changed(None).await;
        }
        self.loc_addr = msg.loc_addr;
        Ok(())
    }

    async fn rch(
        &mut self,
        msg: Result<Option<RrVtepRchMsg>, rch::base::RecvError>,
    ) -> Result<(), String> {
        match msg {
            Ok(Some(RrVtepRchMsg::Hello(_))) => {
                println!("Received hello {}", self.rr_host);
                self.hello_last = Instant::now();
                Ok(())
            }
            Ok(Some(RrVtepRchMsg::NeighsUpdated(msg))) => self.neighs_updated(msg).await,
            Ok(None) => Err(format!("Received none rch")),
            Err(e) => Err(format!("Receive error: {e}")),
        }
    }

    async fn lch(&mut self, msg: Option<RrLchMsg>) -> Result<(), String> {
        match msg {
            Some(RrLchMsg::LocAddrChanged(msg)) => self.loc_addr_changed(msg).await,
            None => Err(format!("Received none lch")),
        }
    }

    pub async fn run(&mut self) {
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                msg = async {
                    match self.rx_rch.as_mut() {
                        Some(rx_rch) => rx_rch.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Err(e) = self.rch(msg).await {
                        println!("rch {} error: {e}", self.rr_host);
                        self.reset(false);
                        self.send_rem_addr_changed(None).await;
                    }
                },
                msg = self.rx_lch.recv() => {
                    if let Err(e) = self.lch(msg).await {
                        println!("lch {} error: {e}", self.rr_host);
                        break;
                    }
                },
                _ = timer.tick() => {
                    if self.tx_rch.is_none() || self.rx_rch.is_none() {
                        self.conn_try().await;
                    }
                    if self.tx_rch.is_some() {
                        if let Err(e) = self.send_hello().await {
                            println!("hello {} error: {e}", self.rr_host);
                            self.reset(false);
                            self.send_rem_addr_changed(None).await;
                        }
                    }
                    if self.rx_rch.is_some() {
                        if self.hello_timeout() {
                            println!("hello timeout {}", self.rr_host);
                            self.reset(false);
                            self.send_rem_addr_changed(None).await;
                        }
                    }
                },
            }
        }
        println!("rr handler exit {}", self.rr_host);
    }
}
