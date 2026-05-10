// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::{HashMap, HashSet};

use tokio_postgres::{Client, NoTls};

use libgvls::{Account, Permission, Rr, Vni, Vtep};

use crate::Context;

async fn get_vtep_id2name(client: &Client) -> Result<HashMap<i32, String>, String> {
    let mut vtep_id2name = HashMap::<i32, String>::new();
    let q = r#"
        SELECT id, name
        FROM vteps;
    "#;
    let rows = client
        .query(q, &[])
        .await
        .map_err(|e| format!("Select query error: {e}"))?;
    for row in &rows {
        let id: i32 = row.get(0);
        let name: String = row.get(1);
        let _ = vtep_id2name.insert(id, name);
    }
    Ok(vtep_id2name)
}

async fn get_vni_id2vni(client: &Client) -> Result<HashMap<i32, i32>, String> {
    let mut vni_id2vni = HashMap::<i32, i32>::new();
    let q = r#"
        SELECT id, vni
        FROM vnis;
    "#;
    let rows = client
        .query(q, &[])
        .await
        .map_err(|e| format!("Select query error: {e}"))?;
    for row in &rows {
        let id: i32 = row.get(0);
        let vni: i32 = row.get(1);
        let _ = vni_id2vni.insert(id, vni);
    }
    Ok(vni_id2vni)
}

impl Context {
    fn conn_str(&self) -> String {
        format!(
            "user={} password={} dbname={} hostaddr={} port={}",
            self.db_username, self.db_password, self.db_name, self.db_host, self.db_port
        )
    }

