# Title: 🇨🇳 贡献者招募与文档审校：简体中文社区 (README.zh.md)
# Labels: documentation, i18n, help wanted, good first issue

欢迎来到 **Jinn OS** 开源社区！

我们刚刚推出了 Jinn OS 核心技术文档的简体中文版本：[**`README.zh.md`**](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.zh.md)。

### 🎯 本 Issue 的目标
**Jinn OS** 是一个完全采用现代 Rust 语言从零实现的抢占式 **x86_64 微内核**操作系统（`#![no_std]` 裸机模式）。项目遵循最小特权原则，仅将最核心的机制保留在 **Ring 0**，而将设备驱动、文件系统、网络协议栈以及 **Aladdin Desktop** 窗口合成器完全置于 **Ring 3** 用户态运行。

为了让中文社区的系统底层开发者、Rustaceans 和操作系统研究者能够更顺畅地了解和参与项目，我们诚挚邀请大家协助完善中文文档并共同参与核心开发！

### 🔍 期待您的参与
1. **技术用语与表述审校：**
   - 检查低级别系统概念（如：四级分页 PML4、W^X 内存写时不可执行保护、PIT 100Hz 抢占式调度、无锁 IPC 进程间通信、具备 `.TMP` -> `.BAK` 事务性轮转持久化的 VFS 虚拟文件系统、Ring 3 Aladdin Desktop 图形合成服务等）的中文翻译是否符合系统工程规范。
   - 提出更加符合中文阅读习惯的排版与措辞改进建议。
2. **参与底层内核与用户态生态研发：**
   - 编写与适配硬件驱动（VirtIO、NVMe、E1000、PCI 等）。
   - 完善 Ring 3 桌面应用（原生轻量浏览器 **Jinn Browser**、交互式终端 **Aladdin Shell**、Web3 非托管钱包 **Aladdin Wallet**）。
   - 在真实裸机硬件或不同虚拟机环境（QEMU、VirtualBox）中进行兼容性压测与报告反馈。
3. **技术交流与想法探讨：**
   - 欢迎在本 Issue 下方直接留言讨论架构设计、提出功能建议或反馈问题！

### 📚 开始参与
- 阅读中文文档：[`README.zh.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/README.zh.md)
- 贡献指南：[`CONTRIBUTING.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CONTRIBUTING.md)
- 贡献者许可协议：[`CLA.md`](https://github.com/GeovaneEstanislau/Jinn_Project/blob/main/CLA.md)

感谢大家共同构建基于 Rust 的现代化、主权可控的微内核计算基石！

