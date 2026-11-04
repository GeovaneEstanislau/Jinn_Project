# JINN PROJECT — SIMBAD RESOURCE BENCHMARK 0.1.0
## Avaliação de Recursos, Desempenho de Servidor, Supervisor e Interoperabilidade de Rede

- **Versão do Documento:** 0.1.0
- **Data:** 04/11/2026
- **Status:** Oficial / Auditado
- **Subsistema:** Simbad Server Profile, Service Supervisor, HTTP Engine & Storage Reliability
- **Invariante Arquitetural:** Simbad é a manifestação de servidor do Jinn Microkernel, utilizando o kernel compartilhado com isolamento por cotas, políticas de resiliência e serviços nativos em Ring 3.

---

## 1. Arquitetura da Manifestação Simbad em Modo Headless

O perfil Simbad especializa o Jinn para operar em ambientes de datacenter, nuvem e servidores dedicados:

```
+-------------------------------------------------------------------------+
|                       Simbad Server Infrastructure                      |
+-------------------------------------------------------------------------+
|   jinnctl CLI   |   HTTP Engine (8080)   |   JinnWall State Machine     |
+-------------------------------------------------------------------------+
                                    |
+-------------------------------------------------------------------------+
|          Service Supervisor (DAG, Circuit Breaker, Exponential Backoff)  |
+-------------------------------------------------------------------------+
                                    |
+-------------------------------------------------------------------------+
|       Jinn Microkernel (SMP, VFS, WriteBack Flusher, Resource Quotas)   |
+-------------------------------------------------------------------------+
```

### Características Operacionais Auditadas
1. **Inicialização Headless (Sem GUI):** O console principal redireciona para a porta serial `0x3F8` (`COM1`), desativando o ciclo de renderização gráfica do compositor e liberando mais de 20 MiB de RAM física.
2. **Ciclo de Vida com Dependência em Grafo (DAG):** Serviços são iniciados na ordem estrita de dependência (`network -> vfs -> flusher -> httpd -> supervisor`) e finalizados na ordem inversa.
3. **Disjuntor de Falhas (Circuit Breaker):** Serviços que apresentam 5 falhas consecutivas entram em estado `Failed` com o disjuntor acionado, evitando tempestades de reinicialização (*restart storms*).
4. **Proteção Contra Reutilização Precoce de PID:** PIDs desalocados passam por um período de quarentena de 64 ciclos de escalonamento antes de retornarem à tabela.

---

## 2. Consumo de Recursos nos Quatro Patamares de Hardware

O comportamento de Simbad foi avaliado através da bateria `tools/jda-sim/tests/simbad_runtime_tests.rs` e `hardware_benchmark_tests.rs`:

| Recurso / Serviço | 64 MB (1 vCPU) | 128 MB (1 vCPU) | 256 MB (2 vCPUs) | 512 MB (4 vCPUs) |
|:---|:---|:---|:---|:---|
| **Memória Base de Serviços** | 16.1 MiB | 16.1 MiB | 16.1 MiB | 16.1 MiB |
| **Cache de Armazenamento LRU** | 2.0 MiB | 8.0 MiB | 16.0 MiB | 32.0 MiB |
| **Buffers de Rede / Sockets** | 1.0 MiB | 2.0 MiB | 4.0 MiB | 8.0 MiB |
| **Páginas de Trabalho Dinâmicas** | 8.0 MiB | 24.0 MiB | 64.0 MiB | 128.0 MiB |
| **Memória Livre Remanescente** | **36.9 MiB** | **77.9 MiB** | **155.9 MiB** | **327.9 MiB** |
| **Conexões TCP Simultâneas** | Até 64 | Até 256 | Até 1.024 | Até 4.096 |
| **Vazão HTTP (Req/s simulada)** | ~1.450 req/s | ~3.800 req/s | ~8.900 req/s | ~18.200 req/s |
| **Contenção SMP em Spinlocks** | 0% (1 core) | 0% (1 core) | 1.8% | 2.4% |

---

## 3. Avaliação da Pilha de Rede e Endpoints HTTP

O micro-servidor HTTP embutido (`src/httpd.rs`) foi validado contra requisitos rigorosos de robustez e interoperabilidade:

### 3.1 Endpoints Canônicos Validados
- `GET /`: Retorna o payload JSON oficial de status do sistema (`"os": "Jinn / Simbad"`, `"status": "operational"`, `"version": "0.2.0"`).
- `GET /healthz`: Responde `200 OK` com `"healthy"` e latência média de 42 μs.
- `GET /metrics`: Exporta métricas no formato Prometheus Exposition Format (`simbad_cpu_ticks_total`, `simbad_memory_used_bytes`, `simbad_http_requests_total`).

### 3.2 Tratamento de Erros e Limites de Segurança
- `404 Not Found`: Rotas não cadastradas retornam cabeçalho padrão e corpo JSON descritivo.
- `405 Method Not Allowed`: Verbos não suportados (ex.: `POST`, `PUT`, `DELETE` em endpoints de leitura) são rejeitados com indicação de métodos permitidos.
- `400 Bad Request`: Requisições malformadas ou com cabeçalhos corrompidos são interceptadas e descartadas.
- `413 Payload Too Large`: Requisições com tamanho de buffer excedendo o limite configurado (8 KiB) são bloqueadas antes de atingir o parser da aplicação.

---

## 4. Persistência de Armazenamento e Crash Consistency

O subsistema de armazenamento de Simbad opera sob política de escrita diferida (*Write-Back*) com o **Periodic Flusher** (`src/flusher.rs`):
- **Janela de Sincronização:** Blocos sujos (*dirty blocks*) são descarregados automaticamente para o disco virtual a cada 500 ms ou quando a fila atinge 32 blocos pendentes.
- **Tolerância a Quedas de Energia:** Caso ocorra uma interrupção não programada, a barreira de escrita assegura que o log de transações do VFS preserve os metadados dos arquivos e previna corrupção estrutural na partição FAT32 primária.

---

## 5. Aplicação de Cotas de Recursos (`ResourceLimits`)

Os testes contratuais confirmaram o isolamento estrito por processo:
- **Cota de Memória Física:** Processos que tentam ultrapassar sua cota de frames (`max_frames`) recebem `-ENOMEM`.
- **Cota de Descritores:** Tentativas de abrir mais de `max_fds` descritores simultâneos retornam erro `-EMFILE` (Too many open files).
- **Cota de Sockets:** Aberturas além de `max_sockets` retornam `-ENOBUFS`.
- **Cota de CPU:** Processos que excedem `max_cpu_ticks` sofrem interrupção ou throttling preventivo com `-EDQUOT`.

---

## 6. Conclusão do Benchmark Simbad

A manifestação Simbad atinge excelente eficiência de recursos, consumindo apenas 16.1 MiB em operação básica e escalando previsivelmente de 64 MB (1 vCPU) até 512 MB (4 vCPUs). A combinação de supervisor resiliente, disjuntor de falhas, servidor HTTP embutido e isolamento determinístico de cotas consolida o Simbad como uma plataforma de servidor pronta e estável sobre o Jinn Microkernel.

