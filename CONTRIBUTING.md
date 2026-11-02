# Contribuindo para o Jinn Project

Obrigado por considerar uma contribuição ao Jinn Project. Este guia descreve o
fluxo técnico e os requisitos de governança de propriedade intelectual.

## Antes de contribuir

1. Leia o [CLA](CLA.md), a [orientação de licença](LICENSE) e os avisos
   aplicáveis aos arquivos que pretende alterar.
2. Confirme que você tem autoridade para enviar o material e que a contribuição
   não incorpora código, documentação ou outros materiais de terceiros sem a
   identificação e autorização exigidas.
3. Se a contribuição estiver relacionada ao seu emprego ou a um contrato, obtenha
   previamente as autorizações previstas no CLA.

Todo colaborador deve assinar formalmente o **Contributor License Agreement
v1.0** antes da integração da contribuição. A assinatura concede os direitos
definidos no CLA, inclusive sublicenciamento comercial; ela não transfere a
titularidade dos direitos autorais. O workflow de CLA verifica a assinatura e
publica o resultado no pull request. A assinatura é feita pelo comentário
indicado pelo bot, usando a conta GitHub do colaborador.

Para a automação bloquear merges, os mantenedores devem configurar o status do
CLA e o workflow SPDX como verificações obrigatórias nas regras de proteção da
branch padrão. O workflow de CLA armazena as confirmações na branch
`cla-signatures`; antes de habilitá-lo, os mantenedores devem criar essa branch
e deixá-la sem proteção para que o bot possa gravar as confirmações. Não
armazene assinaturas na branch de código protegida.

## Fluxo de submissão

1. Abra uma issue para discutir mudanças amplas ou alterações de arquitetura.
2. Crie uma branch a partir da branch padrão e faça alterações focadas.
3. Execute as verificações pertinentes e inclua testes para mudanças de
   comportamento.
4. Abra um pull request com objetivo, contexto, testes realizados, camada
   arquitetural afetada e detalhes sobre dependências ou material de terceiros.
5. Atenda às revisões e aguarde a confirmação do CLA e dos cabeçalhos SPDX antes
   da integração. Não inclua segredos, dados pessoais ou conteúdo sem
   autorização no patch.

Issues, discussões ou outros envios que não sejam contribuições devem ser
identificados ostensivamente como **NÃO É UMA CONTRIBUIÇÃO**, conforme a
definição do CLA.

## Camadas e isolamento

As camadas são limites arquiteturais, não apenas diretórios. Novos módulos e
dependências devem indicar sua camada e respeitar os limites de comunicação:

- **L0 — Núcleo fundamental:** código próprio ou material em domínio público/
  CC0 cuja procedência tenha sido auditada. Não introduza dependências de
  terceiros sob GPL, LGPL ou outra licença copyleft nesta camada.
- **L1 — Servidores em userspace:** dependências de terceiros somente com
  licenças permissivas aprovadas (MIT, Apache-2.0, BSD-2-Clause ou
  BSD-3-Clause). Comunicação com L0 exclusivamente por IPC.
- **L2 — Drivers em userspace:** mesmas restrições de dependências de L1 e
  isolamento de L0 por IPC.
- **L3 — Aplicações e shell:** aplicações e utilitários do Alladin OS; respeite
  os contratos de userspace e registre as dependências e licenças.

Não exponha endereços de memória ou ponteiros crus do kernel através de
interfaces de L0/L1/L2. Use mensagens tipadas, handles/capabilities e buffers
com limites validados. Não mova código entre camadas para contornar uma
restrição de licença.

## Cabeçalhos SPDX

Todo arquivo-fonte próprio abrangido pela política deve começar com o aviso
abaixo, adaptando apenas o marcador de comentário à linguagem:

```text
SPDX-License-Identifier: GPL-2.0-only OR Jinn-Commercial-1.0
Copyright (c) 2026 Jinn Project Maintainers. All rights reserved.
```

Por exemplo, em Rust, C, cabeçalhos C, assembly com comentários `;`, Python ou
shell, cada linha deve usar a sintaxe de comentário aceita pela linguagem.
Preserve cabeçalhos de terceiros existentes; não substitua autoria ou licença
de terceiros pelo aviso do projeto. Arquivos gerados e conteúdo de terceiros
não devem receber o cabeçalho como se fossem código próprio. Informe no pull
request qualquer exceção necessária, sem omitir avisos já existentes.

`Jinn-Commercial-1.0` é o identificador da minuta comercial do projeto. Consulte
[`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt); não
interprete o cabeçalho isoladamente como confirmação de que o projeto detém
direitos sobre o arquivo.

## Dependências e material de terceiros

Antes de adicionar ou atualizar uma dependência:

- identifique o nome, versão, origem, licença e camada de uso;
- verifique o texto integral da licença, avisos de copyright e obrigações de
  redistribuição;
- confirme que a licença é permitida para a camada e que a integração respeita
  o limite de IPC;
- documente o componente para o catálogo de dependências do projeto.

Código de terceiros incluído no patch deve ser identificado no momento da
submissão, com titular original e cópia exata da licença, conforme o CLA. Não
submeta conteúdo cuja origem ou licença não possa ser verificada.

## Configurar, compilar e executar

O Jinn OS é um kernel bare-metal x86_64 escrito em Rust.

```powershell
rustup toolchain install nightly --component rust-src --component llvm-tools-preview
rustup target add x86_64-unknown-none --toolchain nightly
```

Na raiz do repositório:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run.ps1 -NoRun
```

O launcher pode baixar binários do Limine quando ainda não estiverem disponíveis
no clone. A criação da ISO requer `mkisofs`, `genisoimage` ou `xorriso`; no
Windows, `oscdimg` também é aceito. Para iniciar no QEMU, execute
`scripts\run.ps1` sem `-NoRun`.

## Estilo de código

- O kernel usa `#![no_std]`.
- Evite panics e alocações em interrupções.
- Acompanhe blocos `unsafe` com um comentário `// SAFETY:` que explique os
  invariantes relevantes.
- Inicialize buffers explicitamente; para zerá-los, use `.fill(0)` quando
  apropriado.
- Documente funções públicas com doc comments.
- Evite `loop {}` sem uma operação apropriada como `hlt` ou `yield`.
- Prefira mudanças focadas, alocação em pilha nos caminhos críticos e atomics
  quando forem apropriados e corretos.
