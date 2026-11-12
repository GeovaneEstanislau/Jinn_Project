# Relatório Técnico de Diagnóstico e Roteamento de Interrupção: IRQ 12 (PS/2 Mouse) no Jinn OS

---

## 1. Topologia de Hardware e Roteamento de Interrupções

No microkernel **Jinn OS**, o suporte ao mouse PS/2 baseia-se na segunda porta auxiliar do controlador de barramento **i8042** (Super I/O / Chipset PIIX3 / ICH9), cujo canal de sinalização é roteado através da linha **IRQ 12**:

```
                       ┌───────────────────────────────┐
                       │   Controlador i8042 (Aux)    │
                       │    Porta de Dados: 0x60       │
                       │    Porta de Comandos: 0x64    │
                       └──────────────┬────────────────┘
                                      │ Linha Física IRQ 12
                                      ▼
                       ┌───────────────────────────────┐
                       │  PIC Secundário / Escravo     │
                       │  (Linha 4 / Offset INT 40..47)│
                       └──────────────┬────────────────┘
                                      │ Linha de Cascata (IRQ 2)
                                      ▼
                       ┌───────────────────────────────┐
                       │  PIC Primário / Mestre        │
                       │  (Linha 2 / Offset INT 32..39)│
                       └──────────────┬────────────────┘
                                      │ Linha INTR da CPU
                                      ▼
                       ┌───────────────────────────────┐
                       │    IDT Entrada Vetor 44       │
                       │  (0x2C - DPL 0 Interrupt Gate)│
                       └──────────────┬────────────────┘
                                      │
                                      ▼
                       ┌───────────────────────────────┐
                       │  `irq_stub 44` (stubs.s)      │
                       │  `interrupt_dispatcher(44)`   │
                       │  `mouse_irq_handler()`        │
                       └───────────────────────────────┘
```

---

## 2. Configuração dos Registradores do Controlador i8042

Para que o controlador i8042 direcione bytes recebidos na porta auxiliar para a linha IRQ 12 e sinalize adequadamente o status, é necessária uma configuração estrita do **Controller Configuration Byte (CCB)**:

1. **Leitura do CCB Atual:** Comando `0x20` enviado à porta `0x64`, com leitura do byte resultante na porta `0x60`.
2. **Ajuste de Máscara de Bits:**
   - **Bit 1 (Port 2 Interrupt):** Configurado para `1` para habilitar a geração de IRQ 12 na chegada de dados da porta auxiliar.
   - **Bit 5 (Port 2 Clock Disable):** Configurado para `0` para manter o clock serial ativo (bit invertido).
   - **Bit 0 (Port 1 Interrupt):** Mantido de acordo com a política de teclado (polling direto ou IRQ 1).
3. **Escrita do CCB Atualizado:** Comando `0x60` na porta `0x64`, seguido do byte configurado na porta `0x60`.
4. **Habilitação da Porta Auxiliar:** Comando `0xA8` na porta `0x64`.

---

## 3. Sequenciamento de Máscara e Cascata no PIC (8259A)

O vetor de interrupção correspondente à IRQ 12 é calculado como:
$$\text{Vector} = \text{PIC2\_OFFSET} + (12 - 8) = 40 + 4 = 44 \quad (0\text{x}2C)$$

A correta entrega da interrupção requer a manipulação sincronizada das máscaras de IMR (Interrupt Mask Register):

```rust
pub fn unmask_irq(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA) & !(1 << irq);
            outb(PIC1_DATA, mask);
        } else {
            // Desmascara linha no PIC escravo
            let mask = inb(PIC2_DATA) & !(1 << (irq - 8));
            outb(PIC2_DATA, mask);
            // Obrigatoriamente desmascara a linha 2 (cascata) no PIC mestre
            let m1 = inb(PIC1_DATA) & !(1 << 2);
            outb(PIC1_DATA, m1);
        }
    }
}
```

### Protocolo de EOI (End of Interrupt)
Por residir no PIC escravo, a ISR do mouse (`mouse_irq_handler`) deve obrigatoriamente enviar o sinal EOI para **ambos os controladores**:
1. EOI para o PIC2: `outb(0xA0, 0x20)`
2. EOI para o PIC1: `outb(0x20, 0x20)`

Se o EOI para o PIC2 for omitido ou enviado na ordem incorreta, o controlador escravo permanece em estado de serviço (*In-Service Register* ativo), bloqueando indefinidamente quaisquer novas interrupções de hardware da IRQ 12.

---

## 4. Diagnóstico de Bloqueio e Race Conditions

Durante a auditoria, identificou-se que quando o laço do compositor gráfico executava a Syscall 75 (`sys_poll_input`), a função `ps2_mouse::poll()` realizava a leitura de `inb(0x60)` simultaneamente ao disparo da IRQ 12:

| Cenário de Concorrência | Efeito Observado no Kernel Anterior | Correção com `PORT60_LOCK` |
| :--- | :--- | :--- |
| **ISR dispara enquanto `poll()` lê 0x60** | Interrupção lê byte seguinte antes de `poll()` terminar pacote | `PORT60_LOCK` atômico faz `poll()` desistir ou ISR apenas enviar EOI |
| **Comando 0xF6/0xF4 emitido com IRQ12 ativa** | ACK (`0xFA`) capturado pela ISR e injetado em `process_byte` | IRQ 12 é mascarada no PIC durante o handshake de comandos |
| **Interrupção espúria sem dados na porta** | Leitura de lixo com `status & 1 == 0` | Verificação rigorosa de `(status & 1) != 0 && (status & 0x20) != 0` |

---

## 5. Telemetria e Monitoramento da IRQ 12

O subsistema inclui o rastreador atômico `IRQ12_COUNT: AtomicU64`, que é incrementado na primeira linha da ISR antes de qualquer operação de I/O. Isso permite auditar em tempo real:
- Taxa de disparos por segundo em repouso vs movimento (típico: 40–100 IRQs/s em movimento ativo).
- Proporção entre interrupções atendidas pela ISR e eventos consumidos via polling do servidor gráfico.
- Ausência total de interrupções perdidas (*lost interrupts*) ou vetores fantasmas (*spurious IRQs*).

