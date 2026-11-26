# Estratégia de Propriedade Intelectual, Licenciamento & Conformidade (`STRATEGY_IP_AND_LICENSING`)

> **Escopo:** Auditoria jurídica-técnica unificada de **`Jinn_Project`**, **`Aladdin`** e **`jhcs-rs`**  
> **Titularidade & Manutenção:** Geovane da Conceição Estanislau / Jinn Project Maintainers  
> **Referências Canônicas:** [`LICENSE`](../LICENSE), [`CLA.md`](../CLA.md), [`CONTRIBUTING.md`](../CONTRIBUTING.md), [`THIRD_PARTY_LICENSES.md`](../THIRD_PARTY_LICENSES.md)  
> *Nota Legal: Documento técnico de governança e engenharia de IP para auditores e parceiros; não constitui parecer jurídico formal de escritório advocatício.*

---

## 1. Matriz de Licenciamento por Repositório e Componente

O ecossistema Jinn separa propositalmente o regime copyleft/comercial do microkernel e aplicações nucleares das camadas modulares de menor atrito destinadas à integração independente em espaço de usuário ou em ambientes host (Linux/Windows):

| Repositório / Componente | Camada | Licença Pública Primária | Via Comercial / OEM | Escopo & Propósito Estratégico |
| :--- | :--- | :--- | :--- | :--- |
| **`Jinn_Project`** (`src/`, `sdk/`, `apps/ola-mundo/`, `apps/aladdin-server/`, `apps/jhcs-daemon/`) | L0–L3 (Microkernel, Syscalls, Rede, VFS, Browser) | **`GPL-2.0-only`** (`LICENSES/GPL-2.0-only.txt`) | **`Jinn-Commercial-1.0`** (`LICENSES/Jinn-Commercial-1.0.txt`) via **`CLA.md`** | Preserva reciprocidade aberta na pesquisa do sistema operacional enquanto permite licenciamento dedicado para appliances/embarcados comerciais fechados. |
| **`Aladdin`** (`../Aladdin`) | L3 (`aladdin` library) | **`MIT OR Apache-2.0`** (`Cargo.toml`) | Permissiva | Compositor Ring 3 e contratos gráficos (`contracts/*`) desacoplados para reutilização livre em sistemas bare-metal ou hosts (`--features std`). |
| **`jhcs-rs`** (`../jhcs-rs`) | Portável Host / L3 | **`Apache-2.0`** (arquivo raiz [`LICENSE`](../../jhcs-rs/LICENSE); crates individuais declaram `MIT OR Apache-2.0` / `Apache-2.0 OR MIT`) | Permissiva (+ OEM/custom via `LICENSING_POLICY.md`) | Ponto de entrada de menor atrito para equipes Linux/Windows e microkernels avaliarem grafos DAG CPU/SIMD/GPU sem copyleft. |
| **`limine/`** (Bootloader BIOS/UEFI) | Boot | **`BSD-2-Clause`** (`THIRD_PARTY_LICENSES.md`) | Permissiva | Binários de inicialização redistribuídos sob termos BSD-2-Clause intactos. |
| **Glifos VGA (`src/font.rs`)** | L0 Fonte | **Domínio Público / CC0** | Livre | Tabela ASCII 8×16 padrão IBM VGA BIOS sem restrições de redistribuição. |

---

## 2. Verificação de Compatibilidade nos Manifestos `Cargo.toml`

A auditoria direta de todos os arquivos `Cargo.toml` nos três workspaces comprova:

1. **Dependências Zero do `crates.io` no Microkernel (`D:\Projetos\Jinn\Cargo.toml`) e Núcleo Ring 3 (`ola-mundo`, `aladdin-sdk`, `contador`, `ola-grafico`):**
   - O kernel `#![no_std]` depende exclusivamente de `core`, `alloc`, `compiler-builtins` e do workspace local `aladdin = { path = "../Aladdin", default-features = false }`.
   - Zero risco de contaminação transitiva de licenças externas em Ring 0.
