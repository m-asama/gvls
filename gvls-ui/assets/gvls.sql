--
--
--

DROP TABLE IF EXISTS accounts;
CREATE TABLE accounts (
	id		SERIAL NOT NULL,
	mail_addr	TEXT NOT NULL UNIQUE,
	password	TEXT NOT NULL,
	perm		INT NOT NULL,		-- 1: admin, 2: free, 3: pro
	vni_ids		INT[] NOT NULL,
	vtep_ids	INT[] NOT NULL,
	PRIMARY KEY (id)
);

DROP TABLE IF EXISTS rrs;
CREATE TABLE rrs (
	id		SERIAL NOT NULL,
	name		TEXT NOT NULL UNIQUE,
	password	TEXT NOT NULL,
	PRIMARY KEY (id)
);

DROP TABLE IF EXISTS vteps;
CREATE TABLE vteps (
	id		SERIAL NOT NULL,
	name		TEXT NOT NULL UNIQUE,
	description	TEXT NOT NULL DEFAULT '',
	password	TEXT NOT NULL,
	bgp_pass	TEXT NOT NULL,
	account_id	INT NOT NULL,
	vni_ids	INT[] NOT NULL,
	PRIMARY KEY (id)
);

DROP TABLE IF EXISTS vnis;
CREATE TABLE vnis (
	id		SERIAL NOT NULL,
	vni		INT NOT NULL UNIQUE,
	description	TEXT NOT NULL DEFAULT '',
	account_id	INT NOT NULL,
	vtep_ids	INT[] NOT NULL,
	PRIMARY KEY (id)
);

INSERT INTO accounts ( mail_addr, password, perm, vni_ids, vtep_ids )
	VALUES ( 'admin@gvls.ginzado.ne.jp', '$argon2id$v=19$m=19456,t=2,p=1$VDBIWVJ0b2xkNU41bkZaVFg3bDdIYXJOUFNjU01GTDU$FVTR3jqclDhfNgSkgmg3jHfEqxKmOw5uTrjWFKM4LRA', 1, '{}', '{}' );

INSERT INTO rrs ( name, password )
	VALUES ( 'gvls-rr1', '$argon2id$v=19$m=19456,t=2,p=1$Q3dCeTlKVjdqanE1R09wUDM1eURWMmxpc0FKaFVQMTg$MVPUTmomcCmBh0bilti7sQ7VyVIUh/UVGOXgw60Xv+M' );

INSERT INTO rrs ( name, password )
	VALUES ( 'gvls-rr2', '$argon2id$v=19$m=19456,t=2,p=1$YWh0b2FleThRdmducnhnT1VEYWI5RkZvMlVPZHpkM08$AAI+OexLdO1AAOv7aK/OIkCQMsk77WWJrIbRLyBeRsI' );
