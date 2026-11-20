import subprocess
import time
import socket
import os
import sys

def main():
    log_file = "ui_clicks.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except: pass

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    port = 5598

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

        print("[*] Aguardando desktop...")
        desktop_ready = False
        for sec in range(35):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        if "Servidor grafico em execucao" in f.read():
                            desktop_ready = True
                            print(f"[+] Desktop pronto aos {sec+1}s!")
                            break
                except: pass

        if not desktop_ready:
            print("[-] Timeout esperando desktop.")
            return

        time.sleep(2.0)

        # Mouse começa no centro: 512, 384
        # Para clicar no atalho da direita (x ~ 970, y ~ 100):
        # dx = 970 - 512 = +458
        # dy = 100 - 384 = -284
        print("[*] Movendo mouse para a coluna de atalhos da direita (x=970, y=100)...")
        qcmd("mouse_move 450 -280")
        time.sleep(0.3)
        
        # Clica 10 vezes no atalho Historia / Sobre
        for c in range(10):
            print(f"  [>] Clicando atalho Historia ({c+1}/10)...")
            qcmd("mouse_button 1")
            time.sleep(0.1)
            qcmd("mouse_button 0")
            time.sleep(0.2)

        # Move para o atalho de Código / VFS (y ~ 250 -> dy = +150)
        print("[*] Movendo mouse para o atalho Codigo (VFS)...")
        qcmd("mouse_move 0 150")
        time.sleep(0.3)
        for c in range(5):
            print(f"  [>] Clicando atalho Codigo VFS ({c+1}/5)...")
            qcmd("mouse_button 1")
            time.sleep(0.1)
            qcmd("mouse_button 0")
            time.sleep(0.2)

        # Move para o atalho de Browser (y ~ 320 -> dy = +70)
        print("[*] Movendo mouse para o atalho Browser...")
        qcmd("mouse_move 0 70")
        time.sleep(0.3)
        for c in range(5):
            print(f"  [>] Clicando atalho Browser ({c+1}/5)...")
            qcmd("mouse_button 1")
            time.sleep(0.1)
            qcmd("mouse_button 0")
            time.sleep(0.3)

        # Move para a Dock inferior (x ~ 512 -> dx = -450, y ~ 730 -> dy = +400)
        print("[*] Movendo mouse para a Dock inferior...")
        qcmd("mouse_move -450 400")
        time.sleep(0.3)
        for c in range(10):
            print(f"  [>] Clicando na Dock ({c+1}/10)...")
            qcmd("mouse_button 1")
            time.sleep(0.1)
            qcmd("mouse_button 0")
            time.sleep(0.2)

        # Aguarda 5 segundos para observar o comportamento do sistema
        print("[*] Aguardando 5s para avaliar estabilidade...")
        time.sleep(5.0)

        resp = qcmd("info cpus")
        print(f"[*] info cpus: {resp.strip()}")

        qcmd("screendump ui_clicks_screen.ppm")
        time.sleep(1.0)
        if os.path.exists("ui_clicks_screen.ppm"):
            print(f"[+] Screendump salvo: {os.path.getsize('ui_clicks_screen.ppm')} bytes")

    finally:
        if s: s.close()
        proc.kill()
        proc.wait()

    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            lines = f.readlines()
            print(f"[*] Total de linhas no log: {len(lines)}")
            print("[*] Ultimas 40 linhas do log serial:")
            for l in lines[-40:]:
                print("   ", l.strip())

if __name__ == "__main__":
    main()
