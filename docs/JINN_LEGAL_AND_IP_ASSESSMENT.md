# ⚖️ Jinn Project — Avaliação Jurídica, Propriedade Intelectual e Licenciamento
## Análise de Licenciamento Duplo, Titularidade de Código, Marcas, Patentes e Riscos Regulatórios

> **Documento Oficial de Engenharia do Jinn Project — Documento 5 de 11**  
> **Data de Referência:** 16 de Novembro de 2026  
> **Status:** Concluído e Auditado | **Classificação:** Parecer Técnico e Análise Preliminar de IP  
> **Aviso Legal:** *Este documento constitui uma análise técnica preliminar de engenharia e governança de propriedade intelectual realizada no âmbito do projeto. Não substitui parecer formal emitido por advogado inscrito na Ordem dos Advogados do Brasil (OAB) ou equivalente internacional.*  

---

## 1. Inventário de Licenças Atuais no Ecossistema

A auditoria dos arquivos `Cargo.toml`, cabeçalhos SPDX e documentações em `LICENSES/` revelou a seguinte configuração de licenças entre os repositórios:

| Repositório / Componente | Licença Comunitária Primária | Licença Comercial / Secundária | Mecanismo de Governança | Observações e Riscos de Conformidade |
| :--- | :--- | :--- | :--- | :--- |
| **Microkernel Jinn OS** (`D:\Projetos\Jinn`) | `GPL-2.0-only` | `Jinn-Commercial-1.0` (Minuta em `LICENSES/`) | Dual-Licensing pretendido via aviso SPDX nos arquivos | O arquivo `LICENSE` declara o modelo pretendido, mas ressalva que commits históricos foram publicados sob condições anteriores. |
| **Aladdin Desktop** (`D:\Projetos\Aladdin`) | `MIT OR Apache-2.0` | N/A (Permissiva Dual) | Declarada em `Cargo.toml` (`license = "MIT OR Apache-2.0"`) | Licença altamente permissiva. Permite integração comercial direta por terceiros sem obrigação de abertura de código derivado. |
| **JHCS Middleware** (`D:\Projetos\jhcs-rs`) | `Apache-2.0` | `JHCS Commercial License` (Proposta em `docs/`) | Dual-Licensing estruturado com CLA e exceções comerciais | Licença base Apache-2.0 viabiliza adoção ampla pela comunidade e ecossistema Rust; versão comercial concede direito de fechamento. |
| **Dependências Rust (Kernel)** | N/A (Zero dependências) | N/A | Compilação puramente nativa (`#![no_std]`, `core`, `alloc`) | Ausência de risco de contaminação por bibliotecas de terceiros no Ring 0. |

---

## 2. Análise Jurídica do Licenciamento Duplo (Dual Licensing)

O modelo de licenciamento duplo (*Commercial Dual-Licensing*) é uma estratégia consagrada no mercado de software de infraestrutura (utilizada historicamente por MySQL, Qt, MongoDB e SQLite). No entanto, sua eficácia depende do cumprimento estrito de pré-requisitos jurídicos fundamentais.

### 2.1. Limites do Copyleft: GPLv2 vs. AGPLv3 vs. Licenças Permissivas

1. **GPLv2 (`GPL-2.0-only` - Jinn OS):**
   * **Alcance:** Exige que qualquer pessoa ou organização que **distribua** binários do sistema operacional para terceiros disponibilize o código-fonte integral sob a mesma licença.
   * **Limitação Estratégica:** A GPLv2 não possui a cláusula de "interação via rede" (*Network Copyleft*). Se uma empresa utilizar o Jinn OS internamente em seus servidores ou disponibilizar serviços de computação em nuvem executando sobre o kernel sem distribuir o binário do sistema operacional, **não há obrigação jurídica de abrir as modificações do kernel ou os serviços proprietários executados em cima dele**.
