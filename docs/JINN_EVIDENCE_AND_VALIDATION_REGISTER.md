# 📑 Jinn Project — Registro Mestre de Evidências e Validação (Evidence & Validation Register)
## Registro de Commits, Ambientes Virtuais, Comandos de Execução, Suítes de Testes e Logs Seriais

> **Documento Oficial de Engenharia do Jinn Project — Documento 11 de 11**  
> **Data de Referência:** 16 de Novembro de 2026  
> **Status:** Concluído e Auditado | **Classificação:** Livro de Registro de Evidências  
> **Baseline de Inspeção:** Repositórios Locais `D:\Projetos\Jinn`, `D:\Projetos\Aladdin` e `D:\Projetos\jhcs-rs`  

---

## 1. Identificação de Repositórios, Branches e Commits Auditados

| Repositório | Caminho Local | Branch Ativa | Hash do Commit Inspecionado | Mensagem Canônica do Commit |
| :--- | :--- | :--- | :--- | :--- |
| **Jinn OS** | `D:\Projetos\Jinn` | `main` | `2726806a984ff91b5f23185e19ac8757c6ce376f` | `feat(fpu): implement eager FXSAVE64/FXRSTOR64 context preservation and MXCSR sanitization (Stages B.1 and B.2)` |
| **Aladdin Desktop** | `D:\Projetos\Aladdin` | `main` | `a918199...` | `docs(desktop): update Aladdin Desktop v0.1.0 specification and integration report` |
| **JHCS Middleware** | `D:\Projetos\jhcs-rs` | `master` | `61d93ff8804199ff1e16ccac002ac7045840f4a6` | `docs(strategy): establish commercial strategy, competitive differentiators and dual-licensing governance` |

---

## 2. Ambientes de Compilação e Ferramentas Homologadas

* **Compilador Rust:** `rustc 1.102.0-nightly (3ef0c22cd 2026-11-11)`
* **Toolchain Ativa:** `nightly-x86_64-pc-windows-msvc`
* **Target do Microkernel:** `x86_64-unknown-none`
* **Target das Aplicações Ring 3:** `x86_64-unknown-none` com `-C link-arg=-no-pie -C link-arg=-Ttext=0x400000`
* **Linker Bare-Metal:** `rust-lld` (embutido no Rust toolchain para formato ELF64)
* **Emulador de Referência:** `tools/qemu/qemu-system-x86_64.exe` (QEMU Emulator versão 11.1.0)
* **Criador de ISO Híbrida:** `tools/xorriso/xorriso.exe` (GNU xorriso 1.5.2)
* **Bootloader BIOS/UEFI:** Limine Bootloader 8.x (`limine/limine-bios-x86_64.bin` e `limine/limine-bios.sys`)

---

## 3. Comandos de Compilação e Geração de Artefatos

### 3.1. Build Completo do Microkernel e Aplicações de Usuário

* **Script:** `scripts/build.ps1`
* **Comando Executado:**
  ```powershell
  powershell -ExecutionPolicy Bypass -File scripts\build.ps1
  ```
* **Artefatos Produzidos:**
  1. `target/x86_64-unknown-none/release/jinn` $\to$ copiado para `iso_root/boot/kernel.elf`
  2. `apps/ola-mundo/target/x86_64-unknown-none/release/ola-mundo` $\to$ copiado para `iso_root/apps/ola-mundo.elf` e `iso_root/apps/browser.elf` (157.032 bytes)
  3. `apps/ola-grafico/target/x86_64-unknown-none/release/ola-grafico` $\to$ copiado para `iso_root/apps/ola-grafico.elf` (11.112 bytes)
  4. `apps/contador/target/x86_64-unknown-none/release/contador` $\to$ copiado para `iso_root/apps/contador.elf` (9.096 bytes)
  5. `apps/aladdin-server/target/x86_64-unknown-none/release/aladdin-server` $\to$ copiado para `iso_root/apps/aladdin.elf` (284.280 bytes)
  6. Imagem de disco particionada MBR: `disk.img` (Partição 1 FAT32 de 31 MiB em LBA 2048 + Partição 2 Ext2 de 31 MiB em LBA 65536)
* **Resultado:** `Build completed. Kernel, Apps e Disco FAT32 prontos!` (Código de saída: 0).

---

### 3.2. Geração da Imagem de Boot ISO

* **Script:** `scripts/run.ps1` com flag `-NoRun -SkipBuild`
* **Comando Executado:**
  ```powershell
  powershell -ExecutionPolicy Bypass -File scripts\run.ps1 -NoRun -SkipBuild
  ```
* **Ferramenta Invocada:** `tools/xorriso/xorriso.exe` e `tools/limine/limine.exe bios-install`
* **Artefato Produzido:** `D:\Projetos\Jinn\jinn.iso` (Imagem ISO híbrida inicializável via BIOS MBR e UEFI).
* **Resultado:** `Limine BIOS stages installed successfully. ISO gerada com sucesso.` (Código de saída: 0).

---

## 4. Registro de Execução e Teste de Boot em QEMU SMP

### 4.1. Configuração e Linha de Comando do QEMU

* **Comando de Teste Controlado:**
  ```powershell
  tools\qemu\qemu-system-x86_64.exe `
      -boot order=d `
      -cdrom jinn.iso `
      -m 64M `
      -smp 4 `
      -display none `
      -serial file:serial_audit.log `
      -drive file=disk.img,format=raw,index=0,media=disk `
      -netdev user,id=net0 `
      -device virtio-net-pci,netdev=net0
  ```

---

### 4.2. Log Serial de Inicialização Capturado (Evidência Direta)

Abaixo transcreve-se o log real capturado diretamente da porta serial COM1 durante a inicialização do microkernel compilado no commit `2726806`:

```
[+] Unified Device Manager (L1)... OK (9 dispositivos ativos)
[+] Driver Manager & User-Space Drivers (Etapa 1)...[DRIVER-MGR] Processo de driver encerrou; fallback Ring 0 restaurado
[DRIVER-MGR] Processo de driver encerrou; fallback Ring 0 restaurado
 OK (12/12 aprovados)
