// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use remoc::rch;
use tokio::sync::mpsc;

use libgvls::{
    HELLO_INTERVAL, HELLO_TIMEOUT, HelloMsg, RegisterRrRepMsg, RrRchMsg, UiRchMsg,
    VtepStateUpdatedMsg,
};

use crate::{AuthRrReq, RrExitMsg, RrLchMsg, RrRegisteredMsg, UiLchMsg, UpdateVtepStateMsg};

pub struct RrHandler {
    addr: Ipv4Addr,
    name: String,
    hello_next: Instant,
    hello_last: Instant,
    tx_rch: rch::base::Sender<RrRchMsg>,
    rx_rch: rch::base::Receiver<UiRchMsg>,
    tx_lch: mpsc::Sender<UiLchMsg>,
    rx_lch: mpsc::Receiver<RrLchMsg>,
}

impl RrHandler {
    pub fn new(
        addr: Ipv4Addr,
        tx_rch: rch::base::Sender<RrRchMsg>,
        rx_rch: rch::base::Receiver<UiRchMsg>,
        tx_lch: mpsc::Sender<UiLchMsg>,
        rx_lch: mpsc::Receiver<RrLchMsg>,
    ) -> Self {
        Self {
            addr,
            name: String::new(),
            hello_next: Instant::now(),
            hello_last: Instant::now(),
            tx_rch: tx_rch,
            rx_rch: rx_rch,
            tx_lch,
            rx_lch,
        }
    }

    async fn send_hello(&mut self) -> Result<(), String> {
        if self.name == "" {
            return Ok(());
        }
        if Instant::now() < self.hello_next {
            return Ok(());
        }
        let req = RrRchMsg::Hello(HelloMsg {});
        println!("Sending hello {} {}", self.name, self.addr);
        if let Err(e) = self.tx_rch.send(req).await {
            return Err(format!("Send hello error: {e}"));
        }
        println!("Sent hello {} {}", self.name, self.addr);
        self.hello_next = Instant::now() + Duration::from_secs(HELLO_INTERVAL);
        Ok(())
    }

    fn hello_timeout(&self) -> bool {
        if self.hello_last < Instant::now() - Duration::from_secs(HELLO_TIMEOUT) {
            true
        } else {
            false
        }
    }

    async fn vtep_state_updated(&mut self, msg: VtepStateUpdatedMsg) -> Result<(), String> {
        let msg = UiLchMsg::UpdateVtepState(UpdateVtepStateMsg {
            rr_name: self.name.clone(),
            vtep_name: msg.name,
            ipv6_addr: msg.ipv6_addr,
            last_update: msg.last_update,
        });
        if let Err(e) = self.tx_lch.send(msg).await {
            return Err(format!("Send VTEP state updated error: {e}"));
        }
        Ok(())
    }

    async fn rch(
        &mut self,
        msg: Result<Option<UiRchMsg>, rch::base::RecvError>,
    ) -> Result<(), String> {
        match msg {
            Ok(Some(UiRchMsg::Hello(_))) => {
                println!("Received hello {} {}", self.name, self.addr);
                self.hello_last = Instant::now();
                Ok(())
            }
            Ok(Some(UiRchMsg::RegisterRrReq(_))) => {
                // RegisterRrReq は run で処理する最初のみ
                println!("Unexpected RegisterRrReq after RR registration");
                Ok(())
            }
            Ok(Some(UiRchMsg::VtepStateUpdated(msg))) => self.vtep_state_updated(msg).await,
            Ok(None) => Err(format!("Received none rch")),
            Err(e) => Err(format!("Receive error: {e}")),
        }
    }

    async fn lch(&mut self, msg: Option<RrLchMsg>) -> Result<(), String> {
        match msg {
            Some(RrLchMsg::VtepAdded(vtep)) => self
                .tx_rch
                .send(RrRchMsg::VtepAdded(libgvls::VtepAddedMsg { vtep }))
                .await
                .map_err(|e| format!("Send VtepAdded error: {e}")),
            Some(RrLchMsg::VtepDeleted(vtep)) => self
                .tx_rch
                .send(RrRchMsg::VtepDeleted(libgvls::VtepDeletedMsg { vtep }))
                .await
                .map_err(|e| format!("Send VtepDeleted error: {e}")),
            Some(RrLchMsg::VniAdded(vni)) => self
                .tx_rch
                .send(RrRchMsg::VniAdded(libgvls::VniAddedMsg { vni }))
                .await
                .map_err(|e| format!("Send VniAdded error: {e}")),
            Some(RrLchMsg::VniDeleted(vni)) => self
                .tx_rch
                .send(RrRchMsg::VniDeleted(libgvls::VniDeletedMsg { vni }))
                .await
                .map_err(|e| format!("Send VniDeleted error: {e}")),
            Some(RrLchMsg::VtepVniModified { vtep, vni }) => self
                .tx_rch
                .send(RrRchMsg::VtepVniModified(libgvls::VtepVniModifiedMsg {
                    vtep,
                    vni,
                }))
                .await
                .map_err(|e| format!("Send VtepVniModified error: {e}")),
            None => Err(format!("Received none lch")),
        }
    }