2. **AGPLv3 (GNU Affero General Public License v3):**
   * **Mecanismo:** Fecha a brecha do SaaS/Cloud (*SaaS Loophole*), estabelecendo que a oferta de serviços através de uma rede de computadores equivale à distribuição, exigindo a disponibilização do código-fonte para os usuários remotos.
   * **Cuidado na Interpretação:** A AGPLv3 **não obriga automaticamente** uma empresa a abrir o código de aplicações de terceiros que apenas se comuniquem com o kernel através de chamadas de sistema (Syscalls) ou IPC padronizado, pois a fronteira de processo com isolamento de memória e chamadas documentadas é amplamente reconhecida como limite de obra derivada, a menos que haja vinculação estática ou acoplamento funcional direto.
3. **Apache 2.0 / MIT (Aladdin e JHCS Core):**
   * A adoção de licença permissiva no Aladdin e no JHCS favorece a criação de ecossistema e atrai desenvolvedores. No entanto, se uma corporação integrar o JHCS diretamente em seus produtos proprietários para Windows ou Linux, ela não precisará pagar royalties ao projeto Jinn, a menos que utilize uma versão que contenha componentes exclusivos da edição comercial ou esteja sujeita a um acordo comercial bilateral.

---

## 3. Titularidade do Código e Acordo de Colaboradores (CLA)

Para que o detentor do projeto possa exercer legalmente o modelo de licenciamento duplo e comercializar licenças proprietárias fechadas de um código que possui uma versão de código aberto, é **indispensável a titularidade exclusiva de 100% dos direitos patrimoniais de autor** sobre a obra.

### 3.1. Risco de Contribuições Externas sem CLA

* Se um desenvolvedor externo enviar um Pull Request com correções ou novas funcionalidades e o código for incorporado ao repositório sob a licença GPLv2 ou Apache-2.0 sem a assinatura de um **Contributor License Agreement (CLA)** com cessão de direitos autorais ou concessão de licença ampla de relicenciamento, esse colaborador passa a ser coautor da obra.
* **Consequência Jurídica:** O mantenedor do Jinn OS **não poderá comercializar a versão comercial** contendo o código desse colaborador sem o consentimento formal e a partilha de eventuais receitas, sob risco de infração de direitos autorais (Lei Federal nº 9.610/1998 no Brasil e *Copyright Act* nos EUA).
* **Situação Atual do Repositório:** O repositório Jinn possui a branch `cla-signatures` e o workflow `.github/workflows/cla-check.yml` configurado para verificar assinaturas de colaboradores. Essa política preventiva deve ser mantida estritamente ativa para todas as contribuições públicas.

---

## 4. Marcas, Patentes e Propriedade Intelectual

### 4.1. Proteção de Marcas Registradas

Os nomes comerciais e logotipos do ecossistema constituem o ativo intangível de mais rápida proteção e maior valor comercial inicial:
* **Termos de Destaque:** `Jinn`, `Jinn OS`, `Aladdin Desktop`, `JHCS (Jinn Heterogeneous Compute Service)`, `Simbad`, `Ifrit`.
* **Registro de Marcas no Brasil (INPI):** Deve ser protocolado pedido de registro perante o Instituto Nacional da Propriedade Industrial (INPI) nas classes correspondentes da Classificação Internacional de Nice (NCL):
  * **Classe 9:** Softwares de sistema operacional, interfaces gráficas, programas de computador gravados ou baixáveis, middlewares de computação.
  * **Classe 42:** Serviços de tecnologia da informação, desenvolvimento, manutenção e atualização de sistemas operacionais, consultoria em computação de alto desempenho.
* **Busca de Anterioridade:** Recomenda-se a realização prévia de busca de anterioridade marcária para mitigar colidência com marcas registradas preexistentes na área de software e telecomunicações.

---

### 4.2. Patenteabilidade de Software e Arquiteturas

A proteção por patentes para invenções implementadas por programa de computador varia substancialmente entre as jurisdições:

1. **Legislação Brasileira (Lei nº 9.279/1996 e Diretrizes do INPI):**
   * O Artigo 10, inciso V, da Lei de Propriedade Industrial estabelece explicitamente que programas de computador em si (*software per se*) não são considerados invenção nem modelo de utilidade.
   * **Requisito para Patenteabilidade:** Para ser patenteável no Brasil, o método deve configurar uma **Invenção Implementada por Computador (IIC)**, onde o software produz um **efeito técnico tangível** que resolve um problema técnico concreto do hardware (por exemplo: método de gestão de filas de hardware para prevenção de corrupção em DMA com quarentena automática de barramento, ou método físico de despacho adaptativo com comutação de registradores SIMD em tempo real).