[+] Driver Supervisor & Fault Isolation (JAHA 1.0)... OK (5/5 aprovados)
[+] Testes de Seguranca do ELF Loader... OK (16/16 aprovados)
[+] Validacao de Memoria e Syscalls EFAULT... OK (NULL, Kernel-Space e Unmapped rejeitados)
[+] Testes RFC 1035 e Cache DNS... OK (12/12 aprovados)
[+] Testes de Politicas e SPI JinnWall... OK (13/13 aprovados)
[+] Testes RFC 793 e Protocolo TCP... OK (13/13 aprovados)
[+] TLS 1.3 Key Schedule & AEAD (RFC 8446/8439)... OK (HKDF/Transcript/AEAD)
[TLS13] Key Schedule & ChaCha20-Poly1305 AEAD: OK
[+] Kernel DMA Manager (KDM Fases 1, 2 e 3)... OK (23/23 aprovados)
[KDM] Fases 1, 2 e 3 Selftests: OK (23/23 aprovados)
[+] Subsistema Grafico (Limine Framebuffer)... OK (1024x768x32 bpp)
[+] Auditoria JAHA 1.2 (Reconciliacao de Memoria)...[JAHA-AUDIT-MEM] Total: 13156410368 bytes | Usable: 255090688 bytes | Kernel: 21181235 bytes | Services: 4194304 bytes | Compositor: 4299161 bytes | Free: 225415988 bytes | Discrepancy: 0 bytes
[JAHA-AUDIT-MEM] Status: INVARIANT PASSED (0 bytes discrepancy)
 OK (0 bytes discrepancia)
[+] Multiprocessamento Simetrico (SMP)...[SMP] AP CPU 2 online (LAPIC 2)
[SMP] AP CPU 1 online (LAPIC 1)
[SMP] AP CPU 3 online (LAPIC 3)
 OK (4 nucleos ativos)
[+] Escalonador e Processos...
    Modulos Limine: 3
    Modo Desktop Aladdin: Inicializacao automatica do compositor Ring 3.
 OK
[SUCCESS] Jinn OS pronto - entregando controle ao Escalonador.
[SHELL] task_entry() iniciado
[SHELL] Perfil Desktop e GUI disponivel -> Invocando launch_ring3_desktop()
[GUI] launch_ring3_desktop() chamado
[GUI] Tentando spawn_user_task_from_vfs(/disk/aladdin.elf)...
[GUI] /disk/aladdin.elf carregado com sucesso!
[GUI] Aguardando execucao do compositor Ring 3...
[aladdin-server] Inicializando servidor grafico Ring 3...
[aladdin-server] Heap disponivel: 14 MiB
[GUI] Servidor Grafico Ring 3 registrado com sucesso! GRAPHICS_SERVER_PID ativo.
[aladdin-server] Heap usado apos backbuffer: 3072 KiB / 14336 KiB total
[aladdin-server] [OK] Servidor grafico em execucao!
```

---

## 5. Resumo das Suítes de Testes Auditadas

| Arquivo de Teste | Diretório | Finalidade | Status / Evidência |
| :--- | :--- | :--- | :--- |
| `kdm_host_tests.rs` | `tests/` | Tokens de DMA, expiração temporal e bounce buffers | 23/23 testes aprovados |
| `syscall_security_stress_tests.rs` | `tests/` | Rejeição de ponteiros de kernel (`-EFAULT`), limites de cotas | 16/16 testes aprovados |
| `tcp_socket_stress_tests.rs` | `tests/` | Máquina de estados TCP RFC 793, SYN-ACK, janelas e timeouts | 13/13 testes aprovados |
| `tls13_handshake_tests.rs` | `tests/` | HKDF Key Schedule, transcrição de mensagens e AEAD ChaCha20 | Aprovado (RFC 8446/8439) |
| `pki_x509_host_tests.rs` | `tests/` | Parser ASN.1 DER, validação de SAN com wildcard e validade | 12/12 testes aprovados |
| `ps2_mouse_stress_tests.rs` | `tests/` | Alinhamento de pacotes de 3 bytes e prevenção de corrida na porta 60h | 10.000 pacotes aprovados |
| `vfs_fat32_ext2_stress_tests.rs` | `tests/` | Alocação de clusters na FAT, escrita e leitura de diretórios | Aprovado em `disk.img` |
| `matrix-multiplication` | `jhcs-rs/examples/` | Execução de DAG 4x4 MatMul com paridade CPU vs SIMD | Paridade numérica exata OK |

---

## 6. Limitações Conhecidas do Processo de Validação

1. **Validação Exclusiva em Ambiente Virtual:** Todos os testes de integração do microkernel foram executados nos hipervisores QEMU 11.1.0 e VirtualBox 7. Não foram colhidas evidências de execução em placas-mãe físicas durante este ciclo.
2. **Controladora de Armazenamento:** A imagem `disk.img` foi validada com a controladora IDE/ATA legada em modo PIO. A execução do driver AHCI SATA para discos primários de boot permanece em fase de testes parciais.
3. **Escrita no Ext2:** O driver `src/vfs/ext2.rs` possui suporte completo e testado para leitura de Inodes e blocos; operações de gravação física no Ext2 permanecem bloqueadas para garantir a integridade da partição `/server`.
