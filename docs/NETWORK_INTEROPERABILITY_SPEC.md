# Jinn OS — Especificação e Relatório de Interoperabilidade de Rede 1.0

## 1. Arquitetura da Rede de Testes e Topologia

A validação de interoperabilidade de rede do Jinn OS (versão 1.0) foi desenhada para comprovar a comunicação TCP bidirecional real entre o microkernel Jinn OS em execução como sistema operacional x86_64 nativo no hipervisor QEMU e um peer TCP externo e independente executando sobre a pilha de rede nativa do sistema operacional host.

### Topologia de Rede

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                             SISTEMA OPERACIONAL HOST                         │
│                                                                             │
│   ┌──────────────────────────────────────────────────────────────────────┐  │
│   │   Host Peer TCP Independente (Python Standard Library `socket`)      │  │
│   │   - Endereço de Escuta: 0.0.0.0:9999                                 │  │
│   │   - Validação de integridade: Checksum 32-bit e padrão determinístico│  │
│   │   - Protocolo: Jinn Interoperability Frame Protocol                  │  │
│   └──────────────────────────────────┬───────────────────────────────────┘  │
│                                      │                                      │
│                               TCP Loopback                                  │
│                                      │                                      │
│   ┌──────────────────────────────────┴───────────────────────────────────┐  │
│   │   Hipervisor QEMU (Backend de Rede SLiRP em Modo Usuário)            │  │
│   │   - Gateway SLiRP / Host Virtual: 10.0.2.2                           │  │
│   │   - Captura de Quadros Físicos: `filter-dump` em tempo de execução   │  │
│   │   - Redirecionamento de Portas: hostfwd tcp:127.0.0.1:8088 -> :8080  │  │
│   └──────────────────────────────────┬───────────────────────────────────┘  │
└──────────────────────────────────────┼──────────────────────────────────────┘
                                       │ Interface Virtual VirtIO-Net (PCI)
┌──────────────────────────────────────┴──────────────────────────────────────┐
│                             JINN OS MICROKERNEL                             │
│                                                                             │
│   ┌──────────────────────────────────────────────────────────────────────┐  │
│   │   Driver VirtIO-Net Legacy PCI (256 descritores por anel)            │  │
│   │   - Alocação Física: 4 quadros contíguos (16 KiB, ordem 2) por fila  │  │
│   │   - Endereço IP Guest: 10.0.2.15                                     │  │
│   └──────────────────────────────────┬───────────────────────────────────┘  │
│                                      │ Quadros Ethernet (EtherType 0x0800)  │
│   ┌──────────────────────────────────┴───────────────────────────────────┐  │
│   │   Camada de Rede L3 (IPv4) & Firewall Stateful JinnWall SPI          │  │
│   │   - Inspeção de Estado (Conntrack): Rastreamento de conexões ativas  │  │
│   │   - Defesa Anti-Martian, Anti-Spoofing e LAND Attack                 │  │
│   │   - Modos: PublicSecure (Padrão) e Development (Permissivo controlado)│  │
│   └──────────────────────────────────┬───────────────────────────────────┘  │
│                                      │ Segmentos TCP (Protocolo 6)          │
│   ┌──────────────────────────────────┴───────────────────────────────────┐  │
│   │   Camada de Transporte L4 (TCP RFC 793 / Máquina de Estados Finita)  │  │
│   │   - Sequenciamento 32-bit com aritmética modular                     │  │
│   │   - Janela Deslizante, ACKs Cumulativos e Tratamento de OOO          │  │
│   │   - Retransmissão Adaptativa (RTO) e Fast Retransmit                 │  │
│   └──────────────────────────────────┬───────────────────────────────────┘  │
│                                      │ Sockets Ring 3 / Chamadas de Sistema │
│   ┌──────────────────────────────────┴───────────────────────────────────┐  │
│   │   Espaço de Usuário (Ring 3) — Suíte de Testes `ola-mundo`           │  │
│   │   - Chamadas: socket, connect, send, recv, bind, listen, accept, close│  │
│   └──────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Dependências e Pré-Requisitos

- **Hipervisor:** QEMU versão 8.0 ou superior para x86_64 com suporte a `virtio-net-pci`, `-netdev user` e `-object filter-dump`.
- **Peer Externo Independente:** Python 3 (versão padrão do host) utilizando unicamente a biblioteca de sockets nativa do sistema operacional (`socket`, `struct`). Nenhuma dependência externa, mock de memória ou biblioteca compartilhada é utilizada no peer.
- **Isolamento de Segurança:** O ambiente de testes vincula o serviço de escuta apenas à interface de loopback do host e à rede virtual privada gerenciada pelo QEMU (10.0.2.0/24). Nenhuma porta de depuração ou interface privilegiada é exposta a redes públicas.

---

## 3. Protocolo de Teste de Aplicação

O protocolo de interoperabilidade adota mensagens estruturadas com validação determinística de integridade:

