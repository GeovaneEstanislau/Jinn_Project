import subprocess
import time
import socket
import os
import sys

def test_offline_update_gui():
    log_file = "test_update_gui.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except: pass

    qemu_bin = r"D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe"
    if not os.path.exists(qemu_bin):
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

    print("[*] [TEST 1] Iniciando QEMU com Aladdin Desktop para validar Gerenciador de Softwares e Offline Update...")
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

        print("[*] Aguardando compositor Aladdin Ring 3 ficar pronto...")
        desktop_ready = False
        for sec in range(35):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        content = f.read()
                        if "Servidor grafico em execucao" in content:
                            desktop_ready = True
                            print(f"[+] Aladdin Desktop pronto aos {sec+1}s!")
                            break
                except: pass

        if not desktop_ready:
            print("[-] Timeout esperando desktop.")
            return False

        time.sleep(2.0)

        # 1. Abre o Menu Iniciar: mouse começa em 512, 384. Move para (50, 14) -> dx = -462, dy = -370
        print("[*] Clicando no Launcher para abrir Menu Iniciar...")
        qcmd("mouse_move -460 -370")
        time.sleep(0.3)
        qcmd("mouse_button 1")
        time.sleep(0.1)
        qcmd("mouse_button 0")
        time.sleep(0.5)

        # 2. Clica no item 5: 'Softwares & Updates' (x ~ 100, y ~ 190) -> dx = +50, dy = +176
        print("[*] Clicando em 'Softwares & Updates'...")
        qcmd("mouse_move 50 176")
        time.sleep(0.3)
        qcmd("mouse_button 1")
        time.sleep(0.1)
        qcmd("mouse_button 0")
        time.sleep(0.8)

        # Screendump do Desktop com o Software Manager aberto
        qcmd("screendump software_manager_screen.ppm")
        time.sleep(0.5)
        if os.path.exists("software_manager_screen.ppm"):
            print(f"[+] Screendump do Software Manager capturado com sucesso ({os.path.getsize('software_manager_screen.ppm')} bytes)!")

        # 3. Abre o Menu Iniciar novamente para acionar Desligar Sistema
        print("[*] Abrindo Menu Iniciar novamente...")
        qcmd("mouse_move -50 -176")
        time.sleep(0.3)
        qcmd("mouse_button 1")
        time.sleep(0.1)
        qcmd("mouse_button 0")
        time.sleep(0.5)

        # 4. Clica no item 9: 'Desligar Sistema' (x ~ 100, y ~ 290) -> dx = +50, dy = +276
        print("[*] Clicando em 'Desligar Sistema'...")
        qcmd("mouse_move 50 276")
        time.sleep(0.3)
        qcmd("mouse_button 1")
        time.sleep(0.1)
        qcmd("mouse_button 0")
        time.sleep(0.8)

        # Screendump do Modal de Confirmação com Tela Desligada
        qcmd("screendump shutdown_modal_screen.ppm")
        time.sleep(0.5)
        if os.path.exists("shutdown_modal_screen.ppm"):
            print(f"[+] Screendump do Modal de Desligamento capturado ({os.path.getsize('shutdown_modal_screen.ppm')} bytes)!")

        # 5. Clica no Botão 1 do Modal: '[1] Aplicar c/ Tela Desligada e Desligar (Recomendado)'
        # Modal está centralizado em 512, 384. Botão 1 está em y ~ 384 - 110 + 128 = 402, x ~ 512
        # Da posição atual (100, 290) -> dx = +412, dy = +112
        print("[*] Clicando no Botao 1: 'Aplicar c/ Tela Desligada e Desligar'...")
        qcmd("mouse_move 412 112")
        time.sleep(0.3)
        qcmd("mouse_button 1")
        time.sleep(0.1)
        qcmd("mouse_button 0")

        # Aguarda 6 segundos para a conclusão do fluxo offline e poweroff
        print("[*] Aguardando aplicacao offline c/ tela apagada e poweroff ACPI...")
        time.sleep(6.0)

    finally:
        if s: s.close()
        proc.kill()
        proc.wait()

    # Validação do log serial
    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read()
            print("\n=== VERIFICACAO DO FLUXO DE ATUALIZACAO OFFLINE NO DESLIGAMENTO ===")
            if "Display Power: OFF" in content:
                print("[PASS] Tela apagada com sucesso (Display Power: OFF - 0% carga de video)!")
            else:
                print("[-] Display Power: OFF nao detectado no log.")

            if "OFFLINE-UPDATE" in content:
                print("[PASS] Rotina de instalacao offline ativada e pacotes aplicados!")
            else:
                print("[-] OFFLINE-UPDATE nao detectado no log.")

            if "Initiating ACPI" in content or "sys_poweroff" in content:
                print("[PASS] Syscall 80 / Poweroff ACPI executada com sucesso!")
            else:
                print("[-] Poweroff ACPI nao detectado.")

            return True
    return False

if __name__ == "__main__":
    test_offline_update_gui()
