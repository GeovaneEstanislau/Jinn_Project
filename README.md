# Jinn OS — Microkernel x86_64 Minimalista e Preemptivo em Rust

> **Status:** Em desenvolvimento ativo  
> **Arquitetura:** `x86_64` | **Linguagem:** Rust (`#![no_std]`, bare-metal)  
> **Bootloader:** Limine Boot Protocol v2 (Higher-Half)  
> **Comunidade:** Desenvolvido no Brasil com foco em seguranca, previsibilidade e simplicidade.

---

## Sobre o Projeto

O **Jinn OS** é um sistema operacional microkernel escrito do zero em Rust moderno. O projeto nasceu da necessidade de construir uma base de computação confiável, minimalista e verificável, onde apenas o estritamente essencial reside no espaço mais privilegiado do processador (Ring 0).

Drivers de dispositivo, sistemas de arquivos, servidores de rede e ambientes gráficos rodam como servidores isolados no espaço de usuário (Ring 3), comunicando-se por meio de passagem de mensagens rápida e sem bloqueio (IPC).

---

## Pilares Arquiteturais

1. **Preempção Real:** Escalonador Round-Robin preemptivo guiado pelo Timer PIT (100 Hz / quantum de 10ms), garantindo que nenhuma tarefa monopolize o processador.
2. **Isolamento de Memória (Paging x86_64):** Paginação em 4 níveis (PML4) gerenciada de forma estrita. Código de Ring 3 não acessa memória física do kernel.
3. **Privilégios Estritos:** Ring 0 dedicado à troca de contexto, paginação básica e despacho de chamadas de sistema (`int 0x80` / `syscall`). O microkernel nunca manipula chaves privadas de ativos criptográficos.
4. **Comunicação por Mensagens (IPC):** Fila de mensagens lock-free por processo, sem alocações dinâmicas no caminho crítico de execução.
5. **JDA (Jinn Directed Acyclic Graph) — Aceleração Assíncrona Zero-Copy:** Subsistema avançado de I/O de disco e computação in-kernel com anéis de submissão/conclusão lock-free inspirados em `io_uring`.
6. **Gráficos e Janelas Nativos no Aladdin Desktop:** Suporte completo a Linear Framebuffer, mouse PS/2 em tempo real, compositor de janelas com foco e terminal dinâmico.

---

## Estrutura do Repositório

```text
jinn/
├── Cargo.toml            # Manifest da crate principal do microkernel
├── limine.conf           # Configuração de bootloader do Limine
├── build.rs              # Script de compilação e linkagem do kernel
├── apps/                 # Aplicações nativas de espaço de usuário (Ring 3)
│   └── ola-mundo/        # Navegador nativo (browser.rs) e Carteira Aladdin (wallet.rs)
├── contracts/            # Contratos inteligentes BEP-20 / EVM e suíte Hardhat
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # Testes unitários e invariantes de segurança na EVM
│   └── scripts/          # Automação de deploy para BSC Testnet/Mainnet
├── tools/                # Utilitários auxiliares do ecossistema
│   ├── jinn-web3/        # jinn-web3 (CLI) e jinn-web3d (Daemon em Ring 3)
│   └── jda-sim/          # Simulador de pipelines DAG e benchmarks SMP
├── docs/                 # Documentação técnica, especificações e relatórios de auditoria
│   ├── adr/              # Architecture Decision Records (ADRs 001 a 006)
│   └── ...               # Whitepapers, PoUC, DePIN e relatórios
└── src/                  # Código-fonte principal do microkernel
    ├── main.rs           # Ponto de entrada (_start) e inicialização
    ├── gdt.rs            # Global Descriptor Table e TSS (Ring 0 / Ring 3)
    ├── idt.rs            # Interrupt Descriptor Table e tratadores
    ├── syscall.rs        # Interface canônica de chamadas de sistema (Syscalls 0..52)
    ├── scheduler.rs      # Escalonador preemptivo de processos
    ├── ipc.rs            # Mensageria IPC estática e lock-free
    ├── memory/           # Gerenciamento de quadros físicos e paginação
    ├── drivers/          # Drivers (PIT, Teclado, Mouse PS/2, IDE, PCI, E1000, NVMe)
    ├── net/              # Pilha de rede nativa IPv4/TCP/UDP/DHCP/DNS e firewall JinnWall
    └── vfs/              # Virtual File System com suporte a FAT32 e Ext2
```

---

## Ecossistema Web3 e Jinn Open Economy ($JINN)

Para garantir a sustentabilidade econômica soberana, financiar pesquisa de sistemas operacionais e recompensar contribuidores de código aberto via *Proof of Useful Contribution* (PoUC), o projeto desenvolve a infraestrutura do token nativo **$JINN** na BNB Smart Chain (BSC).

> [!] **Isolamento Inviolavel:** O microkernel Jinn (Ring 0) nao armazena, processa ou recebe chaves privadas ou sementes mnenonicas. Todas as operacoes criptograficas residem estritamente em userspace (Ring 3) e na aplicacao **Aladdin Wallet**.

- **Proposta e Whitepaper Economico:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **Relatorio de Validacao EVM & Auditoria Fase II:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **Governanca e Tesouraria Multi-Sig:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **Especificacao do PoUC (Bounties):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Arquitetura da Carteira (Ring 3):** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Aladdin Wallet (Codigo Ring 3 UI):** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **Daemon Userspace `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **Economia Computacional e DePIN (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **Auditoria Interna dos Smart Contracts:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **Guia de Implantacao em Testnet:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)
- **Utilitario e Codificador em Rust:** [`tools/jinn-web3/`](tools/jinn-web3/)

---

## Licença e Governança

O **Jinn OS** adota o modelo de **Duplo Licenciamento (Dual Licensing)**:

1. **Licença Aberta / Comunitária (`GPL-2.0-only`):** Destinada à comunidade open source, entusiastas e pesquisadores acadêmicos. O uso e trabalhos derivados são livres e abertos sob os termos da GNU General Public License versão 2 ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Licença Comercial Proprietária (`Jinn-Commercial-1.0`):** Destinada ao uso corporativo, embarcado e industrial em dispositivos de hardware fechados ou produtos comerciais que exigem isenção das obrigações copyleft da GPL ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)).

Para detalhes completos e orientação sobre o regime de licenças, consulte [`LICENSE`](LICENSE). Contribuições de código exigem a assinatura prévia do [Contributor License Agreement (`CLA.md`)](CLA.md) e observância às diretrizes de [Contribuição (`CONTRIBUTING.md`)](CONTRIBUTING.md).

## Contato e Contribuição

Este projeto valoriza imensamente a comunidade de tecnologia brasileira. Estamos buscando desenvolvedores e entusiastas de sistemas operacionais. Veja o arquivo [CONTRIBUTING.md](CONTRIBUTING.md) para saber por onde começar.
