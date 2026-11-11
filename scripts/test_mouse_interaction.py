import subprocess
import time
import socket
import os
import sys

def main():
    log_file = "qemu_mouse_test.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except: pass

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    port = 5588

    qemu_cmd = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", "jinn.iso",
        "-drive", "file=disk.img,format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "virtio-net-pci,netdev=net0",
        "-m", "64M",
        "-smp", "4",
        "-display", "none",
        "-serial", f"file:{log_file}",
        "-monitor", f"tcp:127.0.0.1:{port},server,nowait"
    ]

    print(f"[TEST MOUSE] Iniciando QEMU com -m 64M na porta monitor {port}...")
    proc = subprocess.Popen(qemu_cmd)

    s = None
    try:
        for attempt in range(25):
            time.sleep(1)
            try:
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.settimeout(3.0)
                s.connect(("127.0.0.1", port))
                print(f"[TEST MOUSE] Conectado ao monitor QEMU (tentativa {attempt+1})")
                break
            except Exception:
                if s: s.close()
                s = None

        if not s:
            print("[TEST MOUSE] [FAIL] Nao conectou ao monitor QEMU.")
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

        print("[TEST MOUSE] Aguardando boot do Jinn OS e inicializacao do Aladdin Desktop (Ring 3)...")
        boot_ok = False
        for sec in range(25):
            time.sleep(1.0)
            if os.path.exists(log_file):
                with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                    content = f.read()
                    if "[aladdin-server] [OK] Servidor grafico em execucao!" in content:
                        print(f"  [+] Aladdin Desktop Ring 3 ativo aos {sec+1}s!")
                        boot_ok = True
                        break

        if not boot_ok:
            print("[TEST MOUSE] [WARN] Timeout aguardando mensagem de boot, continuando teste...")

        time.sleep(2.0)

        print("[TEST MOUSE] Enviando eventos de movimento e cliques via QEMU monitor...")
        # 1. Movimento relativo (dx=+60, dy=+40)
        print("  -> mouse_move 60 40")
        qemu_send("mouse_move 60 40")
        time.sleep(0.5)

        # 2. Clique com botão esquerdo (pressionar = 1, soltar = 0)
        print("  -> mouse_button 1 (MouseDown)")
        qemu_send("mouse_button 1")
        time.sleep(0.5)

        print("  -> mouse_button 0 (MouseUp)")
        qemu_send("mouse_button 0")
        time.sleep(0.5)

        # 3. Mais um movimento (dx=-30, dy=-20)
        print("  -> mouse_move -30 -20")
        qemu_send("mouse_move -30 -20")
        time.sleep(0.5)

        # 4. Envia screendump para capturar visualmente a cena com cursor
        ppm_screen = "qemu_mouse_desktop.ppm"
        print(f"  -> screendump {ppm_screen}")
        qemu_send(f"screendump {ppm_screen}")
        time.sleep(1.0)

        # 5. Encerramento gracioso
        qemu_send("quit")
        time.sleep(1.0)

    finally:
        if s: s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    print("[TEST MOUSE] Analisando logs seriais gerados...")
    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read()
            print("--- ULTIMAS LINHAS DO SERIAL LOG ---")
            for line in content.splitlines()[-25:]:
                print("  ", line)

    if os.path.exists("qemu_mouse_desktop.ppm"):
        size = os.path.getsize("qemu_mouse_desktop.ppm")
        print(f"[TEST MOUSE] Screendump gerado com sucesso: qemu_mouse_desktop.ppm ({size} bytes)")
    else:
        print("[TEST MOUSE] [AVISO] Screendump PPM nao encontrado.")

if __name__ == "__main__":
    main()

