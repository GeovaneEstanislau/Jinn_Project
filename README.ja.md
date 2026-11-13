# Jinn OS — Rust で書かれた極小・プリエンプティブ x86_64 マイクロカーネル

<div align="center">

| Language | Status | File | Language | Status | File |
| :--- | :---: | :---: | :--- | :---: | :---: |
| 🇧🇷 **Português** | 🟢 Concluído | [README.md](README.md) | 🇮🇳 **Hindi (हिन्दी)** | 🟢 Concluído | [README.hi.md](README.hi.md) |
| 🇺🇸 **English** | 🟢 Concluído | [README.en.md](README.en.md) | 🇮🇹 **Italiano** | 🟢 Concluído | [README.it.md](README.it.md) |
| 🇨🇳 **中文 (Chinese)** | 🟢 Concluído | [README.zh.md](README.zh.md) | 🇭🇺 **Magyar (Hungarian)**| 🟢 Concluído | [README.hu.md](README.hu.md) |
| 🇯🇵 **日本語 (Japanese)** | 🟢 Concluído | [README.ja.md](README.ja.md) | 🇭🇷 **Hrvatski (Croatian)** | 🟢 Concluído | [README.hr.md](README.hr.md) |
| 🇪🇸 **Español** | 🟢 Concluído | [README.es.md](README.es.md) | 🇳🇴 **Norsk (Norwegian)** | 🟢 Concluído | [README.no.md](README.no.md) |
| 🇷🇺 **Русский (Russian)**| 🟢 Concluído | [README.ru.md](README.ru.md) | 🇩🇪 **Deutsch (German)** | 🟢 Concluído | [README.de.md](README.de.md) |

</div>

> **開発ステータス:** 活発に開発中 (Alpha v0.0.3)  
> **アーキテクチャ:** `x86_64` | **開発言語:** Rust (`#![no_std]`, ベアメタル, ターゲット `x86_64-unknown-none`)  
> **ブートローダ:** Limine Boot Protocol v2 (Higher-Half)  
> **コミュニティ:** ブラジル発。コンピューティングの主権性、多層防御セキュリティ、そして極限のシンプルさを追求。

---

## プロジェクト概要

**Jinn OS** は、最新の Rust 言語を用いてゼロから構築されたクリーンシート・マイクロカーネル（Microkernel）オペレーティングシステムです。本プロジェクトは、最も特権的な CPU モード（**Ring 0**）に極限まで削ぎ落とした不可欠な基盤機構のみを配置し、高信頼・最小構成・形式的検証が可能な計算基盤を構築するという思想から誕生しました。

従来のモノリシックカーネルとは異なり、ネットワークドライバ、ファイルシステム、**Aladdin Desktop** ウィンドウ合成サーバ、そして **Jinn Browser** などの各種アプリケーションは、すべて隔離されたユーザー空間（**Ring 3**）プロセスとして動作します。システムの完全性とプロセス境界は、ハードウェアレベルのページング、厳格な権限検証、および極めて高速なロックフリー・プロセス間通信（IPC）によって堅牢に保護されています。

---

## 主要な設計原則と達成マイルストーン (Alpha v0.0.3)

1. **完全なプリエンプティブ・マルチタスクスケジューリング:**
   - ハードウェア PIT タイマー（100 Hz / 10ms タイムクォンタム）およびローカル APIC タイマーに基づくラウンドロビン・プリエンプティブスケジューラ。
   - 割り込み処理（IRQ）の完全デカップリングと明示的な LAPIC EOI（End-Of-Interrupt）シグナリングにより、割り込みバスのスタベーションを防止。

2. **厳格なメモリ空間の隔離（x86_64 PML4 および W^X セキュリティ）:**
   - 4 レベル・ページテーブルによるカノニカル仮想アドレス空間の厳密な管理。
   - Ring 3 プロセスは安全な上位ユーザー空間（`>= 0x400000`）に配置され、物理メモリや Ring 0 制御構造体への直接アクセスは一切遮断。
   - すべての動的割り当てページに W^X（*Write XOR Execute*：書き込み可能かつ実行可能なページの禁止）を徹底適用。

3. **最小特権の原則と正規システムコール（Syscalls）インターフェース:**
   - Ring 0 の責務をコンテキストスイッチ、物理ページフレーム割り当て、ハードウェア割り込み配送、および正規システムコール配送（`syscall`/`sysret` による Syscall `0..85`、および `int 0x80` トラップ互換）のみに限定。
   - マイクロカーネルは秘密鍵やニーモニックシードなどの暗号資産情報を一切保持・処理・保存しません。