2. **Jurisdição Internacional (USPTO - Estados Unidos e EPO - Europa):**
   * Nos Estados Unidos (USPTO), métodos computacionais com aplicação industrial e transformação técnica concreta podem ser patenteáveis (*Utility Patents* sob 35 U.S.C. § 101), sujeitos às restrições do precedente *Alice Corp. v. CLS Bank*.
   * Na Europa (EPO), patentes de software exigem comprovação de "caráter técnico" (*technical character* sob o Artigo 52 da Convenção Europeia de Patentes).
3. **Recomendação Estratégica:** Em vez de focar em patentes genéricas de algoritmos de agendamento (que demandam custos de dezenas de milhares de dólares e anos de tramitação com alto risco de indeferimento), o projeto deve focar em:
   * **Registro de Programa de Computador no INPI:** Proteção do código-fonte como direito autoral (rápido, de baixo custo e com validade internacional de 50 anos pela Convenção de Berna).
   * **Segredo de Negócio (*Trade Secret*):** Preservar os parâmetros empíricos calibrados de despacho de hardware e telemetria fina dentro dos módulos da edição comercial fechada.

---

## 5. Auditoria da Proposta "Jinn Open Economy" (Tokens e Criptoativos)

Os documentos presentes em `docs/adr/` descrevem propostas de uma economia descentralizada baseada em tokens de governança e utilidade (*Utility Token* / PoUC - Proof of Useful Contribution) para remuneração de recursos computacionais compartilhados e integração com exchanges (como Binance):

### 5.1. Riscos Regulatórios e de Valores Mobiliários (Securities)

1. **Enquadramento pela CVM (Comissão de Valores Mobiliários - Brasil):**
   * Se um token for ofertado ao público com a promessa direta ou implícita de valorização financeira decorrente do esforço de desenvolvimento do ecossistema pelo mantenedor ou pela comunidade, ele se enquadra no **Teste de Howey** e no conceito de valor mobiliário (Contrato de Investimento Coletivo - CIC, previsto na Lei nº 6.385/1976 e no Parecer de Orientação CVM nº 40/2022).
   * A emissão ou distribuição pública de tokens considerados valores mobiliários sem registro ou dispensa perante a CVM constitui infração administrativa gravíssima e crime contra o sistema financeiro nacional (Lei nº 7.492/1986).
2. **Enquadramento pela SEC (Estados Unidos) e MiCA (União Europeia):**
   * A Securities and Exchange Commission (SEC) aplica o Teste de Howey com extremo rigor a redes que dependem de um grupo central de desenvolvedores para seu sucesso.
   * O regulamento europeu *Markets in Crypto-Assets* (MiCA) impõe requisitos severos de transparência, capital próprio e emissão de *Whitepapers* auditados para emissores de criptoativos.

### 5.2. Riscos Tributários e Fiscais

* A comercialização de tokens ou recebimento de valores em criptomoedas gera fatos geradores tributários imediatos perante a Receita Federal do Brasil (IRPJ, CSLL, PIS/COFINS e Instrução Normativa RFB nº 1.888/2019 de declaração compulsória de operações com criptoativos).

### 5.3. Recomendação de Governança Econômica

* **Desacoplamento Completo da Engenharia de Base:** O desenvolvimento tecnológico do microkernel Jinn OS, do ambiente gráfico Aladdin e do middleware JHCS **não deve depender** do lançamento ou da existência de um token criptográfico.
* A viabilidade do Jinn como produto de *deep tech* repousa sobre suas características de desempenho, confiabilidade e segurança técnica, e não sobre instrumentos financeiros especulativos. Qualquer exploração de economia comunitária ou tokens deve ser formalmente postergada para após a consolidação técnica e após consulta a bancas jurídicas especializadas em regulação de mercados de capitais.
