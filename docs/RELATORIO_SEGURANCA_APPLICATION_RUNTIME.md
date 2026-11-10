# Relatório de Segurança Operacional — Aladdin Application Runtime

> **Projeto Jinn OS & Aladdin Desktop**  
> **Classificação:** Auditoria de Segurança e Hardening  
> **Branch de Homologação:** `stabilization/alpha-0.0.3-security`  
> **Data de Emissão:** 10 de Novembro de 2026  

---

## 1. Modelo de Ameaças e Fronteiras de Confiança

A arquitetura de segurança do Aladdin Application Runtime estabelece três domínios mutuamente isolados:

1. **Domínio de Confiança Absoluta (Ring 0):** O microkernel Jinn OS arbitra os recursos físicos, o alocador Buddy e a tabela de páginas mestre. Todo código executado em Ring 0 é estritamente auditado e `#![no_std]`.
2. **Domínio de Confiança Delegada (Ring 3 — Compositor):** O servidor gráfico (`apps/aladdin-server`) possui autorização exclusiva para apresentar quadros no framebuffer físico (`Syscall 74`), porém executa sem privilégios de CPU (IOPL=0), não podendo acessar memória de outros processos exceto via interface mediada pelo kernel (`Syscall 77`).
3. **Domínio Não-Confiável (Ring 3 — Aplicações Clientes):** As aplicações clientes executam com privilégios mínimos. Podem ser arbitrárias, possuir vulnerabilidades ou tentar explorar chamadas de sistema para obter privilégios indevidos.

```
+--------------------------------------------------------------------+
|                  Aplicações Ring 3 (Não Confiáveis)                |
|               [Isolamento PML4 Total / Cotas Ativas]               |
+--------------------------------------------------------------------+
                                 |  Syscalls (int 0x80)
                                 v
+--------------------------------------------------------------------+
|                  Barreira de Validação do Kernel                   |
|  - validate_user_buffer (PML4 Walk, limites canônicos, USER bit)   |
|  - Verificação estrita de owner_pid (Anti-Interference EPERM)       |
|  - Verificação de Cotas de Recursos (task.resources EDQUOT)       |
+--------------------------------------------------------------------+
                                 |
                                 v
+--------------------------------------------------------------------+
|                Jinn Microkernel (Ring 0 — Confiável)               |
+--------------------------------------------------------------------+
```

---

## 2. Auditoria de Validação de Ponteiros de Usuário

Todas as chamadas de sistema que recebem ponteiros vindos de processos de usuário passam obrigatoriamente pela rotina `validate_user_buffer(ptr, len, write_access)` (`src/syscall.rs`):

### 2.1. Regras de Validação Verificadas:
1. **Ponteiro Nulo e Comprimento Zero:**  
   Ponteiros `null` ou tamanhos `0` são rejeitados com `-EINVAL`.
2. **Overflow Aritmético:**  
   A expressão `addr.checked_add(len)` é avaliada. Qualquer tentativa de estourar a faixa de 64 bits resulta em `-EFAULT`.
3. **NULL Guard (Primeiros 64 KiB):**  
   Qualquer endereço abaixo de `0x0000_0000_0001_0000` (`USER_SPACE_MIN`) é sumariamente rejeitado com `-EFAULT`.
4. **Proteção Contra Sondas no Espaço de Kernel (Upper Half Probing):**  
   Qualquer endereço cujo final ultrapasse `0x0000_8000_0000_0000` (`USER_SPACE_MAX`) ou que aponte para a metade superior canônica (`0xFFFF_8000_0000_0000 .. 0xFFFF_FFFF_FFFF_FFFF`) é bloqueado com `-EFAULT`. Isso impede que processos não-privilegiados utilizem syscalls como oráculo de leitura de memória do kernel.
5. **Caminhamento Ativo no Diretório de Páginas (PML4 Walk):**  
   O kernel inspeciona cada página de 4 KiB que compõe o buffer no diretório PML4 ativo (`current_cr3()`):
   - A página deve estar **presente** (`PAGE_PRESENT`).
   - O bit de usuário (`PAGE_USER`) deve estar **obrigatoriamente ativo**. Páginas de supervisor disparam `-EFAULT`.
   - Se o acesso for de escrita (`write_access == true`), o bit gravável (`PAGE_WRITABLE`) deve estar ativo.