2. **Compatibilidade Permissiva ↔ Copyleft (`GPL-2.0-only`):**
   - A licença `Apache-2.0` pura é considerada pela FSF incompatível com vinculação estática sob `GPL-2.0-only` devido às cláusulas de patente de versão 2.0.
   - Nas crates de **`Aladdin`** (`Cargo.toml`: `MIT OR Apache-2.0`) e de **`jhcs-rs`** vinculadas por `apps/jhcs-daemon` (`jhcs-core`, `jhcs-broker`, `jhcs-backend-cpu`, `jhcs-backend-simd`, `jhcs-platform-jinn`, todas com `license = "MIT OR Apache-2.0"` nos seus `Cargo.toml`), a vinculação dentro de binários **`GPL-2.0-only`** utiliza legitimamente a disjunção **`MIT`**, mantendo conformidade integral.
   - **Nota de Higiene Descoberta na Auditoria:** Enquanto o manifesto raiz `jhcs-rs/LICENSE` declara exclusivamente **`Apache-2.0`** (em alinhamento ao contrato comunitário preferencial para host/cloud/GPU), os manifestos de subcrates `crates/*/Cargo.toml` ainda preservam `MIT OR Apache-2.0` (ou `Apache-2.0 OR MIT` em `jhcs-backend-wgpu`). Recomenda-se preservar explicitamente `MIT OR Apache-2.0` nas crates `#![no_std]` compartilhadas com o binário `GPL-2.0-only` `jhcs-daemon` ou registrar a exceção dupla documentada no `LICENSING_POLICY.md`.
3. **Dependências Opcionais de Host GPU (`jhcs-backend-wgpu/Cargo.toml`):**
   - `wgpu = "23"`, `pollster = "0.4"` e `bytemuck = "1.16"` são estritamente opcionais sob a feature `std` (desabilitada em builds bare-metal `default-features = false` do Jinn OS) e possuem licenças permissivas (`MIT OR Apache-2.0` / `Zlib`), sem dependências copyleft.

---

## 3. Governança de Contribuições Externas (`CLA`, `SPDX` e `DCO`)

Para proteger a viabilidade do duplo licenciamento (`GPL-2.0-only OR Jinn-Commercial-1.0`) e oferecer segurança jurídica a adotantes corporativos:

1. **CLA Obrigatório (`CLA.md` + `.github/workflows/cla-check.yml`):**
   - Todo colaborador externo em `Jinn_Project` deve aceitar o **Contributor License Agreement v1.0** antes do merge de qualquer Pull Request (`cla-assistant`, registrando assinaturas na branch dedicada `cla-signatures`).
   - O CLA preserva a autoria moral do contribuidor e concede ao mantenedor direito não exclusivo, mundial e irrevogável para distribuir tanto sob `GPL-2.0-only` quanto sob `Jinn-Commercial-1.0`.
   - Materiais não destinados a incorporação devem trazer explicitamente *"NÃO É UMA CONTRIBUIÇÃO"*.
2. **Verificação Automatizada de Cabeçalhos SPDX (`.github/workflows/spdx-license-check.yml`):**
   - Arquivos `.rs`, `.c`, `.h`, `.asm`, `.s`, `.S`, `.py` e `.sh` adicionados ou modificados em PRs para `main` em `Jinn_Project` devem conter nas primeiras 10 linhas:
     `SPDX-License-Identifier: GPL-2.0-only OR Jinn-Commercial-1.0`
   - Arquivos históricos pré-existentes anteriores à política de cabeçalho dual mantêm rastreabilidade por histórico git, sem relicenciamento retroativo sem autorização dos titulares (`LICENSE`).
3. **Disciplina de Origem / DCO e Isolamento em Camadas (`CONTRIBUTING.md`):**
   - **L0 (`src/` Core):** Vedadas dependências GPL/LGPL de terceiros; código próprio ou domínio público/CC0.
   - **L1–L3 (Serviços e Drivers Ring 3):** Somente bibliotecas permissivas (`MIT`, `Apache-2.0`, `BSD-2/3-Clause`), isoladas por IPC e Syscall ABI; proibido mover código entre anéis para contornar licenças.

---

## 4. Limitações Jurídicas Conhecidas (Transparência para Auditoria)

1. A licença comercial **`Jinn-Commercial-1.0`** encontra-se em texto de minuta preliminar em `LICENSES/Jinn-Commercial-1.0.txt` e requer formalização contratual específica por cliente OEM com assessoria jurídica especializada antes de contratos comerciais em larga escala.
2. O registro formal de marcas (`INPI` / USPTO) e contratos de indenização de patentes (*IP Indemnity*) não estão protocolados neste estágio Alpha v0.0.3.
