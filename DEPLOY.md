# 銀座堂仮想 LAN サービスのクローンを構成する

## 概要

> [!NOTE]
> この情報は銀座堂仮想 LAN サービス ( https://gvls.ginzado.ne.jp/ ) のクローンを作成するためのものです。
> 単純に銀座堂仮想 LAN サービスを利用したいだけであればこの作業は不要です。
> その場合は [README.md](README.md) を参照してください。

> [!NOTE]
> 現時点で Ubuntu 24.04 LTS のみサポートしています。

銀座堂仮想 LAN サービス ( https://gvls.ginzado.ne.jp/ ) のクローンを作成する手順は以下の流れになります。

1. 「[OPEN IPv6 ダイナミック DNS for フレッツ・光ネクスト](https://i.open.ad.jp/)」の利用登録
1. `gvls-ui` の構築
2. `gvls-rr` の構築

### gvls-ui の要件

* 利用者が Web UI へアクセスするためインターネットに接続されている(インターネットから接続できる)必要があります。
* `gvls-ui` は `gvls-rr` からの IPv4 接続を受け付けれる必要があり、その際のアドレスは固定である必要があります。

### gvls-rr の要件

* `gvls-rr` は `gvls-vtep` からの接続を受け付けれる必要があるためフレッツ閉域 IPv6 網に接続されている必要があります。
* `gvls-rr` は `gvls-ui` へ接続する必要があるため IPv4 インターネットに接続されている必要があり、そのアドレスは固定であることが望ましいです。
* IPv6 プレフィクスが変更となった際の通信への影響を考慮し `gvls-rr` は可能な限り地理的に離れた場所に 2 台構築します。

## 「OPEN IPv6 ダイナミック DNS for フレッツ・光ネクスト」の利用登録

「[OPEN IPv6 ダイナミック DNS for フレッツ・光ネクスト](https://i.open.ad.jp/)」で `gvls-rr` のための「DDNS ホストの新規作成」を行います。

`gvls-rr` は 2 台構築するため 2 つ登録する必要があります。

ホスト名と作成時に発行されるホストキーの情報は以降 `gvls-rr` の設定の際に必要となるので控えておきます。

## gvls-ui の構築

### PostgreSQL データベースをインストールする

`gvls-ui` は利用者のアカウント情報や RR/VTEP/VNI の情報を PostgreSQL データベースに格納します。

以下のコマンドを実行し PostgreSQL データベースをインストールします。

```shell
$ sudo apt install postgresql
$ sudo systemctl enable postgresql.service
$ sudo systemctl start postgresql.service
```

### Apache httpd をインストールする

`gvls-ui` は Web UI を提供しますがフロントエンドに Apache httpd を設置し Proxy 経由で Web UI 機能を提供するようにします。

以下のコマンドを実行し Apache httpd をインストールします。

```shell
$ sudo apt install apache2
$ sudo systemctl enable apache2.service
$ sudo systemctl start apache2.service
```

以下のコマンドを実行し Proxy 機能を有効化します。

```shell
$ sudo a2enmod proxy
$ sudo a2enmod proxy_http
```

以下のような内容の `/etc/apache2/conf-available/gvls.conf` というファイルを作成します。

```
<VirtualHost *:80>
ProxyPass        / http://127.0.0.1:3000/
ProxyPassReverse / http://127.0.0.1:3000/
</VirtualHost>
```

以下のコマンドを実行し上記の設定を有効化します。

```shell
$ sudo a2enconf gvls
```

Apache httpd を再起動します。

```shell
$ sudo systemctl restart apache2.service
```

### gvls-ui をインストールする

`gvls-ui` の deb パッケージをインストールします。

### データベース gvls の準備

`/usr/share/doc/gvls-ui/gvls.sql` を適当な場所にコピーし以下の箇所の管理者アカウントのメールアドレスと RR の名前とパスワード部分を書き換えます。

```sql
INSERT INTO accounts ( mail_addr, password, perm, vni_ids, vtep_ids )
        VALUES ( 'admin@gvls.ginzado.ne.jp', '$argon2id$v=19$m=19456,t=2,p=1$VDBIWVJ0b2xkNU41bkZaVFg3bDdIYXJOUFNjU01GTDU$FVTR3jqclDhfNgSkgmg3jHfEqxKmOw5uTrjWFKM4LRA', 1, '{}', '{}' );

INSERT INTO rrs ( name, password )
        VALUES ( 'gvls-rr1', '$argon2id$v=19$m=19456,t=2,p=1$Q3dCeTlKVjdqanE1R09wUDM1eURWMmxpc0FKaFVQMTg$MVPUTmomcCmBh0bilti7sQ7VyVIUh/UVGOXgw60Xv+M' );

INSERT INTO rrs ( name, password )
        VALUES ( 'gvls-rr2', '$argon2id$v=19$m=19456,t=2,p=1$YWh0b2FleThRdmducnhnT1VEYWI5RkZvMlVPZHpkM08$AAI+OexLdO1AAOv7aK/OIkCQMsk77WWJrIbRLyBeRsI' );
```

パスワード部分は Argon2 のハッシュである必要があります。
適当に Argon2 ハッシュ生成サイトで生成するかツールを用意して生成します。

管理アカウントと RR の情報を書き換えたら PostgreSQL でデータベースを作成し SQL 文を実行します。

### gvls-ui の設定

`/etc/default/gvls-ui` を以下のように修正します。

```shell
GVLS_UI_HTTP_ADDR="127.0.0.1"
GVLS_UI_HTTP_PORT="3000"
GVLS_UI_RCH_ADDR="0.0.0.0"
GVLS_UI_RCH_PORT="7101"
GVLS_UI_DB_USERNAME="gvls"
GVLS_UI_DB_PASSWORD="gvls"
GVLS_UI_DB_HOST="127.0.0.1"
GVLS_UI_DB_PORT="5432"
GVLS_UI_DB_NAME="gvls"
```

データベースへの接続情報は適宜修正してください。

### gvls-ui の起動

`gvls-ui` を起動します。

```shell
$ sudo systemctl enable gvls-ui.service
$ sudo systemctl start gvls-ui.service
```

ウェブブラウザからアクセスし PostgreSQL データベースに登録した管理者アカウントのメールアドレスとパスワードでログインできることを確認します。

## gvls-rr の構築

### zebra-rs をインストールする

[zebra-rs](https://zebra.rs/) をインストールします。
deb パッケージは [こちら](https://github.com/zebra-rs/zebra-rs/releases) で配布されています。

```shell
$ sudo systemctl enable zebra-rs.service
$ sudo systemctl start zebra-rs.service
```

> [!NOTE]
> [ここ](https://www.ginzado.ne.jp/~m-asama/evpnvxlan6/) にある frr と frr-pythontools をインストールし、このあと説明する `/etc/default/gvls-rr` で `GVLS_RR_OPTS="--bgp-backend frr"` を設定することで BGP 実装を zebra-rs から frr に切り替えることもできます。

### gvls-rr をインストールする

`gvls-rr` の deb パッケージをインストールします。

### gvls-rr の設定

`/etc/default/gvls-rr` を以下のように修正します。

```shell
GVLS_RR_NAME="gvls-rr1"
GVLS_RR_PASS="gvls-rr1"
GVLS_RR_SRC_IFNAME="enp8s0"
GVLS_RR_UI_ADDR="169.254.1.1"
GVLS_RR_UI_PORT="7101"
GVLS_RR_RCH_ADDR="::"
GVLS_RR_RCH_PORT="7102"
GVLS_RR_DDNS_HOSTKEY=""
GVLS_RR_OPTS=""
```

`GVLS_RR_NAME` と `GVLS_RR_PASS` は RR の名前とそのパスワードに置き換えます。
これは PostgreSQL データベースに格納した RR の情報と一致させる必要があります。

`GVLS_RR_SRC_IFNAME` はフレッツ閉域 IPv6 網につながっているインターフェースを指定します。

`GVLS_RR_UI_ADDR` には `gvls-ui` の IP アドレスを指定します。

`GVLS_RR_DDNS_HOSTKEY` には「[OPEN IPv6 ダイナミック DNS for フレッツ・光ネクスト](https://i.open.ad.jp/)」のホストキー情報を埋めます。
空の場合は IPv6 プレフィクス変更があっても何もしません。

### gvls-rr の起動

`gvls-rr` を起動します。

```shell
$ sudo systemctl enable gvls-rr.service
$ sudo systemctl start gvls-rr.service
```

## gvls-vtep の構築

基本的に [README.md](README.md) と同様です。

但し、 `/etc/default/gvls-vtep` の `RR1_HOST` と `RR2_HOST` は「[OPEN IPv6 ダイナミック DNS for フレッツ・光ネクスト](https://i.open.ad.jp/)」で登録したホスト名に置き換える必要があります。