4. **ロックフリー・プロセス間通信（Lock-Free IPC）:**
   - プロセスごとに静的に割り当てられたメッセージキューを備え、クリティカルパス上でのヒープ動的メモリ割り当てを完全に排除。

5. **トランザクション整合性を備えた仮想ファイルシステム（VFS）:**
   - **FAT32**（`/disk`）および **Ext2** の同時ネイティブサポート。
   - ディスク I/O を劇的に高速化する LRU（Least Recently Used）方式の専用 **BlockCache** サブシステム。
   - 標準 POSIX アクセスフラグ（`O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`）の完全実装。
   - ローテーション整合性メカニズム（`.TMP` -> `.BAK`）により、突然の電源切断時でもブラウザのキャッシュ、履歴、ブックマーク、Cookie の破損を完全に防止。

6. **Ring 3 ネイティブ Aladdin Desktop グラフィカル環境:**
   - 完全にユーザー空間で独立して動作するウィンドウ合成サーバ（`apps/aladdin-server`）。
   - Limine プロトコル提供の高解像度リニアフレームバッファ（Linear Framebuffer）への直接描画。
   - デカップル方式のパケット解析を備えた PS/2 マウスドライバによる、滑らかなリアルタイムカーソル追従とウィンドウイベント配送（`Focus`, `Drag`, `Resize`, `Close`）。

7. **自律的ネットワークスタック、JinnWall SPI ファイアウォール、PKI 暗号:**
   - 外部ライブラリに依存しないベアメタル IPv4/TCP/UDP/ARP/DHCP/DNS スタック（VirtIO-Net および Intel E1000 NIC 対応）。
   - ステートフル・パケット・インスペクション（**JinnWall SPI**）ファイアウォール。
   - **PKI X.509** デジタル証明書検証および HKDF-SHA256/384 鍵スケジュールを備えた **TLS 1.3**（RFC 8446）セキュアハンドシェイク。

8. **ネイティブ Ring 3 アプリケーション群:**
   - **Jinn Browser:** C 言語ライブラリに依存しないネイティブ GUI ブラウザ。HTML/CSS レイアウトエンジン、HTTP クライアント、ローカルキャッシュ、ブックマーク、および `/disk/browser/` 配下の永続的ナビゲーション履歴を装備。
   - **Aladdin Shell:** カーネル内部状態を監視・診断できる対話型ターミナル（`sysinfo`, `vfs`, `net`, `ps`, `mem`）。
   - **Aladdin Wallet:** BIP-39/BIP-44 に準拠した決定論的鍵導出を備えた非カストディ型 Web3 ウォレット（Ring 3 サンドボックスで安全に隔離実行）。

---

## リポジトリ構成

```text
jinn/
├── Cargo.toml            # マイクロカーネル・ルートワークスペース構成ファイル
├── limine.conf           # Limine v2 ブートローダ設定ファイル
├── build.rs              # カーネルのビルドおよびリンクスクリプト
├── apps/                 # Ring 3 ネイティブアプリケーションおよびサービス
│   ├── aladdin-server/   # Aladdin Desktop ウィンドウサーバ（Ring 3 コンポジタ）
│   └── ola-mundo/        # Jinn Browser (browser.rs) および Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK および Ring 3 クライアントランタイム
│   └── aladdin-sdk/      # システムコールラッパー、グラフィックス、イベント、IPC
├── contracts/            # BEP-20 スマートコントラクトおよび Hardhat テストスイート
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # EVM 単体テストおよび安全不変条件検証
│   └── scripts/          # BSC テストネット・メインネット自動デプロイスクリプト
├── tools/                # 開発支援ユーティリティおよびシミュレーション環境
│   ├── jinn-web3/        # jinn-web3 (CLI) および jinn-web3d (Ring 3 デーモン)
│   └── jda-sim/          # DAG パイプラインシミュレータおよび SMP ベンチマーク
├── web/                  # Jinn OS 公式ポータルサイト (jinn-os.dev.br / GitHub Pages)
├── docs/                 # 技術仕様書、アーキテクチャ決定記録 (ADR)、監査レポート
└── src/                  # マイクロカーネル本体ソースコード (Ring 0)
    ├── main.rs           # カーネルエントリポイント (_start) およびサブシステム初期化
    ├── gdt.rs            # グローバル記述子テーブル (GDT) および TSS (Ring 0 / Ring 3)
    ├── idt.rs            # 割り込み記述子テーブル (IDT) および ISR/IRQ ハンドラ
    ├── syscall.rs        # 正規システムコールディスパッチャ (Syscalls 0..85)
    ├── scheduler.rs      # プリエンプティブ・マルチタスクスケジューラ
    ├── ipc.rs            # 静的ロックフリー・プロセス間通信エンジン
    ├── memory/           # 物理フレームアロケータ (Buddy)、カーネルヒープ、PML4
    ├── drivers/          # デバイスドライバ (PIT, LAPIC, キーボード, PS/2 マウス, IDE, PCI, VirtIO, E1000)
    ├── net/              # TCP/IP スタック、JinnWall SPI、TLS 1.3、X.509 PKI
    └── vfs/              # 仮想ファイルシステム (FAT32, Ext2, LRU BlockCache)
```

