import subprocess
import time
import socket
import os
import sys

def main():
    log_file = "qemu_driver_homologation.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except: pass

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    if not os.path.exists(qemu_bin):
        print(f"[ERRO] QEMU nao encontrado em: {qemu_bin}")
        sys.exit(1)

    port = 5596
    qemu_cmd = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", "jinn.iso",
        "-drive", "file=disk.img,format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "virtio-net-pci,netdev=net0",
        "-m", "256M",
        "-smp", "1",
        "-display", "none",
        "-serial", f"file:{log_file}",
        "-monitor", f"tcp:127.0.0.1:{port},server,nowait"
    ]

    print("================================================================================")
    print(" ETAPA 1 — AUDITORIA DE HOMOLOGACAO DE DRIVERS EM RING 3 (PS/2 & DRIVER MGR)")
    print("================================================================================")
    print("[1/5] Iniciando QEMU com imagem jinn.iso...")
    proc = subprocess.Popen(qemu_cmd)

    s = None
    try:
        for attempt in range(25):
            time.sleep(1)
            try:
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.settimeout(3.0)
                s.connect(("127.0.0.1", port))
                print(f"  [+] Conectado ao monitor QEMU (tentativa {attempt+1})")
                break
            except Exception:
                if s: s.close()
                s = None

        if not s:
            print("  [FAIL] Nao foi possivel conectar ao monitor QEMU")
            proc.kill()
            sys.exit(1)

        def qemu_send(cmd):
            s.sendall((cmd + "\n").encode("utf-8"))
            time.sleep(0.2)
            try: s.recv(1024)
            except: pass

        time.sleep(1.0)
        try: s.recv(4096)
        except: pass

        print("[2/5] Aguardando bootstrapping do kernel e execucao da suite de testes Ring 3 (35s)...")
        time.sleep(35.0)

        # Envia comando 'mem' para inspecionar contabilidade JAHA 1.2
        print("[3/5] Coletando auditoria de memoria...")
        for ch in "mem":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(1.5)

        # Envia comando 'ps'
        print("[4/5] Coletando tabela de processos...")
        for ch in "ps":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(1.5)

        # Encerramento controlado
        print("[5/5] Encerrando execucao do QEMU com 'quit'...")
        qemu_send("quit")
        time.sleep(1.0)

    finally:
        if s: s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    print("\n--- AUDITORIA DE TELEMETRIA E RESULTADOS DOS TESTES ---")
    if not os.path.exists(log_file):
        print("[ERRO] Arquivo de log serial nao foi gerado!")
        sys.exit(1)

    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
        log_content = f.read()
        lines = log_content.splitlines()

    print(f"Total de linhas de log capturadas: {len(lines)}")

    # 1. Kernel Selftests
    print("\n1. Testes Internos do Kernel (Ring 0 Driver Manager):")
    dm_lines = [l for l in lines if "Driver Manager & User-Space Drivers" in l or "12/12 aprovados" in l]
    for l in dm_lines:
        print(f"  {l}")
    kernel_dm_ok = any("12/12 aprovados" in l for l in lines)

    # 2. Ring 3 Integration Tests (18.1 a 18.14)
    print("\n2. Testes Adversariais e Integrados em Ring 3 (Test 18):")
    test18_lines = [l for l in lines if "18." in l]
    passed_count = 0
    total_expected = 14
    for l in test18_lines:
        print(f"  {l}")
        if "[PASS]" in l:
            passed_count += 1

    print(f"\nResumo Ring 3 Test 18: {passed_count}/{total_expected} testes aprovados.")

    # 3. JAHA 1.2 Memory Invariant
    print("\n3. Invariante de Memoria JAHA 1.2:")
    jaha_lines = [l for l in lines if "[JAHA-AUDIT-MEM]" in l]
    for l in jaha_lines:
        print(f"  {l}")

    mem_ok = any("INVARIANT PASSED (0 bytes discrepancy)" in l for l in lines)
    if mem_ok:
        print("  [PASS] JAHA 1.2 Invariante de contabilidade de memoria: 0 bytes de discrepancia!")

    if passed_count == total_expected and mem_ok and kernel_dm_ok:
        print("\n================================================================================")
        print(" [HOMOLOGACAO ETAPA 1 APROVADA] TODOS OS REQUISITOS FORAM SATISFEITOS!")
        print("================================================================================")
        return 0
    else:
        print("\n================================================================================")
        print(" [HOMOLOGACAO PENDENTE] Nem todos os requisitos foram satisfeitos no log.")
        print("================================================================================")
        return 1

if __name__ == "__main__":
    sys.exit(main())

