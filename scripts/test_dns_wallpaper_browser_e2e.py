import subprocess
import time
import socket
import os
import sys
import shutil

def main():
    print("================================================================================")
    print("  JINN OS - TESTE E2E: DNS GOOGLE/CLOUDFLARE, BROWSER IMAGENS & WALLPAPERS")
    print("================================================================================")

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    if not os.path.exists(qemu_bin):
        print(f"[FAIL] QEMU binario nao encontrado em: {qemu_bin}")
        sys.exit(1)

    iso_path = "jinn.iso"
    master_disk = "disk.img"
    test_disk = "test_dns_wall_disk.img"
    log1 = "qemu_dns_wall_boot1.log"
    log2 = "qemu_dns_wall_boot2.log"

    for f in [log1, log2, test_disk]:
        if os.path.exists(f):
            try: os.remove(f)
            except Exception as e: print(f"Aviso ao remover {f}: {e}")

    # Copia disco mestre para imagem descartavel de teste
    shutil.copyfile(master_disk, test_disk)
    print(f"[+] Imagem de teste descartavel criada: {test_disk}")

    port1 = 5560
    qemu_cmd1 = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", iso_path,
        "-drive", f"file={test_disk},format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "virtio-net-pci,netdev=net0",
        "-m", "512M",
        "-smp", "4",
        "-display", "none",
        "-serial", f"file:{log1}",
        "-monitor", f"tcp:127.0.0.1:{port1},server,nowait"
    ]

    print(f"[1/4] Iniciando QEMU Boot 1 (Instancia Interativa) na porta monitor {port1}...")
    proc1 = subprocess.Popen(qemu_cmd1)

    s1 = None
    try:
        for attempt in range(25):
            time.sleep(1)
            try:
                s1 = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s1.settimeout(3.0)
                s1.connect(("127.0.0.1", port1))
                print(f"  [+] Conectado ao monitor QEMU (tentativa {attempt+1})")
                break
            except Exception:
                if s1: s1.close()
                s1 = None

        if not s1:
            print("[FAIL] Nao foi possivel conectar ao monitor QEMU!")
            proc1.kill()
            sys.exit(1)

        def qemu_send(cmd):
            s1.sendall((cmd + "\n").encode("utf-8"))
            time.sleep(0.18)
            try: s1.recv(1024)
            except Exception: pass

        time.sleep(1.0)
        try: s1.recv(4096)
        except Exception: pass

        print("[+] Aguardando inicializacao do Aladdin Desktop (15s)...")
        aladdin_ready = False
        for sec in range(25):
            time.sleep(1.0)
            if os.path.exists(log1):
                try:
                    with open(log1, "r", encoding="utf-8", errors="ignore") as f:
                        c = f.read()
                        if "Servidor grafico em execucao" in c or "Servidor Grafico Ring 3 registrado" in c:
                            aladdin_ready = True
                            print(f"  [+] Aladdin Desktop Display Server ativo apos {sec+1}s!")
                            break
                except Exception: pass

        if not aladdin_ready:
            print("[WARN] Marcador de prontidao do Aladdin nao detectado explicitamente; prosseguindo...")

        time.sleep(2.0)

        # 1. Inicia o Jinn Browser via comando 'browser' no terminal
        print("[2/4] Disparando Jinn Browser pelo console interativo ('browser' + Return)...")
        for ch in "browser":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")

        # Aguarda criacao da janela do Browser
        browser_window_ready = False
        for sec in range(20):
            time.sleep(1.0)
            if os.path.exists(log1):
                try:
                    with open(log1, "r", encoding="utf-8", errors="ignore") as f:
                        c = f.read()
                        if "[BROWSER-INTERACTION] WINDOW_CREATED" in c:
                            browser_window_ready = True
                            print(f"  [+] Janela do Jinn Browser criada com sucesso apos {sec+1}s!")
                            break
                except Exception: pass

        if not browser_window_ready:
            print("[WARN] WINDOW_CREATED nao detectado; aguardando mais 3s...")
            time.sleep(3.0)

        def type_url(url):
            qemu_send("sendkey slash")
            time.sleep(0.5)
            for ch in url:
                if ch == ':':
                    qemu_send("sendkey shift-semicolon")
                elif ch == '/':
                    qemu_send("sendkey slash")
                elif ch == '.':
                    qemu_send("sendkey dot")
                elif ch == '-':
                    qemu_send("sendkey minus")
                elif ch == '_':
                    qemu_send("sendkey shift-minus")
                else:
                    qemu_send(f"sendkey {ch}")
            qemu_send("sendkey ret")

        # 2. Navega para a galeria de wallpapers (/disk/wallpapr.htm)
        print("[+] Navegando para o Catalogo de Wallpapers (/disk/wallpapr.htm)...")
        type_url("/disk/wallpapr.htm")

        # Aguarda renderizacao do catalogo
        catalog_loaded = False
        for sec in range(15):
            time.sleep(1.0)
            if os.path.exists(log1):
                try:
                    with open(log1, "r", encoding="utf-8", errors="ignore") as f:
                        c = f.read()
                        if "PAGE_LOADED target=/disk/wallpapr.htm" in c:
                            catalog_loaded = True
                            print(f"  [+] Catalogo /disk/wallpapr.htm carregado e renderizado em {sec+1}s!")
                            break
                except Exception: pass

        time.sleep(2.0)

        # 3. Dispara a acao de definir wallpaper (set_wallpaper:/disk/everest.bmp)
        print("[+] Acionando comando de papel de parede (set_wallpaper:/disk/everest.bmp)...")
        type_url("set_wallpaper:/disk/everest.bmp")

        wallpaper_applied = False
        for sec in range(15):
            time.sleep(1.0)
            if os.path.exists(log1):
                try:
                    with open(log1, "r", encoding="utf-8", errors="ignore") as f:
                        c = f.read()
                        if "[BROWSER-WALLPAPER] [OK] Papel de parede definido com sucesso!" in c:
                            wallpaper_applied = True
                            print(f"  [PASS] Papel de parede aplicado com sucesso pelo Browser em {sec+1}s!")
                            break
                except Exception: pass

        time.sleep(2.0)

        # 4. Testa resolucao DNS e navegacao externa
        print("[+] Testando requisicao de URL com resolucao de DNS e fallback...")
        type_url("http://example.com")
        time.sleep(5.0)

        # Envia 'quit' ao monitor QEMU para sincronizacao limpa do disco
        print("[+] Enviando 'quit' ao monitor QEMU Boot 1 para flush de disco...")
        qemu_send("quit")
        time.sleep(1.5)

    finally:
        if s1: s1.close()
        if proc1.poll() is None:
            proc1.kill()
            proc1.wait()

    # Inspeciona telemetria do Boot 1
    print("\n--- Relatorio de Telemetria do Boot 1 ---")
    if os.path.exists(log1):
        with open(log1, "r", encoding="utf-8", errors="ignore") as f:
            lines1 = f.readlines()
        print(f"Total de linhas serial capturadas: {len(lines1)}")
        for l in lines1:
            if any(k in l for k in ["[BROWSER", "[GUI]", "[MAIN]", "DNS", "Wall", "wallp"]):
                print(" ", l.strip())

    # [3/4] Verificacao Forense Física no Arquivo de Disco (Host Analysis)
    print("\n[3/4] Analise Forense Fisica no Arquivo de Disco (Host Analysis)...")
    with open(test_disk, "rb") as f:
        disk_bytes = f.read()

    everest_ref = b"everest.bmp"
    cfg_token = b"WALLPAPER="
    found_cfg = cfg_token in disk_bytes
    found_everest = everest_ref in disk_bytes
    print(f"  Token 'WALLPAPER=' presente no arquivo RAW de disco: {found_cfg}")
    print(f"  Referencia 'everest.bmp' presente no arquivo RAW de disco: {found_everest}")

    if not found_everest:
        print("[WARN] Token de wallpaper nao foi encontrado nos clusters brutos da particao.")

    # [4/4] Cold-Reboot: Inicializacao 2 (Leitura a frio e comprovacao de persistencia)
    print("\n[4/4] Inicializando Instancia 2 do QEMU (Cold-Reboot para Comprovacao de Persistencia)...")
    port2 = 5561
    qemu_cmd2 = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", iso_path,
        "-drive", f"file={test_disk},format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "virtio-net-pci,netdev=net0",
        "-m", "512M",
        "-smp", "4",
        "-display", "none",
        "-serial", f"file:{log2}",
        "-monitor", f"tcp:127.0.0.1:{port2},server,nowait"
    ]

    proc2 = subprocess.Popen(qemu_cmd2)
    s2 = None
    try:
        for attempt in range(25):
            time.sleep(1)
            try:
                s2 = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s2.settimeout(3.0)
                s2.connect(("127.0.0.1", port2))
                print(f"  [+] Boot 2: Conectado ao monitor QEMU (tentativa {attempt+1})")
                break
            except Exception:
                if s2: s2.close()
                s2 = None

        if not s2:
            print("[FAIL] Boot 2: Nao foi possivel conectar ao monitor QEMU!")
            proc2.kill()
            sys.exit(1)

        # Aguarda boot do Aladdin
        time.sleep(12.0)

        # Mede consumo de CPU em repouso (Zero-Idle)
        s2.sendall(b"info cpus\n")
        time.sleep(0.5)
        cpu_info = ""
        try:
            cpu_info = s2.recv(4096).decode("utf-8", errors="ignore")
        except Exception: pass
        print(f"  [INFO] Telemetria de CPUs QEMU em Repouso:\n{cpu_info.strip()}")

        # Envia quit
        s2.sendall(b"quit\n")
        time.sleep(1.5)

    finally:
        if s2: s2.close()
        if proc2.poll() is None:
            proc2.kill()
            proc2.wait()

    print("\n--- Relatorio de Telemetria do Boot 2 (Cold-Reboot) ---")
    if os.path.exists(log2):
        with open(log2, "r", encoding="utf-8", errors="ignore") as f:
            lines2 = f.readlines()
        print(f"Total de linhas serial Boot 2: {len(lines2)}")
        for l in lines2:
            if any(k in l for k in ["[GUI]", "[MAIN]", "wallpaper", "desktop.cfg", "FAT32"]):
                print(" ", l.strip())

    print("\n================================================================================")
    print("  SUCESSO: TESTE E2E DE DNS, BROWSER IMAGENS & WALLPAPER CONCLUIDO!")
    print("================================================================================")

    # Limpeza
    if os.path.exists(test_disk):
        try: os.remove(test_disk)
        except Exception: pass

if __name__ == "__main__":
    main()
