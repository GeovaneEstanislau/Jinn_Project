import subprocess
import time
import socket
import os
import sys

def parse_ppm(path):
    if not os.path.exists(path):
        return None
    with open(path, "rb") as f:
        header = f.readline().strip()
        if header != b"P6":
            return None
        # Pula comentários
        line = f.readline()
        while line.startswith(b"#"):
            line = f.readline()
        parts = line.split()
        width, height = int(parts[0]), int(parts[1])
        maxval = int(f.readline().strip())
        data = f.read()
        return (width, height, data)

def main():
    print("=================================================================")
    print("   JINN OS / ALADDIN — AUDITORIA E2E DE ENTRADA (SEÇÃO 8)        ")
    print("=================================================================")

    log_file = "qemu_mouse_test.log"
    ppm1 = "cursor_capture_1.ppm"
    ppm2 = "cursor_capture_2.ppm"

    for f in [log_file, ppm1, ppm2]:
        if os.path.exists(f):
            try: os.remove(f)
            except: pass

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    port = 5589

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

    print(f"[*] Iniciando QEMU (-m 64M, -smp 4) na porta monitor {port}...")
    proc = subprocess.Popen(qemu_cmd)

    s = None
    results = {
        "1_boot": False,
        "2_keyboard_initial": False,
        "3_mouse_move": False,
        "4_coords_changed": False,
        "5_visual_cursor_changed": False,
        "6_mouse_clicks": False,
        "7_keyboard_sustained": False,
        "8_simultaneous": False,
        "9_no_freeze": False,
    }

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
            sys.exit(1)

        # Consome prompt inicial do QEMU
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
            except:
                return ""
            res_b = b""
            while True:
                try:
                    chunk = s.recv(1024)
                    if not chunk: break
                    res_b += chunk
                    if b"(qemu)" in chunk: break
                except: break
            return res_b.decode(errors="ignore")

        # 1. Inicializa o sistema e aguarda compositor Ring 3
        print("[*] 1. Aguardando inicialização do sistema e Aladdin Desktop em Ring 3...")
        for sec in range(45):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        if "Servidor grafico em execucao" in f.read():
                            results["1_boot"] = True
                            print(f"  [+] Aladdin Desktop Ring 3 operacional aos {sec+1}s!")
                            break
                except: pass

        if not results["1_boot"]:
            print("[-] Timeout na inicialização do desktop.")
            return

        time.sleep(2.5)

        # 2. Confirma funcionamento do teclado isolado
        print("[*] 2. Verificando funcionamento do teclado isolado...")
        for k in ["t", "e", "s", "t"]:
            qcmd(f"sendkey {k}")
            time.sleep(0.25)

        time.sleep(0.5)

        # 3. Captura imagem inicial com cursor para comprovação visual
        print("[*] 3. Capturando screendump inicial do desktop...")
        qcmd(f"screendump {ppm1}")
        time.sleep(1.0)

        # 4. Injeta movimentos de mouse com alteração real de coordenadas
        print("[*] 4. Injetando movimentos de mouse relativos...")
        qcmd("mouse_move -100 80")
        time.sleep(0.4)
        qcmd("mouse_move -60 40")
        time.sleep(0.5)

        # 5. Captura imagem com cursor movido
        print("[*] 5. Capturando screendump com cursor movido...")
        qcmd(f"screendump {ppm2}")
        time.sleep(1.0)

        # 6. Injeta cliques de botão do mouse
        print("[*] 6. Injetando cliques do mouse (Down e Up)...")
        qcmd("mouse_button 1")
        time.sleep(0.3)
        qcmd("mouse_button 0")
        time.sleep(0.5)

        # 7. Verifica se teclado continua funcional após uso do mouse
        print("[*] 7. Verificando teclado após uso de mouse...")
        for k in ["o", "k"]:
            qcmd(f"sendkey {k}")
            time.sleep(0.3)
        time.sleep(0.5)

        # 8. Repete movimentos e digitação simultaneamente (Intercalação)
        print("[*] 8. Executando movimentos e digitação simultaneamente...")
        simult_ops = [
            ("key", "a"),
            ("move", (20, 20)),
            ("key", "b"),
            ("move", (-20, -20)),
            ("key", "c"),
        ]
        for op, val in simult_ops:
            if op == "key":
                qcmd(f"sendkey {val}")
            elif op == "move":
                qcmd(f"mouse_move {val[0]} {val[1]}")
            time.sleep(0.3)

        time.sleep(2.0)

        # Encerramento gracioso
        qcmd("quit")
        time.sleep(0.5)

    finally:
        if s: s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    # ── Validação e Auditoria dos Critérios da Seção 8 ──
    print("\n=================================================================")
    print("   AUDITORIA DE CRITÉRIOS DA SEÇÃO 8 — TESTE E2E                 ")
    print("=================================================================")

    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
        log_content = f.read()

    # Critério 2: Teclado inicial funcional
    keys = log_content.count("[ALADDIN-INPUT] KeyDown")
    if keys >= 4:
        results["2_keyboard_initial"] = True
    print(f"  [2] Teclado Inicial Funcional:           {'PASS' if results['2_keyboard_initial'] else 'FAIL'} ({keys} teclas registradas)")

    # Critério 3: Movimento de mouse injetado e recebido em Ring 3
    moves = log_content.count("[ALADDIN-INPUT] MouseMove")
    if moves >= 1:
        results["3_mouse_move"] = True
    print(f"  [3] Movimentos de Mouse Recebidos:       {'PASS' if results['3_mouse_move'] else 'FAIL'} ({moves} movimentos)")

    # Critério 4: Alteração real de coordenadas comprovada pelo log de Ring 3
    coords = []
    for line in log_content.splitlines():
        if "[ALADDIN-INPUT] MouseMove" in line:
            coords.append(line.strip())
    if len(coords) >= 2 and coords[0] != coords[1]:
        results["4_coords_changed"] = True
    print(f"  [4] Alteração Real de Coordenadas:       {'PASS' if results['4_coords_changed'] else 'FAIL'} ({len(coords)} posições)")
    for c in coords[:3]:
        print(f"       -> {c}")

    # Critério 5: Mudança visual real do cursor entre captures PPM
    ppm_diff_pixels = 0
    p1 = parse_ppm(ppm1)
    p2 = parse_ppm(ppm2)
    if p1 and p2 and p1[0] == p2[0] and p1[1] == p2[1]:
        d1 = p1[2]
        d2 = p2[2]
        # Compara pixels (RGB trios)
        for i in range(0, min(len(d1), len(d2)), 3):
            if d1[i:i+3] != d2[i:i+3]:
                ppm_diff_pixels += 1
        if ppm_diff_pixels > 0:
            results["5_visual_cursor_changed"] = True
    print(f"  [5] Mudança Visual do Cursor (PPM Diff): {'PASS' if results['5_visual_cursor_changed'] else 'FAIL'} ({ppm_diff_pixels} pixels alterados)")

    # Critério 6: Injeção e registro de cliques
    clicks = log_content.count("[ALADDIN-INPUT] MouseBtn")
    if clicks >= 2:
        results["6_mouse_clicks"] = True
    print(f"  [6] Cliques Registrados:                 {'PASS' if results['6_mouse_clicks'] else 'FAIL'} ({clicks} eventos de botão)")

    # Critério 7: Teclado continua funcional após uso de mouse
    if keys >= 6:
        results["7_keyboard_sustained"] = True
    print(f"  [7] Teclado Sustentado Pós-Mouse:        {'PASS' if results['7_keyboard_sustained'] else 'FAIL'}")

    # Critério 8: Movimentos e digitação simultâneos
    if keys >= 9 and moves >= 3:
        results["8_simultaneous"] = True
    print(f"  [8] Entrada Simultânea e Concorrente:    {'PASS' if results['8_simultaneous'] else 'FAIL'} ({keys} teclas, {moves} movs)")

    # Critério 9: Detecção de ausência de congelamento ou perda de eventos
    if all([results[k] for k in ["1_boot", "2_keyboard_initial", "3_mouse_move", "4_coords_changed", "5_visual_cursor_changed", "6_mouse_clicks", "7_keyboard_sustained", "8_simultaneous"]]):
        results["9_no_freeze"] = True
    print(f"  [9] Ausência Total de Congelamento:      {'PASS' if results['9_no_freeze'] else 'FAIL'}")

    all_pass = all(results.values())
    if all_pass:
        print("\n>>> AUDITORIA SEÇÃO 8: APROVADA COM SUCESSO EM TODOS OS 9 CRITÉRIOS! (PASS) <<<\n")
        sys.exit(0)
    else:
        print("\n>>> AUDITORIA SEÇÃO 8: FALHA EM UM OU MAIS CRITÉRIOS! (FAIL) <<<\n")
        sys.exit(1)

if __name__ == "__main__":
    main()
