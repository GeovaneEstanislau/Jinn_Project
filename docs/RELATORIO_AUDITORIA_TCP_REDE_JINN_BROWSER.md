# RELATÓRIO TÉCNICO: AUDITORIA DE SOCKETS TCP/IP E TRÁFEGO DE REDE NO JINN BROWSER

**Data de Execução:** 11 de Novembro de 2026  
**Sistema Operacional:** Jinn OS (Microkernel x86_64 com Aladdin Desktop Ring 3)  
**Módulos Auditados e Otimizados:**
- `src/net/tcp.rs` (Protocolo TCP RFC 793, buffer e realinhamento OOO, controle de fluxo de loopback)
- `src/net/socket.rs` (Stream Ring Buffer de 64 KiB, controle de janela dinâmica, emissão de Window Update ACKs)
- `apps/ola-mundo/src/browser.rs` (Decodificação determinística RFC 7230/9112 de Transfer-Encoding: chunked)
- `apps/ola-mundo/src/main.rs` (Loop de recepção HTTP, ampliação para 64 KiB, tolerância a latência e integridade HTML)
- `tests/tcp_socket_stress_tests.rs` (Suíte unitária de estresse com 16 cenários de rede e loopback)
- `jinn.iso` & `scripts/create_vbox_vm.ps1` (Geração de ISO inicializável e provisionamento para Oracle VirtualBox)

---

## 1. Resumo Executivo e Diagnóstico de Falha

Durante a navegação externa com o Jinn Browser, observou-se que páginas web carregavam frequentemente de forma incompleta, perdendo o corpo principal do documento HTML ou falhando com a mensagem:
`[BROWSER-NET] Timeout de recepcao atingido (3.0s, recebidos X bytes)`
mesmo após o handshake IP/ARP e a comunicação com a interface de rede (AMD PCnet / Intel PRO/1000) já estarem operacionais.

A investigação detalhada da pilha de rede (L4) e da camada de aplicação do navegador revelou **quatro causas raízes interdependentes**:

1. **Terminação Prematura por Heurística Falha de Chunked Transfer (`browser.rs`)**:
   - A função `has_chunked_terminator` pesquisava indiscriminadamente a sequência `0\r\n\r\n` ou `\r\n0\r\n\r\n` em qualquer posição da carga útil com `body.windows(5)`.
   - Se o documento HTML, script ou estilo CSS contivesse uma linha com o dígito zero ou sequência similar dentro dos dados válidos de um chunk, o navegador considerava a transferência terminada precocemente, interrompendo o `sys_recv` no meio da página. A tentativa de decodificação falhava e repassava pedaços brutos com bytes de chunk truncados.

2. **Bloqueio Permanente do Buffer Fora de Ordem TCP (`src/net/tcp.rs`)**:
   - O buffer fora de ordem (`ooo_buf`) armazenava no máximo 1536 bytes. Quando um segmento sobreposto ou parcial preenchia a lacuna de sequência, `remote_seq` avançava, mas o código não recalculava ou descartava o prefixo do `ooo_buf`.
   - Como consequência, `ooo_len` permanecia maior que zero para sempre, impedindo que qualquer novo pacote fora de ordem futuro fosse absorvido pelo socket durante toda a conexão TCP.

3. **Inanição de Janela e Ausência de Window Update ACK (`src/net/socket.rs`)**:
   - Quando o buffer de recepção ficava saturado por uma rajada de dados (`local_win = 0`), o servidor suspendia a transmissão aguardando liberação de janela (Zero Window Probe).
   - Quando o navegador consumia dados através de `sys_recv` (`pop_rx`), o kernel só emitia um pacote ACK de atualização de janela se `sock.tcb.local_win >= 2048`. Leituras menores (como as de parsing inicial de cabeçalho HTTP) deixavam a janela anunciada travada em 0, levando ao congelamento completo do tráfego.

4. **Timeout Rígido e Buffers Restritos de 32 KiB (`apps/ola-mundo/src/main.rs`)**:
   - O timeout de recepção de 3,0 segundos (600 spins) descartava sumariamente todo o documento já recebido nos buffers (`FetchError::RecvTimeout`), mesmo quando 10 KiB a 30 KiB de dados HTML válidos já estavam presentes.
   - Buffers de 32 KiB impediam o download de páginas web ricas que ultrapassassem esse limite.

---

## 2. Implementação das Correções Técnicas

