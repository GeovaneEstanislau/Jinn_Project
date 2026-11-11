# Relatório de Análise e Resolução de Incidente Crítico: Cursor de Mouse PS/2 Congelado no Aladdin Desktop (Ring 3)

---

## 1. Sumário Executivo do Incidente

Durante a transição do microkernel **Jinn OS** e do ambiente gráfico **Aladdin Desktop (Ring 3)** para o teto rígido de 64 MB de RAM e a restauração do teclado PS/2, identificou-se uma regressão operacional crítica: o **ponteiro de mouse PS/2 permanecia estático no centro da tela (`x=512, y=384`)**, sem responder a movimentos físicos ou cliques de botões, embora o subsistema de teclado estivesse funcional e o compositor estivesse executando a 60 FPS.

Este relatório documenta a investigação forense de causa raiz através de todas as camadas do sistema (controlador físico i8042, rotas de interrupção PIC IRQ 12, driver de kernel em Ring 0, barramento de syscalls e servidor de display em Ring 3), detalhando as correções arquiteturais implementadas e a validação formal executada.

---

## 2. Investigação Técnica e Análise de Causa Raiz

A auditoria forense do pipeline de entrada mapeou cinco anomalias concorrentes que causavam a paralisia do cursor:

```
[Hardware PS/2 Aux]
       │ (IRQ 12 / Port 0x60 / 0x64)
       ▼
[pic::unmask_irq(12)] ───► [mouse_irq_handler()] (Vetor 44) ──┐
                                                               │  [PORT60_LOCK]
[Syscall 75 sys_poll_input] ──► [ps2_mouse::poll()] ───────────┴─► [process_byte()]
                                                                         │ (3-byte State Machine)
                                                                         ▼
                                                           [MOUSE_X, MOUSE_Y, BUTTONS] (Atomic)
                                                                         │
                                                                         ▼
                                                           [gui::poll_hardware_input()]
                                                                         │ (Syscall 75)
                                                                         ▼
                                                           [aladdin-server (Ring 3)]
                                                                         │ (AladdinSession::run)
                                                                         ▼
                                                           [DirtyRect Fast-Path & sys_present]
```

### Anomalia A: Reinicialização Destrutiva e Corrupção de Ciclo por Bytes de ACK
- **Mecanismo da Falha:** O kernel inicializava o mouse PS/2 em early boot (`src/main.rs:164`), desmascarando a IRQ 12 no PIC. Posteriormente, ao spawnar o compositor em Ring 3, o binário `aladdin-server` invocava a `Syscall 72: sys_register_graphics(0)`. A rotina `register_graphics_server` chamava novamente `ps2_mouse::init()`.
- **Efeito:** Como as interrupções já estavam ativas (`sti`), os comandos de reinicialização `0xF6` (Set Defaults) e `0xF4` (Enable Streaming) geravam bytes de resposta ACK (`0xFA`) enviados pelo controlador para a porta `0x60`. A IRQ 12 disparava imediatamente; o handler interceptava o byte `0xFA` e o repassava para `process_byte(0xFA)`.
- Como `0xFA` possui o bit 3 ativo (`0xFA & 0x08 != 0`), a máquina de estados considerava `0xFA` como o Byte 0 (Flags) do primeiro pacote! O segundo `0xFA` era interpretado como Delta X (`cycle = 2`). Ao terminar a inicialização, a máquina de estados estava permanentemente desfasada (`cycle = 2`), e qualquer pacote de movimento real subsequente tinha seu primeiro byte interpretado como Delta Y com flags corrompidas contendo bits de overflow (`0xC0`), provocando o descarte sistemático de 100% dos pacotes.

### Anomalia B: Concorrência e Race Condition na Porta 0x60 (ISR vs Polling)
- **Mecanismo da Falha:** O sistema utilizava um modelo híbrido: interrupções assíncronas via IRQ 12 e polling periódico via `ps2_mouse::poll()` chamado a cada iteração de `sys_poll_input` (Syscall 75).
- **Efeito:** Como ambas as rotinas liam a porta `0x60` sem sincronização mútua, uma interrupção podia disparar exatamente quando o loop de polling lia a porta `0x60`, causando leitura de lixo, pacotes divididos entre ISR e polling ou travamento na leitura da porta.

### Anomalia C: Estado Global Mutável sem Proteção Atômica
- **Mecanismo da Falha:** A estrutura de coordenadas `MOUSE` era definida como `static mut MOUSE: MouseState` desprovida de qualquer primitiva de sincronização ou barreira de memória.
- **Efeito:** Em ambientes multiprocessados (SMP 4 vCPUs) ou sob interrupções preemptivas, acessos simultâneos causavam leituras truncadas de coordenadas e botões.

### Anomalia D: Supressão por Deadlock de Estado Inicial em `src/gui.rs`
- **Mecanismo da Falha:** `LAST_MOUSE_STATE` em `src/gui.rs` era inicializado estaticamente em `(512, 384, 0)`.
- **Efeito:** Se os primeiros deltas fossem descartados devido à Anomalia A, o estado do mouse permanecia idêntico a `LAST_MOUSE_STATE`, impedindo a emissão de qualquer evento de sincronização para a Ring 3.

---

## 3. Modificações e Soluções Implementadas

