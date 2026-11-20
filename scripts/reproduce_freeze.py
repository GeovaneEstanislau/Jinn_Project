import subprocess
import time
import socket
import os
import sys

def main():
    log_file = "freeze_repro.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except: pass

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    port = 5599

    qemu_cmd = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", "jinn.iso",
        "-drive", "file=disk.img,format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "virtio-net-pci,netdev=net0",
        "-m", "256M",
        "-smp", "4",
        "-display", "none",
        "-serial", f"file:{log_file}",
        "-monitor", f"tcp:127.0.0.1:{port},server,nowait"
    ]

    print("[*] Iniciando QEMU...")
    proc = subprocess.Popen(qemu_cmd)

    s = None
    try:
        for attempt in range(25):
            time.sleep(1)
            try:
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.settimeout(5.0)
                s.connect(("127.0.0.1", port))
                break
            except Exception:
                if s: s.close()
                s = None

        if not s:
            print("[-] Falha ao conectar ao monitor QEMU.")
            proc.kill()
            return

        buf = b""
        while b"(qemu)" not in buf:
            try:
                c = s.recv(1024)
                if not c: break
                buf += c
            except: break

        def qcmd(cmd):
            try:
                s.sendall((cmd.strip() + "\n").encode("utf-8"))
            except Exception:
                return ""
            res_b = b""
            while True:
                try:
                    chunk = s.recv(1024)
                    if not chunk: break
                    res_b += chunk
                    if b"(qemu)" in chunk: break
                except Exception:
                    break
            return res_b.decode(errors="ignore")

        print("[*] Aguardando inicializacao do Aladdin Desktop...")
        desktop_ready = False
        for sec in range(35):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        content = f.read()
                        if "Servidor grafico em execucao" in content:
                            desktop_ready = True
                            print(f"[+] Desktop pronto aos {sec+1}s!")
                            break
                except: pass

        if not desktop_ready:
            print("[-] Timeout esperando desktop.")
            return

        time.sleep(2.0)
        print("[*] Iniciando teste de estresse de cliques e movimentos...")
        
        # Teste com 150 cliques em diferentes posições
        clicks_done = 0
        frozen = False

        for i in range(150):
            # Move um pouco
            dx = (i % 7 - 3) * 15
            dy = (i % 5 - 2) * 15
            qcmd(f"mouse_move {dx} {dy}")
            time.sleep(0.05)
            
            # Clica botão 1 (Down)
            qcmd("mouse_button 1")
            time.sleep(0.05)
            
            # Solta botão 1 (Up)
            qcmd("mouse_button 0")
            time.sleep(0.05)
            
            clicks_done += 1
            if (i + 1) % 25 == 0:
                print(f"  [>] {clicks_done} cliques enviados...")
                # Checa se o QEMU ainda responde
                resp = qcmd("info cpus")
                if "CPU #" not in resp:
                    print(f"  [-] VM parou de responder no clique {clicks_done}!")
                    frozen = True
                    break

        print(f"[*] Total de cliques enviados: {clicks_done}")
        time.sleep(2.0)

        # Checa status final da VM
        resp = qcmd("info cpus")
        print(f"[*] info cpus final: {resp.strip()[:100]}")

        # Tenta tirar screendump
        qcmd("screendump freeze_test_screen.ppm")
        time.sleep(1.0)
        if os.path.exists("freeze_test_screen.ppm"):
            sz = os.path.getsize("freeze_test_screen.ppm")
            print(f"[+] Screendump gerado: {sz} bytes")

    finally:
        if s: s.close()
        proc.kill()
        proc.wait()

    # Mostra ultimas linhas do log
    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            lines = f.readlines()
            print(f"[*] Total de linhas no log: {len(lines)}")
            print("[*] Ultimas 30 linhas do log serial:")
            for l in lines[-30:]:
                print("   ", l.strip())

if __name__ == "__main__":
    main()
