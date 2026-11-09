import subprocess
import time
import socket
import os
import sys

def main():
    log_file = "qemu_tls13_homologation.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except: pass

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    if not os.path.exists(qemu_bin):
        print(f"[ERRO] QEMU nao encontrado em: {qemu_bin}")
        sys.exit(1)

    peer_script = os.path.join("tools", "interop-peer", "host_peer.py")
    peer_proc = None
    if os.path.exists(peer_script):
        print("[0/4] Iniciando host peer TCP independente na porta 9999...")
        peer_proc = subprocess.Popen([sys.executable, "-u", peer_script, "--mode", "both", "--connections", "4", "--timeout", "40"])
        time.sleep(1.2)

    port = 5597
    qemu_cmd = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", "jinn.iso",
        "-drive", "file=disk.img,format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "virtio-net-pci,netdev=net0",
        "-m", "512M",
        "-smp", "2",
        "-display", "none",
        "-serial", f"file:{log_file}",
        "-monitor", f"tcp:127.0.0.1:{port},server,nowait"
    ]

    print("================================================================================")
    print(" HOMOLOGACAO CRIPTOGRAFICA TLS 1.3 & SUBSISTEMAS DE ENTROPIA / RTC (JINN OS)")
    print("================================================================================")
    print("[1/4] Iniciando QEMU com jinn.iso e disk.img...")
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

        print("[2/4] Monitorando execucao dos testes no log serial (ate 45s)...")
        start_time = time.time()
        while time.time() - start_time < 45.0:
            time.sleep(2.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        content = f.read()
                        if "test_tls13_cryptography_and_syscalls" in content and "Homologados" in content:
                            print(f"  [+] Teste 19 (TLS 1.3 & Criptografia) detectado e concluido no log serial apos {int(time.time() - start_time)}s!")
                            break
                        if "TODOS OS TESTES DA SUITE ADVERSARIAL FORAM CONCLUIDOS" in content:
                            print(f"  [+] Suite completa concluida apos {int(time.time() - start_time)}s!")
                            break
                except:
                    pass

        # Envia 'quit' para encerramento gracioso e flush do arquivo de serial
        print("[3/4] Enviando 'quit' ao monitor QEMU...")
        qemu_send("quit")
        time.sleep(1.5)

    finally:
        if s: s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        if peer_proc and peer_proc.poll() is None:
            peer_proc.kill()
            peer_proc.wait()

    print("[4/4] Analisando relatorio de telemetria do boot e testes de execucao...")
    if not os.path.exists(log_file):
        print("[ERRO] Arquivo de log serial nao foi gerado!")
        sys.exit(1)

    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
        log_content = f.read()
        lines = log_content.splitlines()

    print(f"\nTotal de linhas serial capturadas: {len(lines)}")

    print("\n--- Telemetria do Kernel (RTC CMOS & Entropia CSPRNG) ---")
    kernel_crypto_lines = [l for l in lines if "RTC" in l or "CSPRNG" in l or "Entropia" in l or "RDRAND" in l or "Clock" in l]
    for l in kernel_crypto_lines:
        print(f"  {l}")

    print("\n--- Teste 19: Triade Criptografica & TLS 1.3 em Ring 3 ---")
    test19_lines = [l for l in lines if "TEST 19" in l or "19." in l or "test_tls13" in l]
    passed_count = 0
    for l in test19_lines:
        print(f"  {l}")
        if "[PASS]" in l:
            passed_count += 1

    print(f"\nCriterios de Homologacao Aprovados: {passed_count}")
    if passed_count >= 6:
        print("\n>>> HOMOLOGACAO DE CRIPTOGRAFIA TLS 1.3 E SYSCALLS CONCLUIDA COM SUCESSO INTEGRAL! <<<")
        sys.exit(0)
    else:
        print("\n>>> FALHA NA HOMOLOGACAO: Criterios minimos nao atingidos! <<<")
        sys.exit(1)

if __name__ == "__main__":
    main()
