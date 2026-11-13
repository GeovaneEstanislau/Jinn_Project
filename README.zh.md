# Jinn OS — 基于 Rust 的极简抢占式 x86_64 微内核操作系统

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

> **开发状态：** 积极开发中 (Alpha v0.0.3)  
> **处理器架构：** `x86_64` | **编程语言：** Rust (`#![no_std]`, 裸机模式, 目标平台 `x86_64-unknown-none`)  
> **引导协议：** Limine Boot Protocol v2 (Higher-Half)  
> **社区源起：** 发起于巴西，致力于计算主权、纵深防御安全与极致简约的设计理念。

---

## 项目概述

**Jinn OS** 是一个完全采用现代 Rust 语言从零实现的微内核（Microkernel）操作系统。该项目的诞生源于构建一个可靠、极简且形式化可验证的底层计算底座的需求。系统遵循最小特权原则，仅将维持系统运转所绝对必需的基础机制保留在 CPU 最高的特权级别（**Ring 0**）。

与传统的宏内核架构不同，Jinn OS 中的所有复杂外设与子系统——包括网络设备驱动程序、文件系统、**Aladdin Desktop** 窗口合成服务以及 **Jinn Browser** 等原生应用程序——均作为独立的进程运行在用户空间（**Ring 3**）。系统的稳定性与进程边界由硬件级分页隔离、严格的能力校验以及超轻量、无锁的进程间通信（IPC）机制提供坚实保障。

---

## 核心架构支柱与里程碑 (Alpha v0.0.3)

1. **确定性实时抢占与多任务调度：**
   - 基于可编程间隔定时器 PIT（100 Hz / 10ms 时间片）驱动的抢占式 Round-Robin 调度器，并支持本地 APIC 定时器。
   - 彻底解耦硬件中断处理与中断结束确认（LAPIC EOI），防止高负载下的中断总线饥饿。

2. **严格的内存隔离机制（x86_64 PML4 与 W^X 安全策略）：**
   - 4 级页表管理，严格规范虚拟地址空间布局。
   - 用户态任务（Ring 3）映射在规范的高位安全空间（`>= 0x400000`），完全杜绝直接访问物理内存或 Ring 0 控制结构。
   - 在所有动态分配的物理页帧上严格推行 W^X（*Write XOR Execute*，写时不可执行）内存安全策略。

3. **最小特权与规范系统调用接口（Syscalls）：**
   - Ring 0 严格限定于上下文切换、物理页帧分配、硬件中断分发以及规范系统调用分发（通过 `syscall`/`sysret` 暴露系统调用 `0..85`，并保留兼容性 `int 0x80` 陷阱机制）。
   - 微内核永远不触碰、不存储、也不处理任何私钥或助记词等加密敏感凭据。

4. **无锁进程间通信（Lock-Free IPC）：**
   - 为每个进程提供静态绑定的 IPC 消息队列，在关键分发路径上做到绝对零堆内存动态分配。

5. **具备事务性持久化的虚拟文件系统（VFS）：**
   - 原生并发支持 **FAT32**（`/disk`）与 **Ext2** 文件系统。
   - 专门实现的 **BlockCache** 块缓存子系统，采用最近最少使用（LRU）置换算法以加速磁盘 I/O。
   - 完备的 POSIX 访问标志支持（`O_RDONLY`、`O_WRONLY`、`O_RDWR`、`O_CREAT`、`O_APPEND`、`O_TRUNC`）。
   - 轮转事务性安全写入机制（`.TMP` -> `.BAK`），彻底杜绝系统在遭遇意外断电时出现浏览器历史、书签、Cookie 或配置文件的破损与坏道。

6. **Ring 3 原生 Aladdin Desktop 图形桌面环境：**
   - 完全在用户态独立运行的窗口合成与显示服务（`apps/aladdin-server`）。
   - 基于 Limine 协议的高分辨率线性帧缓冲区（Linear Framebuffer）直接呈现。
   - 支持解耦式数据包摄取的 PS/2 鼠标驱动，提供实时光标平滑跟踪与完备的窗口交互事件（`Focus`、`Drag`、`Resize`、`Close`）。

7. **原生网络协议栈、JinnWall SPI 防火墙与 PKI 密码学支持：**
   - 纯 Rust 裸机实现的 IPv4/TCP/UDP/ARP/DHCP/DNS 网络栈，原生支持 VirtIO-Net 与 Intel E1000 网卡驱动。
   - 基于状态检测的报文防火墙（**JinnWall SPI**）。
   - 原生 **PKI X.509** 数字证书链验证与 **TLS 1.3** 安全握手（RFC 8446），内置 HKDF-SHA256/384 密钥调度派生。

8. **原生 Ring 3 用户态应用套件：**
   - **Jinn Browser：** 零 C 库依赖的原生轻量级图形网页浏览器，具备 HTML/CSS 布局排版引擎、HTTP 客户端，并在 `/disk/browser/` 下提供结构化持久化缓存、书签与访问历史。
   - **Aladdin Shell：** 交互式终端解释器，内置完整的内核诊断命令（`sysinfo`、`vfs`、`net`、`ps`、`mem`）。
   - **Aladdin Wallet：** 遵循 BIP-39/BIP-44 规范的非托管 Web3 资产钱包，在 Ring 3 独立沙盒中安全运行。

---

## 仓库目录结构

