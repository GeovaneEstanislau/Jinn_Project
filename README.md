# Jinn OS — Microkernel x86_64 Minimalista e Preemptivo em Rust

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

> **Status:** Em desenvolvimento ativo (Alpha v0.0.3)  
> **Arquitetura:** `x86_64` | **Linguagem:** Rust (`#![no_std]`, bare-metal, target `x86_64-unknown-none`)  
> **Bootloader:** Limine Boot Protocol v2 (Higher-Half)  
> **Comunidade:** Desenvolvido no Brasil com foco em soberania computacional, segurança em profundidade e simplicidade radical.

---

## Sobre o Projeto

O **Jinn OS** é um sistema operacional microkernel escrito do zero em Rust moderno. O projeto nasceu da necessidade de construir uma base de computação confiável, minimalista e formalmente verificável, onde apenas o estritamente essencial reside no nível mais privilegiado da CPU (**Ring 0**).

Ao contrário de kernels monolíticos tradicionais, no Jinn OS todos os subsistemas complexos — como drivers de rede, sistemas de arquivos, o servidor gráfico **Aladdin Desktop** e aplicações como o **Jinn Browser** — executam como processos isolados no espaço de usuário (**Ring 3**). A integridade do sistema é garantida por proteção de hardware, paginação estrita e troca de mensagens de alta velocidade (IPC).

---

## Pilares Arquiteturais e Marcos (Alpha v0.0.3)

1. **Preempção Real & Escalonamento:**
   - Escalonador preemptivo Round-Robin guiado pelo Timer PIT (100 Hz / quantum de 10ms) e suporte a APIC Timer local.
   - Tratamento desacoplado de IRQs e EOI no LAPIC para evitar bloqueio do barramento de interrupções.

2. **Isolamento Estrito de Memória (PML4 x86_64 & W^X):**
   - Paginação em 4 níveis com espaço de endereçamento canônico estrito.
   - Execução em Ring 3 mapeada em endereços seguros (`>= 0x400000`), sem acesso direto à memória física ou estruturas de controle de Ring 0.
   - Aplicação de política W^X (*Write XOR Execute*) em todas as páginas alocadas dinamicamente.

3. **Privilégios Mínimos e Interface Canônica de Syscalls:**
   - Ring 0 restrito à troca de contexto, alocação de páginas físicas, despacho de interrupções e interface canônica de chamadas de sistema (Syscalls `0..85` via `syscall`/`sysret` e fallback `int 0x80`).
   - O microkernel nunca armazena, processa ou manipula chaves privadas ou credenciais criptográficas.

4. **Comunicação por Mensagens (IPC Lock-Free):**
   - Fila estática de mensagens por processo sem alocações dinâmicas no caminho crítico de execução.

5. **Virtual File System (VFS) com Persistência Transacional:**
   - Suporte nativo e simultâneo a **FAT32** (`/disk`) e **Ext2**.
   - Subsistema de **BlockCache** com política LRU para aceleração de I/O de disco.
   - Permissões POSIX refinadas (`O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`).
   - Camada de integridade transacional rotacional (`.TMP` -> `.BAK`) para salvaguarda de configurações, cache, histórico e cookies de aplicações Ring 3 contra desligamentos abruptos.

6. **Ambiente Gráfico Aladdin Desktop em Ring 3:**
   - Servidor gráfico e compositor de janelas operando inteiramente em espaço de usuário (`apps/aladdin-server`).
   - Renderização sobre Linear Framebuffer de alta resolução fornecido pelo Limine.
   - Suporte a mouse PS/2 com processamento de pacotes desacoplado, rastreamento de cursor em tempo real e eventos de janela (`Focus`, `Drag`, `Resize`, `Close`).

7. **Pilha de Rede Soberana, JinnWall SPI & Criptografia PKI:**
   - Pilha de rede IPv4/TCP/UDP/DHCP/DNS bare-metal suportando adaptadores VirtIO-Net e Intel E1000.
   - Firewall com inspeção de estado (**JinnWall SPI**).
   - Validação de certificados **PKI X.509** e suporte a Handshake seguro **TLS 1.3** (RFC 8446) com HKDF-SHA256/384.

8. **Aplicações Nativas Ring 3:**
   - **Jinn Browser:** Navegador gráfico nativo sem dependências de bibliotecas C, com renderização de layouts HTML/CSS, histórico, favoritos, cookies e cache persistente em `/disk/browser/`.
   - **Aladdin Shell:** Terminal interativo com utilitários de diagnóstico de sistema (`sysinfo`, `vfs`, `net`, `ps`, `mem`).
   - **Aladdin Wallet:** Carteira Web3 não custodial com derivação determinística BIP-39/BIP-44, executada sob isolamento estrito de memória.

---

## Estrutura do Repositório