    async fn db_client(&self) -> Result<Client, String> {
        let conn_str = self.conn_str();
        let (client, conn) = tokio_postgres::connect(&conn_str, NoTls)
            .await
            .map_err(|e| format!("Database connect error: {e}"))?;
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("connection error: {e}");
            }
        });
        Ok(client)
    }

    fn vni_numbers_to_ids(&self, vnis: &HashSet<i32>) -> Result<Vec<i32>, String> {
        let mut ids = Vec::new();
        for vni_num in vnis {
            let vni = self
                .vnis
                .get(vni_num)
                .ok_or_else(|| format!("VNI {vni_num} not found"))?;
            ids.push(vni.id);
        }
        ids.sort_unstable();
        Ok(ids)
    }

    fn vtep_names_to_ids(&self, vteps: &HashSet<String>) -> Result<Vec<i32>, String> {
        let mut ids = Vec::new();
        for vtep_name in vteps {
            let vtep = self
                .vteps
                .get(vtep_name)
                .ok_or_else(|| format!("VTEP {vtep_name} not found"))?;
            ids.push(vtep.id);
        }
        ids.sort_unstable();
        Ok(ids)
    }

    pub async fn restore(&mut self) -> Result<(), String> {
        let client = self.db_client().await?;
        let vtep_id2name = get_vtep_id2name(&client).await?;
        let vni_id2vni = get_vni_id2vni(&client).await?;
        self.restore_accounts(&client, &vtep_id2name, &vni_id2vni)
            .await?;
        self.restore_rrs(&client).await?;
        self.restore_vteps(&client, &vni_id2vni).await?;
        self.restore_vnis(&client, &vtep_id2name).await?;
        Ok(())
    }

    pub async fn restore_accounts(
        &mut self,
        client: &Client,
        vtep_id2name: &HashMap<i32, String>,
        vni_id2vni: &HashMap<i32, i32>,
    ) -> Result<(), String> {
        let q = r#"
            SELECT id, mail_addr, password, perm, vni_ids, vtep_ids
            FROM accounts;
        "#;
        let rows = client
            .query(q, &[])
            .await
            .map_err(|e| format!("Select query error: {e}"))?;
        for row in &rows {
            let id: i32 = row.get(0);
            let mail_addr: String = row.get(1);
            let password: String = row.get(2);
            let perm = self.parse_permission(row.get(3))?;
            let vni_ids: Vec<i32> = row.get(4);
            let vtep_ids: Vec<i32> = row.get(5);
            let vnis = {
                let mut tmp = HashSet::<i32>::new();
                for vni_id in vni_ids {
                    if let Some(vni_num) = vni_id2vni.get(&vni_id) {
                        tmp.insert(*vni_num);
                    }
                }
                tmp
            };
            let vteps = {
                let mut tmp = HashSet::<String>::new();
                for vtep_id in vtep_ids {
                    if let Some(vtep_name) = vtep_id2name.get(&vtep_id) {
                        tmp.insert(vtep_name.to_string());
                    }
                }
                tmp
            };
            let _ = self.accounts.insert(
                mail_addr.clone(),
                Account {
                    id,
                    mail_addr,
                    password,
                    perm,
                    vnis,
                    vteps,
                },
            );
        }
        Ok(())
    }

    pub async fn restore_rrs(&mut self, client: &Client) -> Result<(), String> {
        let q = r#"
            SELECT id, name, password
            FROM rrs;
        "#;
        let rows = client
            .query(q, &[])
            .await
            .map_err(|e| format!("Select query error: {e}"))?;
        for row in &rows {
            let id: i32 = row.get(0);
            let name: String = row.get(1);
            let password: String = row.get(2);
            let _ = self.rrs.insert(
                name.clone(),
                Rr {
                    id,
                    name,
                    password,
                    ipv4_addr: None,
                    last_update: None,
                },
            );
        }
        Ok(())
    }

    pub async fn restore_vteps(
        &mut self,
        client: &Client,
        vni_id2vni: &HashMap<i32, i32>,
    ) -> Result<(), String> {
        let q = r#"
            SELECT id, name, description, password, bgp_pass, account_id, vni_ids
            FROM vteps;
        "#;
        let rows = client
            .query(q, &[])
            .await
            .map_err(|e| format!("Select query error: {e}"))?;
        for row in &rows {
            let id: i32 = row.get(0);
            let name: String = row.get(1);
            let description: String = row.get(2);
            let password: String = row.get(3);
            let bgp_pass: String = row.get(4);
            let account_id: i32 = row.get(5);
            let vni_ids: Vec<i32> = row.get(6);
            let vnis = {
                let mut tmp = HashSet::<i32>::new();
                for vni_id in vni_ids {
                    if let Some(vni_num) = vni_id2vni.get(&vni_id) {
                        tmp.insert(*vni_num);
                    }
                }
                tmp
            };
            let _ = self.vteps.insert(
                name.clone(),
                Vtep {
                    id,
                    name,
                    description,
                    password,
                    bgp_pass,
                    account_id,
                    vnis,
                    ipv6_addr: None,
                    last_update: None,
                },
            );
        }
        Ok(())
    }

    pub async fn restore_vnis(
        &mut self,
        client: &Client,
        vtep_id2name: &HashMap<i32, String>,
    ) -> Result<(), String> {
        let q = r#"
            SELECT id, vni, description, account_id, vtep_ids
            FROM vnis;
        "#;
        let rows = client
            .query(q, &[])
            .await
            .map_err(|e| format!("Select query error: {e}"))?;
        for row in &rows {
            let id: i32 = row.get(0);
            let vni: i32 = row.get(1);
            let description: String = row.get(2);
            let account_id: i32 = row.get(3);
            let vtep_ids: Vec<i32> = row.get(4);
            let vteps = {
                let mut tmp = HashSet::<String>::new();
                for vtep_id in vtep_ids {
                    if let Some(vtep_name) = vtep_id2name.get(&vtep_id) {
                        tmp.insert(vtep_name.to_string());
                    }
                }
                tmp
            };
            let _ = self.vnis.insert(
                vni,
                Vni {
                    id,
                    vni,
                    description,
                    account_id,
                    vteps,
                },
            );
        }
        Ok(())
    }

    pub async fn sync_account(&self, account: &Account) -> Result<(), String> {
        let client = self.db_client().await?;
        let vni_ids = self.vni_numbers_to_ids(&account.vnis)?;
        let vtep_ids = self.vtep_names_to_ids(&account.vteps)?;
        let q = r#"
            UPDATE accounts
            SET mail_addr = $1, password = $2, perm = $3, vni_ids = $4, vtep_ids = $5
            WHERE id = $6;
        "#;
        client
            .execute(
                q,
                &[
                    &account.mail_addr,
                    &account.password,
                    &Self::permission_code(account.perm),
                    &vni_ids,
                    &vtep_ids,
                    &account.id,
                ],
            )
            .await
            .map_err(|e| format!("Update account error: {e}"))?;
        Ok(())
    }

    pub async fn sync_vtep(&self, vtep: &Vtep) -> Result<(), String> {
        let client = self.db_client().await?;
        let vni_ids = self.vni_numbers_to_ids(&vtep.vnis)?;
        let q = r#"
            UPDATE vteps
            SET name = $1, description = $2, password = $3, bgp_pass = $4, account_id = $5, vni_ids = $6
            WHERE id = $7;
        "#;
        client
            .execute(
                q,
                &[
                    &vtep.name,
                    &vtep.description,
                    &vtep.password,
                    &vtep.bgp_pass,
                    &vtep.account_id,
                    &vni_ids,
                    &vtep.id,
                ],
            )
            .await
            .map_err(|e| format!("Update vtep error: {e}"))?;
        Ok(())
    }

    pub async fn sync_vni(&self, vni: &Vni) -> Result<(), String> {
        let client = self.db_client().await?;
        let vtep_ids = self.vtep_names_to_ids(&vni.vteps)?;
        let q = r#"
            UPDATE vnis
            SET vni = $1, description = $2, account_id = $3, vtep_ids = $4
            WHERE id = $5;
        "#;
        client
            .execute(
                q,
                &[
                    &vni.vni,
                    &vni.description,
                    &vni.account_id,
                    &vtep_ids,
                    &vni.id,
                ],
            )
            .await
            .map_err(|e| format!("Update vni error: {e}"))?;
        Ok(())
    }

    pub async fn insert_account(
        &self,
        mail_addr: &str,
        password: &str,
        perm: Permission,
    ) -> Result<i32, String> {
        let client = self.db_client().await?;
        let q = r#"
            INSERT INTO accounts (mail_addr, password, perm, vni_ids, vtep_ids)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id;
        "#;
        let row = client
            .query_one(
                q,
                &[
                    &mail_addr,
                    &password,
                    &Self::permission_code(perm),
                    &Vec::<i32>::new(),
                    &Vec::<i32>::new(),
                ],
            )
            .await
            .map_err(|e| format!("Insert account error: {e}"))?;
        Ok(row.get(0))
    }

    pub async fn delete_account_row(&self, account_id: i32) -> Result<(), String> {
        let client = self.db_client().await?;
        client
            .execute("DELETE FROM accounts WHERE id = $1;", &[&account_id])
            .await
            .map_err(|e| format!("Delete account error: {e}"))?;
        Ok(())
    }

    pub async fn insert_vtep(
        &self,
        id: i32,
        name: &str,
        description: &str,
        password: &str,
        bgp_pass: &str,
        account_id: i32,
    ) -> Result<i32, String> {
        let client = self.db_client().await?;
        let q = r#"
            INSERT INTO vteps (id, name, description, password, bgp_pass, account_id, vni_ids)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id;
        "#;
        let row = client
            .query_one(
                q,
                &[
                    &id,
                    &name,
                    &description,
                    &password,
                    &bgp_pass,
                    &account_id,
                    &Vec::<i32>::new(),
                ],
            )
            .await
            .map_err(|e| format!("Insert vtep error: {e}"))?;
        Ok(row.get(0))
    }

    pub async fn reserve_vtep_id(&self) -> Result<i32, String> {
        let client = self.db_client().await?;
        let row = client
            .query_one(
                "SELECT nextval(pg_get_serial_sequence('vteps', 'id')::REGCLASS)::INT;",
                &[],
            )
            .await
            .map_err(|e| format!("Reserve vtep id error: {e}"))?;
        Ok(row.get(0))
    }

    pub async fn delete_vtep_row(&self, vtep_id: i32) -> Result<(), String> {
        let client = self.db_client().await?;
        client
            .execute("DELETE FROM vteps WHERE id = $1;", &[&vtep_id])
            .await
            .map_err(|e| format!("Delete vtep error: {e}"))?;
        Ok(())
    }

    pub async fn insert_vni(
        &self,
        id: i32,
        description: &str,
        account_id: i32,
    ) -> Result<i32, String> {
        let client = self.db_client().await?;
        let q = r#"
            INSERT INTO vnis (id, vni, description, account_id, vtep_ids)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id;
        "#;
        let row = client
            .query_one(
                q,
                &[&id, &id, &description, &account_id, &Vec::<i32>::new()],
            )
            .await
            .map_err(|e| format!("Insert vni error: {e}"))?;
        Ok(row.get(0))
    }

    pub async fn reserve_vni_id(&self) -> Result<i32, String> {
        let client = self.db_client().await?;
        let row = client
            .query_one(
                "SELECT nextval(pg_get_serial_sequence('vnis', 'id')::REGCLASS)::INT;",
                &[],
            )
            .await
            .map_err(|e| format!("Reserve vni id error: {e}"))?;
        Ok(row.get(0))
    }

    pub async fn delete_vni_row(&self, vni_id: i32) -> Result<(), String> {
        let client = self.db_client().await?;
        client
            .execute("DELETE FROM vnis WHERE id = $1;", &[&vni_id])
            .await
            .map_err(|e| format!("Delete vni error: {e}"))?;
        Ok(())
    }
}