```text
jinn/
├── Cargo.toml            # 微内核根工作区配置文件
├── limine.conf           # Limine v2 引导加载程序配置文件
├── build.rs              # 内核构建与链接脚本
├── apps/                 # Ring 3 原生用户态应用与服务
│   ├── aladdin-server/   # Aladdin Desktop 窗口合成服务端
│   └── ola-mundo/        # Jinn Browser (browser.rs) 与 Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK 与用户态客户端运行时
│   └── aladdin-sdk/      # 系统调用封装、图形界面、事件轮询与 IPC
├── contracts/            # BEP-20 智能合约与 Hardhat 自动化测试套件
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # EVM 单元测试与形式化安全不变性测试
│   └── scripts/          # BSC 测试网与主网部署脚本
├── tools/                # 辅助开发工具与仿真套件
│   ├── jinn-web3/        # jinn-web3 (CLI 命令行工具) 与 jinn-web3d (Ring 3 守护进程)
│   └── jda-sim/          # DAG 流水线模拟器与 SMP 基准测试工具
├── web/                  # Jinn OS 官方网站 (jinn-os.dev.br / GitHub Pages)
├── docs/                 # 技术设计规格书、架构决策记录 (ADR) 与审计报告
└── src/                  # 微内核核心源码 (Ring 0)
    ├── main.rs           # 内核入口 (_start) 与基础子系统初始化
    ├── gdt.rs            # 全局描述符表与任务状态段 (Ring 0 / Ring 3)
    ├── idt.rs            # 中断描述符表与 ISR/IRQ 中断服务例程
    ├── syscall.rs        # 规范系统调用分发调度器 (Syscalls 0..85)
    ├── scheduler.rs      # 抢占式多任务调度器
    ├── ipc.rs            # 静态无锁进程间消息传递引擎
    ├── memory/           # 物理页帧分配器 (Buddy)、内核堆与 PML4
    ├── drivers/          # 设备驱动 (PIT, LAPIC, 键盘, PS/2 鼠标, IDE, PCI, VirtIO, E1000)
    ├── net/              # TCP/IP 协议栈、JinnWall 防火墙、TLS 1.3 密码学与 X.509
    └── vfs/              # 虚拟文件系统 (FAT32, Ext2, LRU BlockCache)
```

---

## 编译与运行指南

### 环境准备
- **Rust 工具链：** 安装有 `x86_64-unknown-none` 目标平台的 `nightly` 编译器。
- **仿真与辅助工具：** `qemu-system-x86_64`，PowerShell 7+（Windows/Linux）或 Python 3。

```bash
# 安装裸机 x86_64 交叉编译目标
rustup target add x86_64-unknown-none
```

### 1. 编译微内核与用户态应用程序
运行自动化构建脚本即可一次性编译内核、构建用户态 Ring 3 应用程序并自动打包包含 FAT32 文件系统的引导磁盘镜像：

```powershell
# 在代码仓库根目录下执行
.\scripts\build.ps1
```

### 2. 在 QEMU 虚拟机中启动运行
通过模拟器启动 Jinn OS 并挂载外设驱动：

```powershell
# 启动包含 Aladdin Desktop 桌面与虚拟网络支持的标准镜像
.\scripts\run.ps1

# 在严苛的物理内存约束下进行稳定性压测（例如 64MB RAM 边界测试）：
.\scripts\run.ps1 -Memory 64M
```

---

## Web3 生态与 Jinn 开放经济模型 ($JINN)

为了实现操作系统开源研究的长久自给自足，并依托“有效贡献证明”（*Proof of Useful Contribution*, PoUC）向开源开发者分发赏金激励，项目在 BNB Smart Chain (BSC) 上推出了原生代币 **$JINN** 的链上基础设施。

> [!IMPORTANT]
> **绝对的私钥隔离红线：** Jinn 微内核（Ring 0）绝不存储、不中继、也不接触任何用户的私钥、助记词或资产凭证。全部 Web3 与链上计算均严格限制在用户态（Ring 3）的 **Aladdin Wallet** 独立进程中。

- **经济学提案与白皮书：** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **EVM 验证与第二阶段审计报告：** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **多签金库与去中心化治理规范：** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **PoUC 贡献证明规范与赏金机制：** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Ring 3 钱包架构设计文档：** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Aladdin Wallet 界面源码：** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **用户态守护进程 `jinn-web3d`：** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **DePIN 计算经济学规范 (JDA)：** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **智能合约内部安全审计报告：** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **测试网部署指南：** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## 许可协议与治理模式

**Jinn OS** 采取**双重许可机制（Dual Licensing）**：

1. **开源社区许可（`GPL-2.0-only`）：** 面向开源爱好者、高校研究学者以及个人开发者。衍生作品与代码再分发均受 GNU 通用公共许可证第 2 版约束（[`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)）。
2. **商业专有许可（`Jinn-Commercial-1.0`）：** 针对企业级商用部署、工业控制系统、嵌入式硬件以及不希望受 GPL 传染性开源约束的商业化产品（[`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)）。

详细法律条款参见 [`LICENSE`](LICENSE)。向项目贡献代码前，请签署 [贡献者许可协议 (`CLA.md`)](CLA.md) 并遵循 [贡献准则 (`CONTRIBUTING.md`)](CONTRIBUTING.md)。

---

## 交流与贡献

Jinn OS 热忱欢迎来自全球及中文 Rust 社区、底层操作系统爱好者以及系统软件工程师的交流与贡献！无论您擅长底层内核编码、硬件驱动适配、密码学审计还是技术文档翻译，我们都期待您的参与。请参阅 [CONTRIBUTING.md](CONTRIBUTING.md) 了解如何提交补丁与 Pull Request。