```text
jinn/
├── Cargo.toml            # Manifesto da crate principal do microkernel
├── limine.conf           # Configuração do bootloader Limine v2
├── build.rs              # Script de compilação e linkagem do kernel
├── apps/                 # Aplicações e serviços nativos de espaço de usuário (Ring 3)
│   ├── aladdin-server/   # Servidor gráfico Aladdin Desktop (Compositor Ring 3)
│   └── ola-mundo/        # Jinn Browser (browser.rs) e Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK e bibliotecas cliente para Ring 3
│   └── aladdin-sdk/      # Wrappers de Syscalls, gráficos, eventos e IPC
├── contracts/            # Smart contracts BEP-20 / EVM e suíte Hardhat
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # Testes unitários e invariantes de segurança na EVM
│   └── scripts/          # Automação de deploy para redes BSC Testnet/Mainnet
├── tools/                # Utilitários de desenvolvimento e simuladores
│   ├── jinn-web3/        # jinn-web3 (CLI) e jinn-web3d (Daemon em Ring 3)
│   └── jda-sim/          # Simulador de pipelines DAG e benchmarks SMP
├── web/                  # Portal web oficial do Jinn OS (jinn-os.dev.br / GitHub Pages)
├── docs/                 # Especificações técnicas, ADRs e relatórios de auditoria
└── src/                  # Código-fonte principal do microkernel (Ring 0)
    ├── main.rs           # Ponto de entrada (_start) e inicialização dos subsistemas
    ├── gdt.rs            # Global Descriptor Table e TSS (Ring 0 / Ring 3)
    ├── idt.rs            # Interrupt Descriptor Table e rotinas ISR/IRQ
    ├── syscall.rs        # Interface canônica de chamadas de sistema (Syscalls 0..85)
    ├── scheduler.rs      # Escalonador preemptivo multitarefa
    ├── ipc.rs            # Mensageria estática entre processos
    ├── memory/           # Gerenciamento de quadros físicos (Buddy), Heap e PML4
    ├── drivers/          # Drivers (PIT, LAPIC, Teclado, Mouse PS/2, IDE, PCI, VirtIO, E1000)
    ├── net/              # Pilha TCP/IP, firewall JinnWall SPI, TLS 1.3 e X.509
    └── vfs/              # Virtual File System (FAT32, Ext2, BlockCache LRU)
```

---

## Como Compilar e Executar

### Pré-requisitos
- **Rust:** Versão `nightly` com suporte ao target `x86_64-unknown-none`.
- **Ferramentas:** `qemu-system-x86_64`, PowerShell 7+ (Windows/Linux) ou Python 3.

```bash
# Adicionar o target bare-metal no Rust
rustup target add x86_64-unknown-none
```

### 1. Compilação Completa do Microkernel e Aplicações
Execute o script de build integrado para compilar o kernel, os binários Ring 3 e gerar a imagem de disco FAT32 com os artefatos:

```powershell
# No PowerShell (raiz do repositório)
.\scripts\build.ps1
```

### 2. Execução no QEMU
Inicie a máquina virtual simulando o hardware x86_64 com aceleração ou emulação de periféricos:

```powershell
# Execução direta do Jinn OS com Aladdin Desktop e rede
.\scripts\run.ps1

# Ou com restrição de memória estrita (ex: teste de homologação 64MB RAM):
.\scripts\run.ps1 -Memory 64M
```

---

## Ecossistema Web3 e Jinn Open Economy ($JINN)

Para garantir sustentabilidade econômica soberana, financiar a pesquisa de sistemas operacionais de código aberto e recompensar contribuidores via *Proof of Useful Contribution* (PoUC), o projeto integra a infraestrutura do token nativo **$JINN** na BNB Smart Chain (BSC).

> [!IMPORTANT]
> **Isolamento Inviolável de Credenciais:** O microkernel Jinn (Ring 0) não armazena, processa ou recebe chaves privadas, sementes mnemônicas ou credenciais financeiras. Toda a camada Web3 reside estritamente em espaço de usuário (Ring 3), isolada no processo da **Aladdin Wallet**.

- **Proposta e Whitepaper Econômico:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **Relatório de Validação EVM & Auditoria Fase II:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **Governança e Tesouraria Multi-Sig:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **Especificação do PoUC (Bounties):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **Arquitetura da Carteira (Ring 3):** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Aladdin Wallet (Código Ring 3 UI):** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **Daemon Userspace `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **Economia Computacional e DePIN (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **Auditoria Interna dos Smart Contracts:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **Guia de Implantação em Testnet:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## Licença e Governança

O **Jinn OS** adota o modelo de **Duplo Licenciamento (Dual Licensing)**:

1. **Licença Aberta / Comunitária (`GPL-2.0-only`):** Destinada à comunidade open source, entusiastas e pesquisadores acadêmicos. O uso e trabalhos derivados são livres e abertos sob os termos da GNU General Public License versão 2 ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt)).
2. **Licença Comercial Proprietária (`Jinn-Commercial-1.0`):** Destinada ao uso corporativo, embarcado e industrial em dispositivos fechados ou produtos comerciais que exigem isenção das obrigações copyleft da GPL ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt)).

Para detalhes completos e orientação sobre o regime de licenças, consulte [`LICENSE`](LICENSE). Contribuições de código exigem a assinatura prévia do [Contributor License Agreement (`CLA.md`)](CLA.md) e observância às diretrizes de [Contribuição (`CONTRIBUTING.md`)](CONTRIBUTING.md).

---

## Contato e Contribuição

Este projeto valoriza profundamente a comunidade global de desenvolvedores Rust e entusiastas de sistemas operacionais e computação de baixo nível. Contribuições com código, documentação, testes de homologação de hardware e relatórios de auditoria são muito bem-vindas! Consulte [CONTRIBUTING.md](CONTRIBUTING.md) para diretrizes de envio de patches e pull requests.
