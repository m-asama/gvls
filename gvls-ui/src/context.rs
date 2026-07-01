// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Instant;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{
        SaltString,
        rand_core::{OsRng, RngCore},
    },
};
use tokio::sync::mpsc;

use libgvls::{Account, Permission, Rr, TlsRchListener, UiRchMsg, UiRrRchMsg, Vni, Vtep};

use crate::{
    AuthAccountRep, AuthAccountReq, AuthRrRep, AuthRrReq, ChangePasswordReq, Config,
    CreateAccountReq, CreateVniReq, CreateVtepReq, DeleteAccountReq, DeleteVniReq, DeleteVtepReq,
    GetAccountByIdRep, GetAccountByIdReq, ListAccountsRep, ListAccountsReq, ListVnisRep,
    ListVnisReq, ListVtepsRep, ListVtepsReq, OpRep, RrExitMsg, RrHandler, RrLchMsg,
    RrRegisteredMsg, UiHandler, UiLchMsg, UpdateAccountPermReq, UpdateVniVtepsReq,
    UpdateVtepStateMsg, UpdateVtepVnisReq,
};

#[derive(Debug)]
pub struct Context {
    http_addr: IpAddr,
    http_port: u16,
    rch_addr: IpAddr,
    rch_port: u16,
    pub db_username: String,
    pub db_password: String,
    pub db_host: String,
    pub db_port: String,
    pub db_name: String,
    pub accounts: HashMap<String, Account>,
    pub rrs: HashMap<String, Rr>,
    pub vteps: HashMap<String, Vtep>,
    pub vnis: HashMap<i32, Vni>,
    vtep_states: HashMap<String, HashMap<String, (Option<Ipv6Addr>, String)>>,
    tx_lch: mpsc::Sender<UiLchMsg>,
    rx_lch: mpsc::Receiver<UiLchMsg>,
    rr_tx_lchs: HashMap<Ipv4Addr, mpsc::Sender<RrLchMsg>>,
}

impl Context {
    pub fn from_conf(conf: Config) -> Result<Self, String> {
        let (tx_lch, rx_lch) = mpsc::channel(8);
        Ok(Self {
            http_addr: conf.http_addr,
            http_port: conf.http_port,
            rch_addr: conf.rch_addr,
            rch_port: conf.rch_port,
            db_username: conf.db_username,
            db_password: conf.db_password,
            db_host: conf.db_host,
            db_port: conf.db_port,
            db_name: conf.db_name,
            accounts: HashMap::<String, Account>::new(),
            rrs: HashMap::<String, Rr>::new(),
            vteps: HashMap::<String, Vtep>::new(),
            vnis: HashMap::<i32, Vni>::new(),
            vtep_states: HashMap::<String, HashMap<String, (Option<Ipv6Addr>, String)>>::new(),
            tx_lch,
            rx_lch,
            rr_tx_lchs: HashMap::<Ipv4Addr, mpsc::Sender<RrLchMsg>>::new(),
        })
    }

    fn find_account_by_id(&self, account_id: i32) -> Option<&Account> {
        self.accounts
            .values()
            .find(|account| account.id == account_id)
    }

    fn find_account_mut_by_id(&mut self, account_id: i32) -> Option<&mut Account> {
        let mail_addr = self
            .accounts
            .values()
            .find(|account| account.id == account_id)
            .map(|account| account.mail_addr.clone())?;
        self.accounts.get_mut(&mail_addr)
    }

    pub fn parse_permission(&self, perm_code: i32) -> Result<Permission, String> {
        match perm_code {
            1 => Ok(Permission::Admin),
            2 => Ok(Permission::Free),
            3 => Ok(Permission::Pro),
            _ => Err("Unknown permission".to_string()),
        }
    }

    pub fn permission_code(perm: Permission) -> i32 {
        match perm {
            Permission::Admin => 1,
            Permission::Free => 2,
            Permission::Pro => 3,
        }
    }

    fn can_manage_account(&self, requester_id: i32, owner_account_id: i32) -> bool {
        match self.find_account_by_id(requester_id) {
            Some(account) if account.perm == Permission::Admin => true,
            Some(account) => account.id == owner_account_id,
            None => false,
        }
    }

    fn check_resource_limit(
        &self,
        owner: &Account,
        next_vni_count: usize,
        next_vtep_count: usize,
    ) -> Result<(), String> {
        if let Some((vni_limit, vtep_limit)) = owner.perm.limits() {
            if next_vni_count > vni_limit {
                return Err(format!(
                    "{} account can own at most {vni_limit} VNI(s)",
                    self.perm_label(owner.perm)
                ));
            }
            if next_vtep_count > vtep_limit {
                return Err(format!(
                    "{} account can own at most {vtep_limit} VTEP(s)",
                    self.perm_label(owner.perm)
                ));
            }
        }
        Ok(())
    }