---

## ビルドと実行方法

### 必須要件
- **Rust ツールチェーン:** `x86_64-unknown-none` ターゲットがインストールされた `nightly` コンパイラ。
- **エミュレータ・周辺ツール:** `qemu-system-x86_64`、PowerShell 7+ (Windows/Linux) または Python 3。

```bash
# ベアメタル x86_64 ターゲットの追加
rustup target add x86_64-unknown-none
```

### 1. マイクロカーネルとアプリケーションのビルド
統合ビルドスクリプトを実行することで、カーネルおよび Ring 3 アプリケーションをコンパイルし、FAT32 起動ディスクイメージを生成します：

```powershell
# リポジトリのルートディレクトリで実行
.\scripts\build.ps1
```

### 2. QEMU エミュレータでの起動
仮想マシンを起動し、ハードウェア機能と周辺機器をエミュレートして動作確認を行います：

```powershell
# Aladdin Desktop および仮想ネットワークを有効にして起動
.\scripts\run.ps1

# 厳格なメモリ制約下でのストレステスト（例: 64MB RAM 上限テスト）:
.\scripts\run.ps1 -Memory 64M
```

---

## Web3 エコシステムと Jinn Open Economy ($JINN)

オープンソース OS 研究開発の持続可能性を確保し、有益な貢献の証明（*Proof of Useful Contribution*, PoUC）を通じて貢献者に報いるため、BNB Smart Chain（BSC）上にネイティブトークン **$JINN** を統合しています。

> [!IMPORTANT]
> **秘密鍵の不可侵分離原則:** Jinn マイクロカーネル（Ring 0）は、ユーザーの秘密鍵、シードフレーズ、または金融トランザクションにアクセス、保存、または関与することは絶対にありません。すべての暗号資産操作は、ユーザー空間（Ring 3）の **Aladdin Wallet** アプリケーション内に厳密に隔離されています。

- **経済提案書およびホワイトペーパー:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **EVM 検証およびフェーズ II 監査レポート:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **マルチシグ・ガバナンスおよび財務仕様:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **PoUC 仕様書 (バウンティ制度):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Ring 3 ウォレット設計仕様:** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Aladdin Wallet 実装コード:** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **ユーザー空間デーモン `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **DePIN 計算経済仕様 (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **スマートコントラクト内部監査報告書:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **テストネット・デプロイ手順書:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## ライセンスとガバナンス

**Jinn OS** は **デュアルライセンス方式（Dual Licensing）** を採用しています：

1. **オープンソース・コミュニティライセンス（`GPL-2.0-only`）:** オープンソースコミュニティ、教育機関、研究者向け。派生著作物や再頒布は GNU General Public License バージョン 2 の条件に従います（[`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)）。
2. **商用プロプライエタリライセンス（`Jinn-Commercial-1.0`）:** GPL のコピーレフト義務免除を必要とする産業機器、車載システム、または商用製品向け（[`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)）。

法的な詳細条項については [`LICENSE`](LICENSE) を参照してください。コードのコントリビューションには、事前に [コントリビューターライセンス同意書 (`CLA.md`)](CLA.md) への同意、および [コントリビューションガイドライン (`CONTRIBUTING.md`)](CONTRIBUTING.md) の遵守が必要です。

---

## コミュニティとコントリビューション

Jinn OS は、世界中および日本の Rust エンジニア、組み込み開発者、低レイヤ技術者の皆さまからのフィードバックと参加を心より歓迎いたします。カーネルの高速化、ドライバの実装、セキュリティ監査、ドキュメントの改善など、あらゆるご協力を歓迎します。パッチや Pull Request の送信方法については [CONTRIBUTING.md](CONTRIBUTING.md) をご確認ください。