### 3.1. Estado Atômico e Lock-Free no Driver PS/2 (`src/interrupts/ps2_mouse.rs`)
Eliminou-se completamente o `static mut MOUSE` em favor de registradores atômicos com garantias `Acquire-Release`:
- `MOUSE_X: AtomicUsize` (coordenada horizontal absoluta)
- `MOUSE_Y: AtomicUsize` (coordenada vertical absoluta)
- `MOUSE_BUTTONS: AtomicU8` (máscara de botões: bit 0=Left, bit 1=Right, bit 2=Middle)
- `MOUSE_CYCLE: AtomicU8` (fase da máquina de estados: 0, 1 ou 2)
- `MOUSE_PACKET: [AtomicU8; 3]` (buffer de montagem do pacote)
- `MOUSE_MAX_X / MOUSE_MAX_Y: AtomicUsize` (limites do framebuffer)

### 3.2. Exclusão Mútua na Ingestão de Hardware (`PORT60_LOCK`)
Criou-se uma barreira atômica `PORT60_LOCK: AtomicBool` que impede qualquer colisão entre a ISR de interrupção (IRQ 12) e a rotina de polling:
- Se `mouse_irq_handler()` estiver ativo, `poll()` recua imediatamente sem bloquear.
- Se `poll()` estiver drenando o buffer, a ISR envia o `pic::send_eoi(12)` e preserva a integridade da porta.

### 3.3. Idempotência e Blindagem contra Vazamento de ACKs no `init()`
O método `ps2_mouse::init()` foi totalmente reestruturado:
1. Verifica `HARDWARE_INITIALIZED`: se o hardware já foi configurado no boot, apenas redefine `reset_sync()` e atualiza as dimensões de tela, sem enviar comandos destrutivos ao i8042.
2. Ao inicializar o hardware, **mascara temporariamente a IRQ 12 no PIC** (`pic::mask_irq(12)`).
3. Executa a negociação síncrona dos comandos `0xF6` e `0xF4`, consumindo os bytes `0xFA` diretamente via polling de status.
4. Drena quaisquer bytes espúrios residuais.
5. Zera a máquina de estados (`cycle = 0`).
6. Desmascara a IRQ 12 (`pic::unmask_irq(12)`), garantindo que a linha de streaming seja aberta apenas em estado limpo e alinhado.

### 3.4. Decodificação 9-bit e Timeout de Sincronização
- Implementou-se decodificação com extensão de sinal rigorosa para deltas de 9 bits (`(raw as i32) - 256` quando o bit de sinal está ativo).
- Adicionou-se timeout automático de pacote: se transcorrerem mais de 5 ticks (~50ms) entre bytes de um pacote incompleto, o ciclo é resetado para zero, recuperando o alinhamento na próxima movimentação do usuário.

### 3.5. Instrumentação Completa de Telemetria
Implementaram-se contadores atômicos exportados via `get_telemetry()`:
- `IRQ12_COUNT`
- `MOUSE_BYTES_RECEIVED`
- `MOUSE_PACKETS_COMPLETED`
- `MOUSE_INVALID_PACKETS`
- `DISCARDED_OVERFLOW`
- `MOUSE_EVENTS_DELIVERED`

---

## 4. Matriz de Testes e Validação

| ID do Teste | Cenário de Teste | Resultado | Status |
| :--- | :--- | :--- | :--- |
| **TEST-M-01** | Montagem de pacote de 3 bytes e movimentação positiva | Coordenadas atualizadas com delta positivo | **APROVADO** |
| **TEST-M-02** | Extensão de sinal 9-bit com deltas negativos (`0xFE`, `0xFD`) | Subtração precisa de X e adição precisa de Y | **APROVADO** |
| **TEST-M-03** | Limitação de coordenadas nas bordas (`0,0` e `max_x-1, max_y-1`) | Clamping em `0` e `1023/767` sem estouro | **APROVADO** |
| **TEST-M-04** | Detecção de overflow (bits 6 e 7) | Descarte de deltas e incremento de `DISCARDED_OVERFLOW` | **APROVADO** |
| **TEST-M-05** | Rejeição de bytes órfãos sem bit 3 ativo no ciclo 0 | Byte descartado e `cycle` mantido em 0 | **APROVADO** |
| **TEST-M-06** | Recuperação de desfasamento de fluxo por injeção de lixo | Resincronização automática no primeiro byte válido | **APROVADO** |
| **TEST-M-07** | Timeout de pacote incompleto (>50ms) | Reset de `cycle` para 0 após lapso temporal | **APROVADO** |
| **TEST-M-08** | Transições de clique (MouseDown, MouseUp, botões múltiplos) | Máscara de botões atômica `(1, 2, 3)` | **APROVADO** |
| **TEST-M-09** | Reconfiguração dinâmica de resolução (`set_bounds`) | Limites atualizados e cursor reajustado | **APROVADO** |
| **TEST-M-10** | Acurácia dos contadores de telemetria | Contadores batem exatamente com bytes/pacotes | **APROVADO** |
| **TEST-M-11** | Teste E2E em QEMU (`-m 64M -smp 4`) com `mouse_move` e clicks | Screendump capturado com cursor renderizado | **APROVADO** |

---

## 5. Conclusão

O incidente crítico do mouse PS/2 foi definitivamente solucionado. O pipeline de entrada entre o controlador i8042, as interrupções PIC IRQ 12, as syscalls de hardware e o compositor Ring 3 opera agora de forma estável, atômica e resiliente a falhas temporais, mantendo o sistema em conformidade com o teto de 64 MB de memória RAM.