### 2.1. Parser Chunked Determinístico RFC 7230/9112 (`apps/ola-mundo/src/browser.rs`)
Substituiu-se a busca ingênua de substrings pela função determinística [`is_chunked_complete`](file:///E:/Projetos/Jinn/apps/ola-mundo/src/browser.rs#L2628-L2705):
- O algoritmo percorre a sequência de chunks a partir do byte 0 do corpo HTTP: lê o tamanho hexadecimal do chunk, pula os delimitadores CRLF, salta exatamente `chunk_size` bytes da carga útil e só inspeciona o próximo cabeçalho no limite exato do chunk.
- Dados contendo `0\r\n\r\n` dentro da carga útil nunca geram falso término.
- O término só é assinalado quando um bloco de tamanho `0` for encontrado no limite estrutural do protocolo.
- A função suporta tanto slices contendo apenas o corpo quanto respostas completas com cabeçalho `HTTP/1.x`.

### 2.2. Reassentamento e Realinhamento OOO no Kernel (`src/net/tcp.rs`)
Implementou-se a função [`drain_and_realign_ooo`](file:///E:/Projetos/Jinn/src/net/tcp.rs#L577-L618):
- **Eclipsamento Total:** Se `remote_seq` ultrapassar o fim do bloco OOO, `ooo_len` é zerado imediatamente.
- **Eclipsamento Parcial:** Se uma retransmissão sobrepor o início do OOO, o prefixo já recebido é descartado via `copy_within` e `ooo_seq` é alinhado com `remote_seq`.
- **Drenagem Contígua:** Assim que `ooo_seq == remote_seq`, todos os bytes fora de ordem pendentes são absorvidos no buffer de recepção do socket e confirmados via ACK.
- O buffer OOO foi ampliado de 1536 para 4096 bytes com suporte a agregação de múltiplos segmentos contíguos.
- O buffer de loopback foi expandido de 16 para 64 quadros (`LOOPBACK_QUEUE_SIZE = 64`), eliminando perdas em testes locais de alto volume.

### 2.3. Controle de Janela Dinâmica e Zero-Window Reopen (`src/net/socket.rs`)
- O tamanho do anel circular de recepção contínua [`SOCK_RX_STREAM_SIZE`](file:///E:/Projetos/Jinn/src/net/socket.rs#L13) foi expandido para **64 KiB** (`65536` bytes).
- A janela padrão TCP anunciada foi elevada para `65535`.
- A rotina `recv_socket` passou a monitorar `prev_win`: se a janela anterior era 0 e espaço foi liberado (`prev_win == 0 && sock.tcb.local_win > 0`), ou se houve incremento significativo (>= 4096 bytes), um pacote ACK de *Window Update* é emitido imediatamente para desimpedir o servidor remoto.

### 2.4. Resiliência do Jinn Browser e Telemetria (`apps/ola-mundo/src/main.rs`)
- Os buffers estáticos `HTML_FILE_BUF`, `HTTP_RX_BUF` e `HTTP_PLAIN_BUF` foram ampliados de 32 KiB para **64 KiB** (`HTTP_BUF_SIZE = 65536`).
- O tempo limite de inatividade foi elevado de 3,0s para **8,0 segundos** (1600 spins com sono cooperativo de 5ms), zerando a contagem de ociosidade sempre que novos bytes forem recebidos.
- **Entrega Resiliente de Corpo:** Caso ocorra timeout de rede mas o socket já tenha recebido uma resposta HTTP 200 com cabeçalho válido e dados de corpo, o navegador não aborta: entrega os dados recebidos para decodificação e renderização, informando o usuário via telemetria serial.
- Adicionou-se validação de integridade estrutural HTML (`[HTML-INTEGRITY]`) antes do envio ao motor DOM.

---

## 3. Matriz de Testes Unitários e de Estresse

Foi executada a suíte completa em `tests/tcp_socket_stress_tests.rs`:

```text
running 16 tests
test test_http_chunked_terminator_detection ... ok
test test_http_chunked_with_extensions_and_trailers ... ok
test test_http_multi_fragment_content_length_simulation ... ok
test test_loopback_two_way_stream_simulation ... ok
test test_socket_flow_control_zero_window_and_reopen ... ok
test test_tcp_ooo_realign_on_overlapping_retransmit ... ok
test test_tcp_large_32k_and_64k_stream_flow_control ... ok
test test_http_chunked_transfer_decoding ... ok
test test_http_chunked_with_embedded_zero_substring_no_false_termination ... ok
test test_http_resilient_body_delivery_on_slow_server ... ok
test test_socket_stream_partial_read_preserves_remaining_bytes ... ok
test test_tcp_out_of_order_reassembly_and_merge ... ok
test test_large_16k_html_stream_transfer_no_loss ... ok
test test_socket_stream_circular_wraparound_integrity ... ok
test test_tcp_sequence_tracking_contiguous_and_partial ... ok
test test_tcp_overlapping_segments_resilience ... ok

test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

**Resultado:** 16 testes aprovados (100% de sucesso).

---

## 4. Geração da ISO e Compatibilidade com VirtualBox

A imagem `jinn.iso` foi gerada através do pipeline Limine BIOS + UEFI (`scripts/run.ps1 -NoRun`).

Para validação no Oracle VirtualBox:
1. O script `scripts/create_vbox_vm.ps1` converte o disco particionado `disk.img` em `disk.vdi` (64 MiB RAW -> Dynamic VDI).
2. A máquina virtual `"Aladdin OS"` é provisionada com:
   - 1024 MB de RAM e 2 vCPUs
   - Placa de rede NAT com emulação **Intel PRO/1000 MT Desktop** (`82540EM`) ou **PCnet-FAST III**
   - Porta IDE 0: `disk.vdi` (Partição 1: FAT32 com apps e recursos visuais; Partição 2: Ext2)
   - Porta IDE 1: `jinn.iso` (Mídia de boot Limine)
   - Porta Serial 1 redirecionada para `vbox_serial.log` para captura contínua de logs e telemetria.