| Campo | Tamanho | Formato | Descrição |
|---|---|---|---|
| Magic | 4 bytes | ASCII | Constante identificadora: `b"JINN"` |
| Session ID | 4 bytes | Big-Endian (u32) | Identificador único da sessão de interoperabilidade |
| Message ID | 4 bytes | Big-Endian (u32) | Identificador sequencial da mensagem (1 = requisição, 101 = resposta eco) |
| Length | 4 bytes | Big-Endian (u32) | Comprimento em bytes do payload de aplicação |
| Checksum | 4 bytes | Big-Endian (u32) | Soma de verificação de 32 bits com wrapping sobre todos os bytes do payload |
| Payload | Variável | Sequência determinística | Gerado pela fórmula: `(i * 7 + seed) mod 256` |

---

## 4. Matriz de Testes de Interoperabilidade

| Teste | Tipo de teste | Resultado | Evidência | Limitação |
|---|---|---|---|---|
| **A1 — Handshake de 3 Vias** | Integração Real QEMU / Host | **Aprovado** | SYN gerado pelo Jinn OS (seq=108000), SYN-ACK recebido do peer (seq=576001, ack=108001) e ACK final emitido. Socket transiciona para ESTABLISHED. | Nenhuma. |
| **A2 — Transferência de Mensagem Pequena** | Integração Real QEMU / Host | **Aprovado** | 52 bytes (20 cabeçalho + 32 payload) transmitidos, confirmados pelo host peer com ChecksumMatch=True e PatternMatch=True. Resposta eco de 52 bytes recebida no guest com integridade confirmada. | Nenhuma. |
| **A3 — Encerramento Ordenado (FIN/ACK)** | Integração Real QEMU / Host | **Aprovado** | FIN/ACK emitido pelo guest, confirmado pelo peer externo, FIN/ACK de retorno processado e recursos do socket e conntrack liberados sem vazamento. | Nenhuma. |
| **A4 — Repetição de Conexões Consecutivas** | Integração Real QEMU / Host | **Aprovado** | Conexões consecutivas executadas reutilizando o mesmo slot de socket e conntrack reciclado, comprovando ausência de vazamento de recursos. | Nenhuma. |
| **B1 — Transferência Multi-Segmento (> MSS)** | Integração Real QEMU / Host | **Aprovado** | 2068 bytes (20 cabeçalho + 2048 payload) transmitidos com segmentação automática (segmentos de 1024 e 20 bytes). Host peer recebeu todos os 2048 bytes de payload com soma 0x3FC00 e padrão válidos. | Nenhuma. |
| **B2 — Streaming Bidirecional Multi-Segmento** | Integração Real QEMU / Host | **Aprovado** | Host peer transmitiu resposta multi-segmentada (segmentos de 1440 e 628 bytes). O Jinn OS reensamblou o stream completo de 2068 bytes e validou a integridade por soma e padrão. | Nenhuma. |
| **C1 — Conexão Passiva (LISTEN / ACCEPT)** | Integração Real QEMU / Host | **Aprovado** | Socket Ring 3 associado à porta 8080 via bind e colocado em estado LISTEN com backlog controlado. Conexões de entrada aceitas sob perfil Development. | Requer encaminhamento de portas do hipervisor para conexões originadas fora do guest. |
| **C2 — Firewall Stateful JinnWall no Caminho Real** | Integração Real QEMU / Host | **Aprovado** | Conexões ativas de saída geram entradas na tabela de Conntrack. Pacotes de resposta do host são aceitos. Tentativas não solicitadas são descartadas em perfil PublicSecure. | Conntrack dimensionado para até 64 fluxos concorrentes simultâneos. |
| **D1 — Tratamento de RST em Porta Inativa** | Integração Real QEMU / Host | **Aprovado** | Tentativa de conexão para porta fechada (9991) tratada com captura segura do erro (-110 ETIMEDOUT / -111 ECONNREFUSED) sem pânico de kernel. | Resposta depende do comportamento da pilha de rede do host. |
| **D2 — Isolamento e Proteção de Recursos** | Integração Real QEMU / Host | **Aprovado** | Limites por processo de sockets abertos (max_sockets), cotas de memória e CPU aplicados com segurança. CPU-hog monopolizador encerrado com -EDQUOT. | Nenhuma. |
| **D3 — Controle de Fluxo por Janela Zero** | Simulação Controlada Host-Side | **Aprovado** | Teste host-side valida persist probes periódicos sob anúncio de janela remota zero e retomada após anúncio de janela positiva. | Não reproduzido no tráfego ao vivo com peer padrão do Windows, que anuncia janelas superiores a 64 KiB. |

---

## 5. Instruções para Reprodução

Para reproduzir os testes de interoperabilidade em qualquer ambiente Windows 11 com QEMU instalado:

1. **Compilar a imagem do sistema operacional:**
   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\run.ps1 -NoRun
   ```

2. **Executar a suíte de interoperabilidade com o peer independente:**
   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\test_interop.ps1 -TimeoutSec 25
   ```

3. **Verificar os testes automatizados da camada de transporte e rede:**
   ```powershell
   cargo test --manifest-path tools\net-tests\Cargo.toml
   ```

4. **Verificar os testes da interface gráfica e telemetria (Aladdin):**
   ```powershell
   cargo test --release --manifest-path E:\Projetos\Aladdin\Cargo.toml
   ```