    fn perm_label(&self, perm: Permission) -> &'static str {
        match perm {
            Permission::Admin => "Admin",
            Permission::Free => "Free",
            Permission::Pro => "Pro",
        }
    }

    fn hash_password(&self, password: &str) -> Result<String, String> {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| format!("Password hash error: {e}"))
    }

    async fn next_vni(&self) -> Result<i32, String> {
        self.reserve_vni_id().await
    }

    async fn next_vtep_name(&self) -> Result<(i32, String), String> {
        let next_id = self.reserve_vtep_id().await?;
        Ok((next_id, format!("gv{next_id:04}")))
    }

    fn generate_bgp_pass(&self) -> String {
        const BGP_PASS_CHARS: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = OsRng;
        let mut bgp_pass = String::with_capacity(9);
        for _ in 0..9 {
            let idx = (rng.next_u32() as usize) % BGP_PASS_CHARS.len();
            bgp_pass.push(BGP_PASS_CHARS[idx] as char);
        }
        bgp_pass
    }

    async fn send_op(rep_tx: mpsc::Sender<OpRep>, ok: bool, message: impl Into<String>) {
        let _ = rep_tx
            .send(OpRep {
                ok,
                message: message.into(),
            })
            .await;
    }

    async fn notify_rrs(&mut self, msg: RrLchMsg) {
        let mut failed = Vec::new();
        for (addr, tx) in &self.rr_tx_lchs {
            if tx.send(msg.clone()).await.is_err() {
                failed.push(*addr);
            }
        }
        for addr in failed {
            println!("Remove disconnected RR channel: {}", addr);
            self.rr_tx_lchs.remove(&addr);
        }
    }

    async fn get_account_by_id(&mut self, msg: GetAccountByIdReq) -> Result<(), String> {
        let account = self.find_account_by_id(msg.account_id).cloned();
        if let Err(e) = msg.rep_tx.send(GetAccountByIdRep { account }).await {
            println!("send account error: {e}");
        }
        Ok(())
    }

    async fn auth_account(&mut self, msg: AuthAccountReq) -> Result<(), String> {
        let mut account_id: Result<i32, String> = Err("Auth failed".to_string());
        if let Some(account) = self.accounts.get(&msg.mail_addr) {
            if let Ok(parsed_hash) = PasswordHash::new(&account.password) {
                if Argon2::default()
                    .verify_password(msg.password.as_bytes(), &parsed_hash)
                    .is_ok()
                {
                    account_id = Ok(account.id);
                }
            }
        }
        if let Err(e) = msg.rep_tx.send(AuthAccountRep { account_id }).await {
            println!("Send AuthAccountRep failed: {e}");
        }
        Ok(())
    }

    async fn auth_rr(&mut self, msg: AuthRrReq) -> Result<(), String> {
        let mut rr_name: Result<String, String> = Err("Auth failed".to_string());
        let mut rr_id = -1;
        if let Some(rr) = self.rrs.get(&msg.name) {
            if let Ok(parsed_hash) = PasswordHash::new(&rr.password) {
                if Argon2::default()
                    .verify_password(msg.password.as_bytes(), &parsed_hash)
                    .is_ok()
                {
                    rr_name = Ok(rr.name.clone());
                    rr_id = rr.id;
                }
            }
        }
        if let Ok(rr_name) = &rr_name {
            println!("RR authenticated: {} from {}", rr_name, msg.addr);
            if let Some(rr) = self.rrs.get_mut(rr_name) {
                if let Some(ipv4_addr) = &rr.ipv4_addr
                    && *ipv4_addr != msg.addr
                {
                    println!("RR {} moved from {} to {}", rr_name, ipv4_addr, msg.addr);
                    self.rr_tx_lchs.remove(ipv4_addr);
                }
                rr.ipv4_addr = Some(msg.addr);
                rr.last_update = Some(Instant::now());
            }
        }
        let vteps: Vec<Vtep> = self.vteps.clone().into_values().collect();
        let vnis: Vec<Vni> = self.vnis.clone().into_values().collect();
        if let Err(e) = msg
            .rep_tx
            .send(AuthRrRep {
                rr_name,
                rr_id,
                vteps,
                vnis,
            })
            .await
        {
            println!("Send AuthRrRep failed: {e}");
        }
        Ok(())
    }

    async fn list_vteps(&mut self, msg: ListVtepsReq) -> Result<(), String> {
        let Some(requester) = self.find_account_by_id(msg.requester_id).cloned() else {
            let _ = msg
                .rep_tx
                .send(ListVtepsRep {
                    requester: None,
                    accounts: Vec::new(),
                    vteps: Vec::new(),
                    assignable_vnis: Vec::new(),
                    vtep_states: self.vtep_states.clone(),
                })
                .await;
            return Ok(());
        };
        let accounts = self.accounts.values().cloned().collect::<Vec<_>>();
        let mut vteps = self.vteps.values().cloned().collect::<Vec<_>>();
        let mut assignable_vnis = self.vnis.values().cloned().collect::<Vec<_>>();
        if requester.perm != Permission::Admin {
            vteps.retain(|vtep| vtep.account_id == requester.id);
            assignable_vnis.retain(|vni| vni.account_id == requester.id);
        }
        vteps.sort_by(|a, b| a.name.cmp(&b.name));
        assignable_vnis.sort_by(|a, b| a.vni.cmp(&b.vni));
        let mut accounts = accounts;
        accounts.sort_by(|a, b| a.mail_addr.cmp(&b.mail_addr));
        let _ = msg
            .rep_tx
            .send(ListVtepsRep {
                requester: Some(requester),
                accounts,
                vteps,
                assignable_vnis,
                vtep_states: self.vtep_states.clone(),
            })
            .await;
        Ok(())
    }

    async fn create_vtep(&mut self, msg: CreateVtepReq) -> Result<(), String> {
        let Some(requester) = self.find_account_by_id(msg.requester_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Account not found").await;
            return Ok(());
        };
        let owner_account_id = if requester.perm == Permission::Admin {
            msg.owner_account_id.unwrap_or(requester.id)
        } else {
            requester.id
        };
        let Some(owner) = self.find_account_by_id(owner_account_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Owner account not found").await;
            return Ok(());
        };
        if msg.password.is_empty() {
            Self::send_op(msg.rep_tx, false, "VTEP password is required").await;
            return Ok(());
        }
        if let Err(e) = self.check_resource_limit(&owner, owner.vnis.len(), owner.vteps.len() + 1) {
            Self::send_op(msg.rep_tx, false, e).await;
            return Ok(());
        }
        let (reserved_id, name) = self.next_vtep_name().await?;
        let bgp_pass = self.generate_bgp_pass();
        let password = self.hash_password(&msg.password)?;
        let vtep_id = self
            .insert_vtep(
                reserved_id,
                &name,
                &msg.description,
                &password,
                &bgp_pass,
                owner_account_id,
            )
            .await?;
        let vtep = Vtep {
            id: vtep_id,
            name,
            description: msg.description,
            password,
            bgp_pass,
            account_id: owner_account_id,
            vnis: Default::default(),
            ipv6_addr: None,
            last_update: None,
        };
        self.vteps.insert(vtep.name.clone(), vtep.clone());
        let owner_to_sync = if let Some(owner) = self.find_account_mut_by_id(owner_account_id) {
            owner.vteps.insert(vtep.name.clone());
            Some(owner.clone())
        } else {
            None
        };
        if let Some(owner) = owner_to_sync {
            self.sync_account(&owner).await?;
        }
        self.notify_rrs(RrLchMsg::VtepAdded(vtep.clone())).await;
        println!(
            "Created VTEP {} for account {} by account {}",
            vtep.name, owner_account_id, requester.id
        );
        Self::send_op(msg.rep_tx, true, "VTEP created").await;
        Ok(())
    }

    async fn update_vtep_vnis(&mut self, msg: UpdateVtepVnisReq) -> Result<(), String> {
        let Some(vtep) = self.vteps.get(&msg.vtep_name).cloned() else {
            Self::send_op(msg.rep_tx, false, "VTEP not found").await;
            return Ok(());
        };
        if !self.can_manage_account(msg.requester_id, vtep.account_id) {
            Self::send_op(msg.rep_tx, false, "Permission denied").await;
            return Ok(());
        }
        let Some(requester) = self.find_account_by_id(msg.requester_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Account not found").await;
            return Ok(());
        };
        for vni_num in &msg.vnis {
            let Some(vni) = self.vnis.get(vni_num) else {
                Self::send_op(msg.rep_tx, false, format!("VNI {vni_num} not found")).await;
                return Ok(());
            };
            if requester.perm != Permission::Admin && vni.account_id != requester.id {
                Self::send_op(msg.rep_tx, false, "Cannot attach another account's VNI").await;
                return Ok(());
            }
        }

        let mut requested_vnis = msg.vnis.clone();
        if requester.perm != Permission::Admin {
            for vni_num in &vtep.vnis {
                if let Some(vni) = self.vnis.get(vni_num)
                    && vni.account_id != requester.id
                {
                    requested_vnis.insert(*vni_num);
                }
            }
        }

        let old_vnis = vtep.vnis.clone();
        let old_description = vtep.description.clone();
        let removed = old_vnis
            .difference(&requested_vnis)
            .copied()
            .collect::<Vec<_>>();
        let added = requested_vnis
            .difference(&old_vnis)
            .copied()
            .collect::<Vec<_>>();
        let description_changed = old_description != msg.description;
        if removed.is_empty() && added.is_empty() && !description_changed {
            Self::send_op(msg.rep_tx, true, "No changes").await;
            return Ok(());
        }

        if let Some(vtep_mut) = self.vteps.get_mut(&msg.vtep_name) {
            vtep_mut.vnis = requested_vnis;
            vtep_mut.description = msg.description.clone();
        }
        for vni_num in &removed {
            if let Some(vni_mut) = self.vnis.get_mut(vni_num) {
                vni_mut.vteps.remove(&msg.vtep_name);
            }
        }
        for vni_num in &added {
            if let Some(vni_mut) = self.vnis.get_mut(vni_num) {
                vni_mut.vteps.insert(msg.vtep_name.clone());
            }
        }

        let new_vtep = self.vteps.get(&msg.vtep_name).cloned().unwrap();
        self.sync_vtep(&new_vtep).await?;
        let mut changed = removed.clone();
        changed.extend(added.clone());
        changed.sort_unstable();
        changed.dedup();
        for vni_num in &changed {
            if let Some(vni) = self.vnis.get(vni_num).cloned() {
                self.sync_vni(&vni).await?;
                self.notify_rrs(RrLchMsg::VtepVniModified {
                    vtep: new_vtep.clone(),
                    vni,
                })
                .await;
            }
        }
        println!(
            "Updated VTEP {} by account {}: description_changed={} add_vnis={:?} del_vnis={:?}",
            msg.vtep_name, requester.id, description_changed, added, removed
        );
        Self::send_op(msg.rep_tx, true, "VTEP updated").await;
        Ok(())
    }

    async fn delete_vtep(&mut self, msg: DeleteVtepReq) -> Result<(), String> {
        let Some(existing) = self.vteps.get(&msg.vtep_name).cloned() else {
            Self::send_op(msg.rep_tx, false, "VTEP not found").await;
            return Ok(());
        };
        if !self.can_manage_account(msg.requester_id, existing.account_id) {
            Self::send_op(msg.rep_tx, false, "Permission denied").await;
            return Ok(());
        }
        let mut deleted_vtep = existing.clone();
        let affected_vnis = existing.vnis.iter().copied().collect::<Vec<_>>();
        if let Some(vtep_mut) = self.vteps.get_mut(&msg.vtep_name) {
            vtep_mut.vnis.clear();
        }
        deleted_vtep.vnis.clear();
        self.sync_vtep(&deleted_vtep).await?;
        for vni_num in &affected_vnis {
            if let Some(vni_mut) = self.vnis.get_mut(vni_num) {
                vni_mut.vteps.remove(&msg.vtep_name);
            }
        }
        for vni_num in &affected_vnis {
            if let Some(vni) = self.vnis.get(vni_num).cloned() {
                self.sync_vni(&vni).await?;
                self.notify_rrs(RrLchMsg::VtepVniModified {
                    vtep: deleted_vtep.clone(),
                    vni,
                })
                .await;
            }
        }
        let owner_to_sync = if let Some(owner) = self.find_account_mut_by_id(existing.account_id) {
            owner.vteps.remove(&msg.vtep_name);
            Some(owner.clone())
        } else {
            None
        };
        if let Some(owner) = owner_to_sync {
            self.sync_account(&owner).await?;
        }
        self.delete_vtep_row(existing.id).await?;
        self.vteps.remove(&msg.vtep_name);
        self.notify_rrs(RrLchMsg::VtepDeleted(deleted_vtep)).await;
        println!(
            "Deleted VTEP {} from account {} by account {}",
            msg.vtep_name, existing.account_id, msg.requester_id
        );
        Self::send_op(msg.rep_tx, true, "VTEP deleted").await;
        Ok(())
    }

    async fn list_vnis(&mut self, msg: ListVnisReq) -> Result<(), String> {
        let Some(requester) = self.find_account_by_id(msg.requester_id).cloned() else {
            let _ = msg
                .rep_tx
                .send(ListVnisRep {
                    requester: None,
                    accounts: Vec::new(),
                    vnis: Vec::new(),
                    assignable_vteps: Vec::new(),
                })
                .await;
            return Ok(());
        };
        let accounts = self.accounts.values().cloned().collect::<Vec<_>>();
        let mut vnis = self.vnis.values().cloned().collect::<Vec<_>>();
        let mut assignable_vteps = self.vteps.values().cloned().collect::<Vec<_>>();
        if requester.perm != Permission::Admin {
            vnis.retain(|vni| vni.account_id == requester.id);
            assignable_vteps.retain(|vtep| vtep.account_id == requester.id);
        }
        vnis.sort_by(|a, b| a.vni.cmp(&b.vni));
        assignable_vteps.sort_by(|a, b| a.name.cmp(&b.name));
        let mut accounts = accounts;
        accounts.sort_by(|a, b| a.mail_addr.cmp(&b.mail_addr));
        let _ = msg
            .rep_tx
            .send(ListVnisRep {
                requester: Some(requester),
                accounts,
                vnis,
                assignable_vteps,
            })
            .await;
        Ok(())
    }

    async fn create_vni(&mut self, msg: CreateVniReq) -> Result<(), String> {
        let Some(requester) = self.find_account_by_id(msg.requester_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Account not found").await;
            return Ok(());
        };
        let owner_account_id = if requester.perm == Permission::Admin {
            msg.owner_account_id.unwrap_or(requester.id)
        } else {
            requester.id
        };
        let Some(owner) = self.find_account_by_id(owner_account_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Owner account not found").await;
            return Ok(());
        };
        if let Err(e) = self.check_resource_limit(&owner, owner.vnis.len() + 1, owner.vteps.len()) {
            Self::send_op(msg.rep_tx, false, e).await;
            return Ok(());
        }
        let vni_num = self.next_vni().await?;
        let vni_id = self
            .insert_vni(vni_num, &msg.description, owner_account_id)
            .await?;
        let vni = Vni {
            id: vni_id,
            vni: vni_num,
            description: msg.description,
            account_id: owner_account_id,
            vteps: Default::default(),
        };
        self.vnis.insert(vni.vni, vni.clone());
        let owner_to_sync = if let Some(owner) = self.find_account_mut_by_id(owner_account_id) {
            owner.vnis.insert(vni.vni);
            Some(owner.clone())
        } else {
            None
        };
        if let Some(owner) = owner_to_sync {
            self.sync_account(&owner).await?;
        }
        self.notify_rrs(RrLchMsg::VniAdded(vni.clone())).await;
        println!(
            "Created VNI {} for account {} by account {}",
            vni.vni, owner_account_id, requester.id
        );
        Self::send_op(msg.rep_tx, true, "VNI created").await;
        Ok(())
    }

    async fn update_vni_vteps(&mut self, msg: UpdateVniVtepsReq) -> Result<(), String> {
        let Some(vni) = self.vnis.get(&msg.vni).cloned() else {
            Self::send_op(msg.rep_tx, false, "VNI not found").await;
            return Ok(());
        };
        if !self.can_manage_account(msg.requester_id, vni.account_id) {
            Self::send_op(msg.rep_tx, false, "Permission denied").await;
            return Ok(());
        }
        let Some(requester) = self.find_account_by_id(msg.requester_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Account not found").await;
            return Ok(());
        };
        for vtep_name in &msg.vteps {
            let Some(vtep) = self.vteps.get(vtep_name) else {
                Self::send_op(msg.rep_tx, false, format!("VTEP {vtep_name} not found")).await;
                return Ok(());
            };
            if requester.perm != Permission::Admin && vtep.account_id != requester.id {
                Self::send_op(msg.rep_tx, false, "Cannot attach another account's VTEP").await;
                return Ok(());
            }
        }

        let mut requested_vteps = msg.vteps.clone();
        if requester.perm != Permission::Admin {
            for vtep_name in &vni.vteps {
                if let Some(vtep) = self.vteps.get(vtep_name)
                    && vtep.account_id != requester.id
                {
                    requested_vteps.insert(vtep_name.clone());
                }
            }
        }

        let old_vteps = vni.vteps.clone();
        let old_description = vni.description.clone();
        let removed = old_vteps
            .difference(&requested_vteps)
            .cloned()
            .collect::<Vec<_>>();
        let added = requested_vteps
            .difference(&old_vteps)
            .cloned()
            .collect::<Vec<_>>();
        let description_changed = old_description != msg.description;
        if removed.is_empty() && added.is_empty() && !description_changed {
            Self::send_op(msg.rep_tx, true, "No changes").await;
            return Ok(());
        }

        if let Some(vni_mut) = self.vnis.get_mut(&msg.vni) {
            vni_mut.vteps = requested_vteps;
            vni_mut.description = msg.description.clone();
        }
        for vtep_name in &removed {
            if let Some(vtep_mut) = self.vteps.get_mut(vtep_name) {
                vtep_mut.vnis.remove(&msg.vni);
            }
        }
        for vtep_name in &added {
            if let Some(vtep_mut) = self.vteps.get_mut(vtep_name) {
                vtep_mut.vnis.insert(msg.vni);
            }
        }

        let new_vni = self.vnis.get(&msg.vni).cloned().unwrap();
        self.sync_vni(&new_vni).await?;
        let mut changed = removed.clone();
        changed.extend(added.clone());
        changed.sort();
        changed.dedup();
        for vtep_name in &changed {
            if let Some(vtep) = self.vteps.get(vtep_name).cloned() {
                self.sync_vtep(&vtep).await?;
                self.notify_rrs(RrLchMsg::VtepVniModified {
                    vtep,
                    vni: new_vni.clone(),
                })
                .await;
            }
        }
        println!(
            "Updated VNI {} by account {}: description_changed={} add_vteps={:?} del_vteps={:?}",
            msg.vni, requester.id, description_changed, added, removed
        );
        Self::send_op(msg.rep_tx, true, "VNI updated").await;
        Ok(())
    }

    async fn delete_vni(&mut self, msg: DeleteVniReq) -> Result<(), String> {
        let Some(existing) = self.vnis.get(&msg.vni).cloned() else {
            Self::send_op(msg.rep_tx, false, "VNI not found").await;
            return Ok(());
        };
        if !self.can_manage_account(msg.requester_id, existing.account_id) {
            Self::send_op(msg.rep_tx, false, "Permission denied").await;
            return Ok(());
        }
        let mut deleted_vni = existing.clone();
        let affected_vteps = existing.vteps.iter().cloned().collect::<Vec<_>>();
        if let Some(vni_mut) = self.vnis.get_mut(&msg.vni) {
            vni_mut.vteps.clear();
        }
        deleted_vni.vteps.clear();
        self.sync_vni(&deleted_vni).await?;
        for vtep_name in &affected_vteps {
            if let Some(vtep_mut) = self.vteps.get_mut(vtep_name) {
                vtep_mut.vnis.remove(&msg.vni);
            }
        }
        for vtep_name in &affected_vteps {
            if let Some(vtep) = self.vteps.get(vtep_name).cloned() {
                self.sync_vtep(&vtep).await?;
                self.notify_rrs(RrLchMsg::VtepVniModified {
                    vtep,
                    vni: deleted_vni.clone(),
                })
                .await;
            }
        }
        let owner_to_sync = if let Some(owner) = self.find_account_mut_by_id(existing.account_id) {
            owner.vnis.remove(&msg.vni);
            Some(owner.clone())
        } else {
            None
        };
        if let Some(owner) = owner_to_sync {
            self.sync_account(&owner).await?;
        }
        self.delete_vni_row(existing.id).await?;
        self.vnis.remove(&msg.vni);
        self.notify_rrs(RrLchMsg::VniDeleted(deleted_vni)).await;
        println!(
            "Deleted VNI {} from account {} by account {}",
            msg.vni, existing.account_id, msg.requester_id
        );
        Self::send_op(msg.rep_tx, true, "VNI deleted").await;
        Ok(())
    }

    async fn list_accounts(&mut self, msg: ListAccountsReq) -> Result<(), String> {
        let requester = self.find_account_by_id(msg.requester_id).cloned();
        if requester.as_ref().map(|account| account.perm) != Some(Permission::Admin) {
            let _ = msg
                .rep_tx
                .send(ListAccountsRep {
                    requester,
                    accounts: Vec::new(),
                })
                .await;
            return Ok(());
        }
        let mut accounts = self.accounts.values().cloned().collect::<Vec<_>>();
        accounts.sort_by(|a, b| a.id.cmp(&b.id));
        let _ = msg
            .rep_tx
            .send(ListAccountsRep {
                requester,
                accounts,
            })
            .await;
        Ok(())
    }

    async fn create_account(&mut self, msg: CreateAccountReq) -> Result<(), String> {
        let Some(requester) = self.find_account_by_id(msg.requester_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Account not found").await;
            return Ok(());
        };
        if requester.perm != Permission::Admin {
            Self::send_op(msg.rep_tx, false, "Permission denied").await;
            return Ok(());
        }
        if msg.mail_addr.trim().is_empty() || msg.password.is_empty() {
            Self::send_op(msg.rep_tx, false, "Mail address and password are required").await;
            return Ok(());
        }
        if self.accounts.contains_key(&msg.mail_addr) {
            Self::send_op(msg.rep_tx, false, "Account already exists").await;
            return Ok(());
        }
        let perm = match self.parse_permission(msg.perm_code) {
            Ok(perm) => perm,
            Err(e) => {
                Self::send_op(msg.rep_tx, false, e).await;
                return Ok(());
            }
        };
        let password = self.hash_password(&msg.password)?;
        let account_id = self.insert_account(&msg.mail_addr, &password, perm).await?;
        self.accounts.insert(
            msg.mail_addr.clone(),
            Account {
                id: account_id,
                mail_addr: msg.mail_addr.clone(),
                password,
                perm,
                vnis: Default::default(),
                vteps: Default::default(),
            },
        );
        println!(
            "Created account {} with permission {} by account {}",
            msg.mail_addr,
            self.perm_label(perm),
            requester.id
        );
        Self::send_op(msg.rep_tx, true, "Account created").await;
        Ok(())
    }

    async fn delete_account(&mut self, msg: DeleteAccountReq) -> Result<(), String> {
        let Some(requester) = self.find_account_by_id(msg.requester_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Account not found").await;
            return Ok(());
        };
        if requester.perm != Permission::Admin {
            Self::send_op(msg.rep_tx, false, "Permission denied").await;
            return Ok(());
        }
        let Some(account) = self.find_account_by_id(msg.account_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Account not found").await;
            return Ok(());
        };
        if !account.vnis.is_empty() || !account.vteps.is_empty() {
            Self::send_op(
                msg.rep_tx,
                false,
                "Delete VTEPs and VNIs before deleting the account",
            )
            .await;
            return Ok(());
        }
        self.delete_account_row(account.id).await?;
        self.accounts.remove(&account.mail_addr);
        println!(
            "Deleted account {} ({}) by account {}",
            account.id, account.mail_addr, requester.id
        );
        Self::send_op(msg.rep_tx, true, "Account deleted").await;
        Ok(())
    }

    async fn update_account_perm(&mut self, msg: UpdateAccountPermReq) -> Result<(), String> {
        let Some(requester) = self.find_account_by_id(msg.requester_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Account not found").await;
            return Ok(());
        };
        if requester.perm != Permission::Admin {
            Self::send_op(msg.rep_tx, false, "Permission denied").await;
            return Ok(());
        }
        let Some(account) = self.find_account_by_id(msg.account_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Account not found").await;
            return Ok(());
        };
        let new_perm = match self.parse_permission(msg.perm_code) {
            Ok(perm) => perm,
            Err(e) => {
                Self::send_op(msg.rep_tx, false, e).await;
                return Ok(());
            }
        };
        if account.perm == Permission::Admin || new_perm == Permission::Admin {
            Self::send_op(msg.rep_tx, false, "Admin permission cannot be changed").await;
            return Ok(());
        }
        if account.perm == new_perm {
            Self::send_op(msg.rep_tx, true, "No changes").await;
            return Ok(());
        }
        if !matches!(account.perm, Permission::Free | Permission::Pro)
            || !matches!(new_perm, Permission::Free | Permission::Pro)
        {
            Self::send_op(
                msg.rep_tx,
                false,
                "Only free/pro permission changes are allowed",
            )
            .await;
            return Ok(());
        }
        let mut account_to_sync = account.clone();
        account_to_sync.perm = new_perm;
        if let Err(e) = self.check_resource_limit(
            &account_to_sync,
            account_to_sync.vnis.len(),
            account_to_sync.vteps.len(),
        ) {
            Self::send_op(msg.rep_tx, false, e).await;
            return Ok(());
        }
        self.sync_account(&account_to_sync).await?;
        if let Some(account_mut) = self.find_account_mut_by_id(msg.account_id) {
            account_mut.perm = new_perm;
        }
        println!(
            "Updated account {} permission: {} -> {} by account {}",
            account.id,
            self.perm_label(account.perm),
            self.perm_label(new_perm),
            requester.id
        );
        Self::send_op(msg.rep_tx, true, "Permission updated").await;
        Ok(())
    }

    async fn change_password(&mut self, msg: ChangePasswordReq) -> Result<(), String> {
        let Some(account) = self.find_account_by_id(msg.account_id).cloned() else {
            Self::send_op(msg.rep_tx, false, "Account not found").await;
            return Ok(());
        };
        if msg.new_password.is_empty() {
            Self::send_op(msg.rep_tx, false, "New password is required").await;
            return Ok(());
        }
        let Ok(parsed_hash) = PasswordHash::new(&account.password) else {
            Self::send_op(msg.rep_tx, false, "Stored password hash is invalid").await;
            return Ok(());
        };
        if Argon2::default()
            .verify_password(msg.current_password.as_bytes(), &parsed_hash)
            .is_err()
        {
            Self::send_op(msg.rep_tx, false, "Current password is incorrect").await;
            return Ok(());
        }
        let new_password = self.hash_password(&msg.new_password)?;
        let account_to_sync = if let Some(account_mut) = self.find_account_mut_by_id(msg.account_id)
        {
            account_mut.password = new_password;
            Some(account_mut.clone())
        } else {
            None
        };
        if let Some(account) = account_to_sync {
            self.sync_account(&account).await?;
        }
        println!("Password updated for account {}", msg.account_id);
        Self::send_op(msg.rep_tx, true, "Password updated").await;
        Ok(())
    }

    async fn rr_registered(&mut self, msg: RrRegisteredMsg) -> Result<(), String> {
        self.vtep_states.insert(
            msg.name,
            HashMap::<String, (Option<Ipv6Addr>, String)>::new(),
        );
        Ok(())
    }

    async fn rr_exit(&mut self, msg: RrExitMsg) -> Result<(), String> {
        self.vtep_states.remove(&msg.name);
        Ok(())
    }

    async fn update_vtep_state(&mut self, msg: UpdateVtepStateMsg) -> Result<(), String> {
        if let Some(vtep_map) = self.vtep_states.get_mut(&msg.rr_name) {
            vtep_map.insert(msg.vtep_name, (msg.ipv6_addr, msg.last_update));
        }
        Ok(())
    }

    async fn lch(&mut self, msg: Option<UiLchMsg>) -> Result<(), String> {
        match msg {
            Some(UiLchMsg::GetAccountById(msg)) => self.get_account_by_id(msg).await,
            Some(UiLchMsg::AuthAccount(msg)) => self.auth_account(msg).await,
            Some(UiLchMsg::AuthRr(msg)) => self.auth_rr(msg).await,
            Some(UiLchMsg::ListVteps(msg)) => self.list_vteps(msg).await,
            Some(UiLchMsg::CreateVtep(msg)) => self.create_vtep(msg).await,
            Some(UiLchMsg::UpdateVtepVnis(msg)) => self.update_vtep_vnis(msg).await,
            Some(UiLchMsg::DeleteVtep(msg)) => self.delete_vtep(msg).await,
            Some(UiLchMsg::ListVnis(msg)) => self.list_vnis(msg).await,
            Some(UiLchMsg::CreateVni(msg)) => self.create_vni(msg).await,
            Some(UiLchMsg::UpdateVniVteps(msg)) => self.update_vni_vteps(msg).await,
            Some(UiLchMsg::DeleteVni(msg)) => self.delete_vni(msg).await,
            Some(UiLchMsg::ListAccounts(msg)) => self.list_accounts(msg).await,
            Some(UiLchMsg::CreateAccount(msg)) => self.create_account(msg).await,
            Some(UiLchMsg::DeleteAccount(msg)) => self.delete_account(msg).await,
            Some(UiLchMsg::UpdateAccountPerm(msg)) => self.update_account_perm(msg).await,
            Some(UiLchMsg::ChangePassword(msg)) => self.change_password(msg).await,
            Some(UiLchMsg::RrRegistered(msg)) => self.rr_registered(msg).await,
            Some(UiLchMsg::RrExit(msg)) => self.rr_exit(msg).await,
            Some(UiLchMsg::UpdateVtepState(msg)) => self.update_vtep_state(msg).await,
            None => Err("Received none lch".to_string()),
        }
    }

    pub async fn run(&mut self) {
        if let Err(e) = self.restore().await {
            println!("Restore from database error: {e}");
            return;
        }

        let http_addr = self.http_addr;
        let http_port = self.http_port;
        let tx_lch = self.tx_lch.clone();
        let mut ui_handler = UiHandler::new(http_addr, http_port, tx_lch);
        tokio::spawn(async move {
            ui_handler.run().await;
        });

        let mut rch_listener = match TlsRchListener::new(self.rch_addr, self.rch_port).await {
            Ok(rch_listener) => rch_listener,
            Err(e) => {
                println!("Rch listener new error: {e}");
                return;
            }
        };

        loop {
            tokio::select! {
                ret = rch_listener.rch_accept::<UiRrRchMsg, UiRchMsg>() => {
                    match ret {
                        Ok((tx_rch, rx_rch, IpAddr::V4(addr))) => {
                            let (tx_lch, rx_lch) = mpsc::channel(8);
                            self.rr_tx_lchs.insert(addr, tx_lch);
                            let mut rr_handler =
                                RrHandler::new(addr, tx_rch, rx_rch, self.tx_lch.clone(), rx_lch);
                            tokio::spawn(async move {
                                rr_handler.run().await;
                            });
                        }
                        Ok((_, _, IpAddr::V6(_))) => {
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
