# SIMBAD ADMINISTRATION CLI SPECIFICATION (`jinnctl`) 0.1.0
## Manual de Operação e Referência Completa da Ferramenta de Controle `jinnctl`

> **Documento:** Manual de Referência e Especificação da CLI  
> **Comando:** `jinnctl`  
> **Sistema:** Simbad Server 0.1.0 & Jinn Microkernel Ecosystem  
> **Classificação:** Ferramenta de Administração e Observabilidade do Sistema  
> **Data:** Novembro de 2026

---

## 1. Visão Geral da Interface `jinnctl`

O utilitário **`jinnctl`** é a interface primária de controle, gerenciamento de serviços e inspeção de telemetria do **Simbad Server**.

Ele opera como uma ferramenta de espaço de usuário (Ring 3) e comando integrado ao shell do sistema, comunicando-se com o [Service Supervisor](file:///e:/Projetos/Jinn/docs/SIMBAD_SERVICE_SUPERVISOR.md) e consultando diretamente as estruturas canônicas do microkernel sem privilégios perigosos.

```mermaid
flowchart LR
    ADMIN["Administrador / Script de Automação"] -->|"jinnctl <subcomando>"| CLI["jinnctl CLI Engine"]
    CLI -->|"supervisor.query() / start() / stop()"| SUP["Service Supervisor (Ring 3)"]
    CLI -->|"sys_sysinfo / sys_meminfo / sys_statvfs"| KERN["Jinn Microkernel (Ring 0)"]
    SUP --> OUT["Saída Estruturada / Tabela Formatada"]
    KERN --> OUT
```

---

## 2. Mapa Geral de Subcomandos

| Subcomando | Sintaxe | Descrição Operacional |
|---|---|---|
| **`status`** | `jinnctl status` | Visão geral do servidor, uptime, perfil ativo, CPUs, memória e serviços. |
| **`services`** | `jinnctl services` | Tabela detalhada de todos os daemons registrados, PID, estado e reinícios. |
| **`start`** | `jinnctl start <serviço>` | Inicia um daemon verificando suas dependências topológicas (DAG). |
| **`stop`** | `jinnctl stop <serviço>` | Encerra um daemon após validar que nenhum serviço ativo depende dele. |
| **`restart`** | `jinnctl restart <serviço>` | Reinicia um daemon de forma ordenada. |
| **`logs`** | `jinnctl logs` | Exibe as últimas 32 entradas do registro estruturado de eventos do servidor. |
| **`memory`** | `jinnctl memory` | Relatório minucioso de quadros físicos (RAM), heap e memória por processo. |
| **`storage`** | `jinnctl storage [--flush]` | Estatísticas de discos, partições, volumes montados e flushes de cache. |
| **`network`** | `jinnctl network` | Telemetria de adaptadores de rede, IP, gateway, DNS e firewall JinnWall. |
| **`profile`** | `jinnctl profile [simbad\|aladdin\|ifrit]` | Consulta ou comuta dinamicamente o perfil ativo do sistema. |

---

## 3. Especificação Detalhada e Exemplos de Saída

### 3.1. `jinnctl status`
Exibe o sumário executivo da saúde e carga do servidor:

```text
================================================================================
  jinnctl: Estado Geral do Servidor Simbad / Jinn OS
================================================================================
  Perfil Ativo       : Simbad (Server OS)
  Modo Operacional   : Headless (Servidor / Sem GUI)
  Scheduler Quantum  : 50 ms / timeslice
  Tempo de Atividade : 124 s (1240 ticks)
  Nucleos Online     : 4 cpus
  Servicos Ativos    : 4 em execucao, 0 falhas (Total: 4)
  Perfil Storage JSA : Simbad (Servidor)
  Firewall JinnWall  : PublicSecure (SPI Ativo)
```

### 3.2. `jinnctl services`
Lista todos os daemons supervisionados em formato tabular:

```text
SERVICO        ESTADO       PID    RESTARTS  POLITICA    DESCRICAO
--------------------------------------------------------------------------------
jinn-logd      RUNNING      11     0         always      Structured Logging & Telemetry Daemon
jinn-flusher   RUNNING      12     0         always      Storage Cache Synchronization Daemon (5s sync)
jinn-netmgr    RUNNING      13     0         always      Network Address & Link Manager (DHCP/DNS)
jinn-httpd     RUNNING      14     0         on-failure  Micro HTTP/1.1 Metrics & Healthcheck Daemon
```

### 3.3. `jinnctl start <serviço>` e `jinnctl stop <serviço>`
Comandos de gerenciamento com imposição estrita de integridade:

```bash
# Início de serviço
$ jinnctl start jinn-httpd
[OK] Servico 'jinn-httpd' iniciado com sucesso.

# Tentativa de iniciar sem dependência (exemplo: jinn-netmgr parado)
$ jinnctl start jinn-httpd
[ERRO] Dependency not satisfied (must be RUNNING)

# Tentativa de parar dependência com consumidor ativo
$ jinnctl stop jinn-logd
[ERRO] Cannot stop: active service depends on this service
```

### 3.4. `jinnctl logs`
Apresenta o fluxo de eventos com carimbo de tempo em segundos decorridos:

```text
REGISTRO DE EVENTOS E TELEMETRIA DO SERVIDOR
--------------------------------------------------------------------------------
[1s] INFO [SUPERVISOR] Simbad Service Supervisor initialized with 4 services
[1s] INFO [SUPERVISOR] Service started: jinn-logd
[1s] INFO [SUPERVISOR] Service started: jinn-flusher
[2s] INFO [SUPERVISOR] Service started: jinn-netmgr
[2s] INFO [SUPERVISOR] Service started: jinn-httpd
[7s] INFO [FLUSHER] Dirty cache flushed to disk
```

### 3.5. `jinnctl storage`
Inspeciona os dispositivos de bloco e permite disparo manual de sincronização:

```bash
$ jinnctl storage
# Exibe dispositivos de bloco, volumes FAT32 montados e métricas do flusher.

$ jinnctl storage --flush
[OK] Sincronizacao forcada executada: 12 setores gravados no disco fisico.
```

---

## 4. Integração Multiplataforma

A CLI `jinnctl` opera com paridade idêntica em dois ambientes:
1. **Console Serial / Headless VGA:** Através do comando nativo do kernel shell Ring 0.
2. **Terminal Gráfico do Aladdin Desktop:** Através do despachante `execute_string_command`, permitindo que desenvolvedores e administradores inspecionem os serviços de servidor mesmo durante sessões com interface gráfica.

---

## 5. Conclusão

A ferramenta **`jinnctl`** estabelece uma interface de observabilidade moderna, estruturada e de fácil automatização, fornecendo aos operadores visibilidade completa do Simbad Server.