---

## 3. Isolamento de Janelas e Propriedade de Superfícies

Cada janela criada no Window Broker possui a seguinte assinatura de controle de acesso:

```rust
pub struct KernelClientWindow {
    pub id: usize,
    pub owner_pid: usize,
    // ...
}
```

### 3.1. Verificação de Propriedade (`owner_pid`):
- Em `client_window_update(caller_pid, win_id, buffer)`:  
  Se `caller_pid != w.owner_pid`, o kernel retorna imediatamente `-EPERM`. Um processo malicioso não pode desenhar ou alterar a janela de outro aplicativo.
- Em `client_window_close(caller_pid, win_id)`:  
  Se `caller_pid != w.owner_pid`, o kernel rejeita com `-EPERM`. Um processo não pode fechar janelas pertencentes a terceiros.
- Em `client_window_poll_event(caller_pid, win_id)`:  
  Se `caller_pid != w.owner_pid`, o kernel rejeita com `-EPERM`. Processos terceiros não podem espionar entradas de teclado ou cliques do mouse de outras janelas (proteção contra *keyloggers* ou *input snooping*).

---

## 4. Prevenção Contra Esgotamento de Recursos (Anti-DoS)

Para evitar que uma aplicação esgote a memória do sistema ou trave o escalonador:

1. **Cota de Janelas por Processo:** Cada tarefa possui um contador ativo `task.resources.open_windows`. Ao invocar `sys_window_create`, o kernel consulta `task.resources.check_and_open_window(&task.limits)`. Caso atinja a cota (ex: 4 janelas), a chamada falha com `-EDQUOT` sem comprometer os recursos globais.
2. **Capacidade Global Delimitada:** A matriz `CLIENT_WINDOWS` possui 32 posições pré-alocadas no kernel. Tentativas que ultrapassem essa capacidade retornam `-EMFILE`.
3. **Limitação Dimensional Estrita:** Janelas com larguras ou alturas fora de `1..=1024` ou `1..=768` falham com `-EINVAL`, prevenindo requisições gigantescas que alocariam dezenas de megabytes no heap.
4. **Fila Circular com Descarte Seguro:** Cada janela dispõe de 16 slots de eventos (`events: [ClientWindowEvent; 16]`). Se a aplicação não drenar sua fila, novos eventos são descartados ou a inserção falha com `-EBUSY`, sem causar vazamento de memória dinâmica ou buffer overflow.

---

## 5. Destruição Atômica e Imunidade a Vazamentos

Quando um processo de usuário é finalizado — seja por encerramento ordenado (`sys_exit(code)`), encerramento forçado (`sys_task_set_state` / `kill`) ou por exceção de CPU não tratada (como Page Fault em endereço inválido ou violação de instrução privilegiada):

```text
[Evento: Falha / Término do Processo PID X]
               |
               v
1. scheduler::exit_task(PID X)
               |
               +---> close_windows_for_pid(PID X)
               |        |
               |        +---> Libera superfícies e buffers no CLIENT_WINDOWS
               |        +---> Notifica AladdinSession e remove da composição
               |        +---> Zera task.resources.open_windows
               |
               +---> JDA::SHARED_BUFFER_POOL.release_all_for_pid(PID X)
               |        +---> Libera handles e buffers Zero-Copy
               |
               +---> ipc::unregister(PID X)
               |        +---> Limpa fila de mensagens IPC
               |
               +---> socket::close_all_for_pid(PID X)
               |        +---> Desconecta e libera sockets de rede
               |
               +---> vfs::close_all_for_pid(PID X)
               |        +---> Libera descritores de arquivo (FDs)
               |
               v
2. scheduler::collect_zombie(PID X)
               |
               +---> Libera frames físicos do PML4 (código, dados, heap, pilha)
               +---> Recicla slot da tarefa na tabela de tarefas
```

> [!IMPORTANT]
> A liberação de todos os recursos de um processo encerrado anormalmente ocorre em **tempo determinístico**, garantindo que nenhum frame de memória física ou slot de janela permaneça órfão.