    pub async fn run(&mut self) {
        // RegisterRrReq
        let rreq = if let Ok(Some(UiRchMsg::RegisterRrReq(rreq))) = self.rx_rch.recv().await {
            rreq
        } else {
            println!("First msg not RegisterRrReq");
            return;
        };

        // AuthRrReq
        let (rep_tx, mut rep_rx) = mpsc::channel(1);
        let lreq = UiLchMsg::AuthRr(AuthRrReq {
            name: rreq.name,
            password: rreq.password,
            addr: self.addr.clone(),
            rep_tx,
        });
        println!("Sending AuthRrReq {} {}", self.name, self.addr);
        if let Err(e) = self.tx_lch.send(lreq).await {
            println!("Send lch error: {e}");
            return;
        }
        println!("Sent AuthRrReq {} {}", self.name, self.addr);

        // AuthRrRep
        println!("Receiving AuthRrRep {} {}", self.name, self.addr);
        let lrep = if let Some(lrep) = rep_rx.recv().await {
            lrep
        } else {
            println!("Recv lch none");
            return;
        };
        println!("Received AuthRrRep {} {}", self.name, self.addr);
        match lrep.rr_name {
            Ok(rr_name) => {
                self.name = rr_name;
            }
            Err(e) => {
                println!("Auth failed: {e}");
                return;
            }
        }

        // RegisterRrRep
        let (vtep_tx, vtep_rx) = rch::mpsc::channel(1);
        let (vni_tx, vni_rx) = rch::mpsc::channel(1);
        let rrep = RegisterRrRepMsg {
            vtep_rx: vtep_rx,
            vni_rx: vni_rx,
        };
        if let Err(e) = rreq.rep_tx.send(rrep).await {
            println!("Send rch error: {e}");
            return;
        }
        let (vteps, vnis) = (lrep.vteps, lrep.vnis);
        for vtep in vteps {
            if let Err(e) = vtep_tx.send(vtep).await {
                println!("Send VTEP error: {e}");
                return;
            }
        }
        drop(vtep_tx);
        for vni in vnis {
            if let Err(e) = vni_tx.send(vni).await {
                println!("Send VNI error: {e}");
                return;
            }
        }
        drop(vni_tx);

        // RrRegistered
        let lmsg = UiLchMsg::RrRegistered(RrRegisteredMsg {
            name: self.name.clone(),
        });
        println!("Sending RrRegistered {}", self.name);
        if let Err(e) = self.tx_lch.send(lmsg).await {
            println!("Send lch error: {e}");
            return;
        }
        println!("Sent RrRegistered {}", self.name);

        println!("Registered: {} {}", self.name, self.addr);
        self.hello_last = Instant::now();

        let mut timer = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                msg = self.rx_rch.recv() => {
                    if let Err(e) = self.rch(msg).await {
                        println!("rch error: {e}");
                        break;
                    }
                },
                msg = self.rx_lch.recv() => {
                    if let Err(e) = self.lch(msg).await {
                        println!("lch error: {e}");
                        break;
                    }
                },
                _ = timer.tick() => {
                    if let Err(e) = self.send_hello().await {
                        println!("hello error: {e}");
                        break;
                    }
                    if self.hello_timeout() {
                        println!("hello timeout");
                        break;
                    }
                }
            }
        }

        // RrExit
        println!("Sending RrExit {}", self.name);
        let lmsg = UiLchMsg::RrExit(RrExitMsg {
            name: self.name.clone(),
        });
        let _ = self.tx_lch.send(lmsg).await;
        println!("Sent RrExit {}", self.name);

        println!("Exit: {} {}", self.name, self.addr);
    }
}
