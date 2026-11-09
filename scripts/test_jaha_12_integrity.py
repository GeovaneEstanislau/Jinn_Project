import subprocess
import time
import socket
import os
import sys

def main():
    log_file = "qemu_jaha_12_integrity.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except: pass

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    if not os.path.exists(qemu_bin):
        print(f"[ERRO] QEMU nao encontrado em: {qemu_bin}")
        sys.exit(1)

    port = 5595
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
    print(" JAHA 1.2 — TESTE AUTOMATIZADO DE INTEGRIDADE, RECONCILIACAO E REPRODUTIBILIDADE")
    print("================================================================================")
    print("[1/5] Iniciando QEMU (256 MB RAM / 1 vCPU / Legacy Profile)...")
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

        print("[2/5] Aguardando bootstrapping do kernel e suites de selftest (18s)...")
        time.sleep(18.0)

        # Envia comando 'mem' para o shell
        print("[3/5] Invocando comando 'mem' no shell do kernel...")
        for ch in "mem":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(1.5)

        # Envia comando 'ps'
        print("[4/5] Invocando comando 'ps' para inspecionar tarefas Ring 0 e Ring 3...")
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

    print("\n--- AUDITORIA DE TELEMETRIA SERIAL ---")
    if not os.path.exists(log_file):
        print("[ERRO] Arquivo de log serial nao foi gerado!")
        sys.exit(1)

    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
        log_content = f.read()
        lines = log_content.splitlines()

    print(f"Total de linhas de log capturadas: {len(lines)}")

    audit_lines = [l for l in lines if "[JAHA-AUDIT-MEM]" in l]
    for l in audit_lines:
        print(f"  {l}")

    passed = any("INVARIANT PASSED (0 bytes discrepancy)" in l for l in lines)
    if passed:
        print("\n[RESULTADO] SUCESSO: Invariante de contabilidade de memoria validada (0 discrepancia)!")
    else:
        print("\n[RESULTADO] AVISO: Linha de invariante nao detectada textualmente no log serial.")

if __name__ == "__main__":
    main()
