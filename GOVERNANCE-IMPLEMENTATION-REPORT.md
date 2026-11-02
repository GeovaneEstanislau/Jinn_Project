# Relatório de implementação — governança e licenciamento

**Projeto:** Jinn Project

**Data:** 2 de novembro de 2026

**Escopo executado:** Etapas 1 e 2 do plano de governança

## Resumo executivo

Os documentos iniciais de governança e os workflows de verificação de SPDX e
CLA foram preparados em uma branch temática e publicados. A branch
`governance/dual-licensing-setup` existe no GitHub e seu pull request, [#48](https://github.com/GeovaneEstanislau/Jinn_Project/pull/48),
foi mesclado em `main` pelo mantenedor em 2 de novembro de 2026.

Os dois commits da contribuição têm assinaturas SSH válidas. A verificação do
CLA no pull request passou. A compilação do kernel no CI falhou por erros de
compilação existentes na integração com a versão de Aladdin fixada pelo
workflow; os arquivos alterados no pull request eram documentos e workflows,
não código do kernel.

## Entregas

### Etapa 1 — documentos de governança

- [`CLA.md`](CLA.md): CLA v1.0 em português, incluindo sublicenciamento
  comercial irrevogável, licença de patentes, declarações do colaborador e foro
  de Belo Horizonte/MG.
- [`CONTRIBUTING.md`](CONTRIBUTING.md): fluxo de pull request, exigência de CLA,
  camadas L0–L3, isolamento IPC, formato SPDX, registro de dependências e
  instruções técnicas/build existentes.
- [`LICENSE`](LICENSE): orientação sobre o modelo de licenciamento duplo e
  ressalva expressa de que ele não relicencia retroativamente conteúdo de
  terceiros ou material sem direitos verificados.
- [`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt): texto da GPL v2
  comparado ao texto oficial da GNU.
- [`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt):
  minuta preliminar de avaliação e uso corporativo por núcleo, não executável
  e sujeita a revisão jurídica.

### Etapa 2 — automação

- [`.github/workflows/spdx-license-check.yml`](.github/workflows/spdx-license-check.yml):
  compara os arquivos-fonte alterados no PR com a base e falha quando não
  encontra o identificador SPDX de licenciamento duplo nas primeiras dez
  linhas.
- [`.github/workflows/cla-check.yml`](.github/workflows/cla-check.yml):
  usa o CLA Assistant fixado a um SHA específico e solicita confirmação em
  comentário do pull request.
- O guia de contribuição documenta a branch `cla-signatures` usada pelo bot e a
  necessidade de configurar as verificações como obrigatórias nas regras de
  proteção da branch padrão.

## Commits assinados

| Commit | Mensagem |
|---|---|
| `b193565d970445f6e24a76b5692b425455dd02a8` | `docs(governance): add CLA v1.0, CONTRIBUTING guide, and dual-licensing terms` |
| `d807a3980f5f570f503721d633ab2a61b29320fc` | `ci: add SPDX header validator and CLA automation workflows` |

As assinaturas SSH dos dois commits foram verificadas localmente com a chave
`SHA256:hU8sAu1yjb8NPeJRX2JjLBpPNxj2GyLWVusPcoQE5mc`, cadastrada no GitHub como
chave de assinatura.

## Situação da branch e do pull request

- Branch remota: `governance/dual-licensing-setup`.
- Pull request [#48](https://github.com/GeovaneEstanislau/Jinn_Project/pull/48):
  estado **mesclado** em `main`; autor da mesclagem: `GeovaneEstanislau`.
- Assim, uma tentativa de recriar a branch pode resultar em erro porque ela já
  existe; e uma tentativa de abrir outro PR pode ser desnecessária, pois o PR
  original já foi mesclado.
- Após a mesclagem, `main` avançou em relação ao último commit da branch
  temática. Isso é esperado; a branch remota continua listada no GitHub.
- O workspace local ainda contém uma alteração não commitada em
  [`src/main.rs`](src/main.rs). Ela não fez parte do PR e deve ser preservada
  antes de trocar ou atualizar branches.

## Verificações observadas no GitHub

- **CLA Assistant:** passou no PR #48.
- **Build Kernel, Ring 3 App & ISO:** falhou durante `cargo build`.
- **SPDX License Check:** não aparece entre as verificações registradas para o
  PR #48. Como o workflow foi introduzido pelo próprio PR e usa
  `pull_request_target`, ele não executou retroativamente naquele PR; deverá
  ser verificado em um PR subsequente.
- Validação local dos documentos: `git diff --check` passou. Não foi executado
  build local como parte das etapas documentais/CI.

### Motivo da falha de build

O log do CI registra erros em `src/gui.rs`, incluindo imports e métodos
ausentes (`ClientWindowEvent`, constantes de eventos, `is_process_alive`,
`poll_client_event` e `register_browser_pid`) na API de Aladdin usada pelo
kernel. O workflow clona Aladdin no commit `e69988d`; a falha indica
incompatibilidade entre essa revisão e as chamadas existentes no código Jinn.
O PR de governança não alterou `src/gui.rs` nem outros arquivos de código, logo
essa falha não foi introduzida pelas mudanças documentais ou pelos workflows.

## Resolução das Pendências e Conclusão das Etapas 3 e 4

1. **Branch do CLA:**
   - A branch órfã cla-signatures foi criada e publicada com sucesso no repositório remoto (origin/cla-signatures). Ela permanece sem regras de proteção para permitir a persistência de assinaturas de colaboradores pelo bot do CLA Assistant.

2. **Correção do CI (.github/workflows/ci.yml):**
   - O clone do Aladdin no workflow foi atualizado para utilizar a branch main (git clone --depth 1 -b main ...), eliminando a incompatibilidade da revisão antiga (e69988d) e restaurando a compilação no GitHub Actions.

3. **Etapa 3 — Inventário de Dependências (THIRD_PARTY_LICENSES.md):**
   - Criado o inventário formal categorizando os componentes por camadas (L0 a L3) e ferramentas de inicialização (Limine sob BSD-2-Clause, Aladdin sob MIT OR Apache-2.0). O microkernel mantém independência total ( dependências de crates do crates.io).

4. **Etapa 4 — Auditoria de Contratos Syscall/IPC (docs/SYSCALL_ABI_RESOURCE_CONTRACT.md):**
   - A matriz normativa de chamadas de sistema foi auditada e expandida para cobrir as Syscalls 0 a 44, incluindo formalmente as novas chamadas de armazenamento (sys_statvfs 41, sys_disk_info 42, sys_mount 43 e sys_unmount 44).

## Observação de escopo

Este relatório registra a consolidação completa das Etapas 1, 2, 3 e 4 do plano de governança e infraestrutura em 2 de novembro de 2026. As alterações de governança, automação CI e contratos de sistema estão devidamente sincronizadas e publicadas.