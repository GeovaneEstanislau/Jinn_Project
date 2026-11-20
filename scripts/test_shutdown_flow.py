import subprocess
import time
import socket
import os
import sys

def main():
    log_file = "test_shutdown.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except: pass

    qemu_bin = r"D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe"
    if not os.path.exists(qemu_bin):
        qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"

    port = 5596
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

    print("[*] Iniciando QEMU para validar o fluxo de Desligamento com Tela Desligada...")
    proc = subprocess.Popen(qemu_cmd)
    s = None

    try:
        for _ in range(25):
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
            return False

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

        print("[*] Aguardando compositor gráfico Aladdin...")
        ready = False
        for sec in range(35):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        if "Servidor grafico em execucao" in f.read():
                            ready = True
                            print(f"[+] Aladdin pronto aos {sec+1}s!")
                            break
                except: pass

        if not ready:
            print("[-] Timeout esperando desktop.")
            return False

        time.sleep(2.0)

        # 1. Abre Menu Iniciar: mouse sai de (512, 384) e vai para (50, 14)
        # dx = 50 - 512 = -462
        # dy = 14 - 384 = -370 (QEMU monitor: -370 é para cima!)
        print("[*] 1. Clicando no Launcher para abrir Menu Iniciar...")
        qcmd("mouse_move -460 -370")
        time.sleep(0.4)
        qcmd("mouse_button 1")
        time.sleep(0.1)
        qcmd("mouse_button 0")
        time.sleep(0.6)

        # 2. Clica no item 9: 'Desligar Sistema' (x ~ 100, y ~ 294)
        # Da posição atual (52, 14) -> dx = +48, dy = +(294 - 14) = +280 (QEMU monitor: +280 é para baixo!)
        print("[*] 2. Clicando no item 'Desligar Sistema'...")
        qcmd("mouse_move 50 280")
        time.sleep(0.4)
        qcmd("mouse_button 1")
        time.sleep(0.1)
        qcmd("mouse_button 0")
        time.sleep(0.8)

        # 3. Screendump do Modal de Confirmação
        qcmd("screendump modal_shutdown.ppm")
        time.sleep(0.4)

        # 4. Clica no Botão 1 do Modal: '[1] Aplicar c/ Tela Desligada e Desligar (Recomendado)'
        # Modal centralizado: botão 1 em (512, 402)
        # Da posição atual (102, 294) -> dx = +410, dy = +(402 - 294) = +108 (para baixo)
        print("[*] 3. Clicando no Botao 1 do Modal: 'Aplicar c/ Tela Desligada e Desligar'...")
        qcmd("mouse_move 410 108")
        time.sleep(0.4)
        qcmd("mouse_button 1")
        time.sleep(0.1)
        qcmd("mouse_button 0")

        print("[*] Aguardando aplicacao com tela apagada e desligamento ACPI...")
        time.sleep(6.0)

    finally:
        if s: s.close()
        proc.kill()
        proc.wait()

    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            lines = f.readlines()
            print("\n[*] Ultimas 35 linhas do log serial:")
            for l in lines[-35:]:
                print("   ", l.strip())

if __name__ == "__main__":
    main()
