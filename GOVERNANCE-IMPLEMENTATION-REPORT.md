# Relatório de implementação — governança e licenciamento

**Projeto:** Jinn Project

**Data:** 2 de novembro de 2026

**Escopo executado:** Etapas 1, 2, 3 e 4 do plano de governança (Consolidação Completa)

## Resumo executivo

Os documentos de governança, inventário de licenças de terceiros, contratos de ABI de chamadas de sistema (Syscalls 0 a 44) e os workflows de verificação de SPDX, CLA e compilação do CI foram integralmente implementados, auditados e sincronizados com a branch `main`.

O pull request inicial de governança, [#48](https://github.com/GeovaneEstanislau/Jinn_Project/pull/48), foi mesclado em `main` pelo mantenedor. As pendências remanescentes (falha de compilação no CI devido a commit defasado de Aladdin, ausência da branch remota `cla-signatures`, necessidade de inventário de licenças de terceiros e expansão da matriz de contratos de syscalls) foram completamente resolvidas e validadas no commit `eb4dc46`.

## Entregas

### Etapa 1 — Documentos de governança

- [`CLA.md`](CLA.md): CLA v1.0 em português, incluindo sublicenciamento comercial irrevogável, licença de patentes, declarações do colaborador e foro de Belo Horizonte/MG.
- [`CONTRIBUTING.md`](CONTRIBUTING.md): fluxo de pull request, exigência de CLA, arquitetura em camadas L0–L3, isolamento IPC, formato de cabeçalho SPDX, registro de dependências e instruções técnicas de compilação.
- [`LICENSE`](LICENSE): orientação sobre o modelo de licenciamento duplo e ressalva expressa de que ele não relicencia retroativamente conteúdo de terceiros ou material sem direitos verificados.
- [`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt): texto integral da GNU General Public License v2.
- [`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt): minuta preliminar de avaliação e uso corporativo por núcleo, sujeita a termos proprietários.

### Etapa 2 — Automação e CI

- [`.github/workflows/spdx-license-check.yml`](.github/workflows/spdx-license-check.yml): compara os arquivos-fonte alterados no PR com a base e falha quando não encontra o identificador SPDX de licenciamento duplo nas primeiras dez linhas.
- [`.github/workflows/cla-check.yml`](.github/workflows/cla-check.yml): automatiza a verificação de assinatura de CLA via CLA Assistant, persistindo os registros na branch remota `cla-signatures`.
- [`.github/workflows/ci.yml`](.github/workflows/ci.yml): fluxo de integração contínua corrigido para clonar a branch `main` do Aladdin Desktop, garantindo compatibilidade da ABI gráfica e compilação limpa do microkernel Ring 0, testes e imagem ISO.

### Etapa 3 — Inventário de Licenças de Terceiros

- [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md): auditoria formal de todos os componentes de terceiros utilizados no sistema operacional, estruturados por camadas:
  - **L0 (Microkernel Core):** zero dependências externas do `crates.io` (`core` e `compiler_builtins` da toolchain Rust `no_std`).
  - **Bootloader e Inicialização:** Limine Bootloader v8.x (`BSD-2-Clause`).
  - **L3 (Ambiente Gráfico Aladdin):** repositório parceiro Aladdin Desktop (`MIT OR Apache-2.0`).
  - **Fontes e Assets:** fonte bitmap embutida 8x16 de domínio público.

### Etapa 4 — Auditoria de Contratos Syscall/IPC

- [`docs/SYSCALL_ABI_RESOURCE_CONTRACT.md`](docs/SYSCALL_ABI_RESOURCE_CONTRACT.md): matriz normativa completa cobrindo todas as 45 chamadas de sistema do microkernel Jinn OS (Syscalls 0 a 44), incluindo as novas chamadas de armazenamento:
  - Syscall 41: `sys_statvfs`
  - Syscall 42: `sys_disk_info`
  - Syscall 43: `sys_mount`
  - Syscall 44: `sys_unmount`

## Commits de Governança e Infraestrutura

| Commit | Mensagem | Escopo |
|---|---|---|
| `b193565` | `docs(governance): add CLA v1.0, CONTRIBUTING guide, and dual-licensing terms` | Etapa 1 |
| `d807a39` | `ci: add SPDX header validator and CLA automation workflows` | Etapa 2 |
| `b3af5e5` | `Merge pull request #48 from GeovaneEstanislau/governance/dual-licensing-setup` | Integração em `main` |
| `eb4dc46` | `docs(governance): add THIRD_PARTY_LICENSES, update syscall ABI contract to 44, and fix CI Aladdin pin` | Etapas 2, 3 e 4 |

## Verificações no GitHub Actions

- **CLA Assistant:** operacional via `.github/workflows/cla-check.yml` apontando para o branch órfão `cla-signatures`.
- **Build Kernel, Ring 3 App & ISO:** executado com sucesso no commit `eb4dc46` ([Run ID 41545140002](https://github.com/GeovaneEstanislau/Jinn_Project/actions/runs/41545140002)):
  - ✓ Set up job
  - ✓ Checkout repository
  - ✓ Clone Aladdin Desktop (`main`)
  - ✓ Install Rust Nightly Toolchain
  - ✓ Install Build Dependencies (`xorriso`, `mtools`)
  - ✓ Build Jinn Kernel (Ring 0)
  - ✓ Run JDA Simulator & Verification Tests
  - ✓ Run RFC 1035 DNS, Cache and JinnWall Verification Tests
  - ✓ Build Ring 3 User Application (`ola-mundo`)
  - ✓ Prepare ISO root structure
  - ✓ Generate FAT32 Virtual Disk Image (`disk.img`)
  - ✓ Setup Limine Bootloader
  - ✓ Create Hybrid Bootable ISO (`jinn.iso`)
  - ✓ Upload ISO Artifact & FAT32 Disk Image Artifact

## Estado dos Repositórios

- **Jinn OS (`E:\Projetos\Jinn`):** branch `main`, sincronizado com `origin/main` (`eb4dc46`), working tree limpa.
- **Aladdin Desktop (`E:\Projetos\Aladdin`):** branch `main`, sincronizado com `origin/main` (`4945754`), working tree limpa.
- **Branch CLA Signatures:** `origin/cla-signatures` criada e disponível no GitHub para persistência de assinaturas.

## Recomendações Finais de Configuração no Repositório Remoto

Nas configurações de **Branch Protection Rules** do repositório `GeovaneEstanislau/Jinn_Project` no GitHub, configurar para a branch `main`:
1. Exigir verificação de status antes do merge:
   - `CLA Assistant`
   - `SPDX License Check`
   - `Build Kernel, Ring 3 App & ISO`
2. Certificar-se de que a branch `cla-signatures` permaneça desprotegida para que o bot do CLA Assistant possa commitar novas assinaturas automaticamente.
