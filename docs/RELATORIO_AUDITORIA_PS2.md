# Relatório Técnico de Auditoria Abrangente: Subsistema PS/2 (Teclado e Mouse i8042) no Jinn OS

---

## 1. Visão Geral da Auditoria

O subsistema **PS/2** do **Jinn Microkernel** gerencia a interação do operador humano através das duas portas físicas do controlador **i8042**:
- **Porta 1 (Dispositivo Principal):** Teclado PS/2 padrão (Scancode Set 1 / Set 2 traduzido).
- **Porta 2 (Dispositivo Auxiliar):** Mouse PS/2 padrão (pacotes seriais de 3 bytes com sinal 9-bit).

Esta auditoria realizou uma revisão integral de segurança, concorrência e integridade das rotinas dos drivers `src/interrupts/ps2_keyboard.rs` e `src/interrupts/ps2_mouse.rs`.

---

## 2. Auditoria do Barramento de Portas e Registrador de Status (0x64)

Ambos os dispositivos compartilham os mesmos registradores de I/O na arquitetura x86:
- **Porta 0x60:** Buffer de Dados (leitura do byte recebido / escrita de comandos ao dispositivo).
- **Porta 0x64:** Registrador de Status (leitura) / Registrador de Comandos do Controlador (escrita).

### Mapeamento do Registrador de Status (0x64):
- **Bit 0 (Output Buffer Full - OBF):** `1` indica que há dados na porta `0x60` prontos para leitura.
- **Bit 1 (Input Buffer Full - IBF):** `1` indica que o controlador está ocupado processando comando anterior.
- **Bit 5 (Auxiliary Device Output Buffer Full):**
  - Se `0`: O byte na porta `0x60` pertence ao **Teclado**.
  - Se `1`: O byte na porta `0x60` pertence ao **Mouse**.

### Verificação de Segregação de Tráfego:
- **Driver de Teclado:** Inspeciona `(status & 1) != 0 && (status & 0x20) == 0`. Se o bit 5 estiver ativo, o byte é preservado na porta para que o driver de mouse possa consumi-lo.
- **Driver de Mouse:** Inspeciona `(status & 1) != 0 && (status & 0x20) != 0`. Se o bit 5 for zero, a rotina interrompe o consumo imediatamente para evitar o "roubo" de teclas digitadas pelo usuário.
- **Resultado da Auditoria:** Segregação de hardware estritamente respeitada.

---

## 3. Auditoria do Driver de Teclado (`src/interrupts/ps2_keyboard.rs`)

1. **Bufferização Circular:** As teclas decodificadas são armazenadas em buffer seguro com controle de overflow, prevenindo travamento do shell em digitação rápida.
2. **Tratamento de Modificadores:** Rastreamento atômico das teclas `Shift`, `Caps Lock` e sequências estendidas `0xE0` (setas de navegação, Delete, Home, End).
3. **Isolamento de Foco:** O despachante do compositor Aladdin direciona teclas primariamente à janela com foco de entrada, com fallback automático para o Terminal quando a janela selecionada for não-interativa.

---

## 4. Auditoria do Driver de Mouse (`src/interrupts/ps2_mouse.rs`)

1. **Máquina de Estados de 3 Bytes:**
   - **Ciclo 0:** Rejeita qualquer byte sem o bit 3 ativo (`b & 0x08 == 0`), impedindo desfasamento por bytes órfãos.
   - **Ciclo 1:** Armazena o Delta X bruto.
   - **Ciclo 2:** Armazena o Delta Y bruto e executa o processamento do pacote completo.
2. **Extensão de Sinal 9-bit:**
   - Delta X negativo: $\text{raw\_x} - 256$ quando o bit 4 das flags estiver ativo.
   - Delta Y negativo: $\text{raw\_y} - 256$ quando o bit 5 das flags estiver ativo.
   - Inversão de Y para coordenadas de tela ($\text{tela\_y} = \text{cur\_y} - \text{delta\_y}$).
3. **Descarte por Overflow:** Pacotes com bits 6 ou 7 ativos são descartados com incremento de `DISCARDED_OVERFLOW`.
4. **Resiliência Temporal:** Se transcorrerem mais de 50ms (> 5 ticks de timer) com o pacote incompleto, o ciclo é zerado automaticamente, recuperando o alinhamento de forma transparente.
5. **Lock-Free Concurrency:** Eliminação de variáveis globais mutáveis não sincronizadas; todas as leituras e escritas de posição e botões utilizam registradores atômicos com ordenação `Acquire-Release`.
6. **Proteção de Ingestão (`PORT60_LOCK`):** Garante exclusão mútua entre interrupções da IRQ 12 e chamadas de polling de syscalls.

---

## 5. Resumo da Conformidade

| Requisito do Subsistema PS/2 | Avaliação | Parecer |
| :--- | :--- | :--- |
| Ausência de colisão Teclado / Mouse | Bit 5 verificado em todas as leituras | **CONFORME** |
| Prevenção de vazamento de ACKs no boot | IRQ 12 mascarada durante handshake 0xF6/0xF4 | **CONFORME** |
| Resiliência a perda de pacotes | Timeout temporal >50ms e bit 3 mandatory | **CONFORME** |
| Thread-Safety / SMP Safety | Registradores atômicos com Acquire-Release | **CONFORME** |
| Telemetria de diagnóstico | Contadores atômicos em todos os estágios | **CONFORME** |
