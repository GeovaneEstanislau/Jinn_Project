import subprocess
import time
import socket
import os
import sys

QEMU_BIN = r"C:\Program Files\qemu\qemu-system-x86_64.exe"

def run_suite_for_mode(smp_cores=4, mode_label="SMP (4 Núcleos, 64 MB)"):
    print(f"\n=======================================================")
    print(f"  EXECUTANDO BATERIA DE TESTES: {mode_label}")
    print(f"=======================================================")

    log_file = f"serial_test_{smp_cores}.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except: pass

    port = 5570 + smp_cores
    cmd = [
        QEMU_BIN,
        "-boot", "order=d",
        "-cdrom", "jinn.iso",
        "-drive", "file=disk.img,format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "virtio-net-pci,netdev=net0",
        "-m", "64M",
        "-smp", str(smp_cores),
        "-display", "none",
        "-serial", f"file:{log_file}",
        "-monitor", f"tcp:127.0.0.1:{port},server,nowait"
    ]

    proc = subprocess.Popen(cmd)
    s = None
    res = {
        "boot": False,
        "scenario_a": False,
        "scenario_b": False,
        "scenario_c": False,
        "scenario_d": False,
        "keys": 0,
        "moves": 0,
        "clicks": 0,
        "irq1": 0,
        "irq12": 0,
        "pic_info": "",
    }

    try:
        # 1. Conexão ao Monitor
        for _ in range(35):
            time.sleep(1)
            try:
                s = socket.socket()
                s.settimeout(5.0)
                s.connect(("127.0.0.1", port))
                break
            except:
                if s: s.close()
                s = None

        if not s:
            print("  [-] Falha ao conectar ao monitor QEMU.")
            return res

        # Consume initial monitor banner and initial prompt
        initial_buf = b""
        while b"(qemu)" not in initial_buf:
            try:
                chunk = s.recv(1024)
                if not chunk: break
                initial_buf += chunk
            except:
                break

        def qcmd(c):
            try:
                s.sendall((c.strip() + "\n").encode())
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

        # 2. Aguarda inicialização do Aladdin Desktop em Ring 3
        print("  [*] Aguardando boot e inicialização do Aladdin Desktop em Ring 3...")
        for sec in range(40):
            time.sleep(1)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        if "Servidor grafico em execucao" in f.read():
                            res["boot"] = True
                            print(f"  [+] Aladdin Desktop Ring 3 operacional aos {sec+1}s!")
                            break
                except: pass

        if not res["boot"]:
            print("  [-] Timeout ao aguardar Aladdin Desktop.")
            return res

        # Aguarda o compositor renderizar o primeiro quadro e assentar o loop de eventos
        time.sleep(2.5)

        # 3. CENÁRIO A: SOMENTE TECLADO
        print("  [>>>] Executando Cenário A: Somente Teclado")
        keys_a = ["h", "e", "l", "l", "o", "ret"]
        for k in keys_a:
            qcmd(f"sendkey {k}")
            time.sleep(0.25)
        time.sleep(0.5)

        # 4. CENÁRIO B: SOMENTE MOUSE
        print("  [>>>] Executando Cenário B: Somente Mouse")
        r = qcmd("mouse_move -200 150")
        print("    move 1:", repr(r[:30] if r else ""))
        time.sleep(0.25)
        r = qcmd("mouse_move 30 20")
        print("    move 2:", repr(r[:30] if r else ""))
        time.sleep(0.25)
        r = qcmd("mouse_button 1")
        print("    btn 1:", repr(r[:30] if r else ""))
        time.sleep(0.25)
        r = qcmd("mouse_button 0")
        print("    btn 0:", repr(r[:30] if r else ""))
        time.sleep(0.5)

        # 5. CENÁRIO C: SIMULTÂNEO (INTERCALAÇÃO CONCORRENTE)
        print("  [>>>] Executando Cenário C: Teclado e Mouse Simultâneos")
        simult_ops = [
            ("key", "a"),
            ("move", (20, 20)),
            ("key", "b"),
            ("btn", 1),
            ("key", "c"),
            ("btn", 0),
            ("move", (-20, -20)),
            ("key", "ret"),
        ]
        for op, val in simult_ops:
            if op == "key":
                qcmd(f"sendkey {val}")
            elif op == "move":
                qcmd(f"mouse_move {val[0]} {val[1]}")
            elif op == "btn":
                qcmd(f"mouse_button {val}")
            time.sleep(0.25)
        time.sleep(0.5)

        # 6. CENÁRIO D: MOVIMENTAÇÃO CONTÍNUA E DIGITAÇÃO PROLONGADA (STRESS E SUSTENTAÇÃO)
        print("  [>>>] Executando Cenário D: Sustentação Prolongada (Stress de Movimentação Contínua e Digitação)")
        for cycle in range(6):
            qcmd(f"mouse_move {15 if cycle % 2 == 0 else -15} {15 if cycle % 2 == 0 else -15}")
            time.sleep(0.25)
            qcmd(f"sendkey {chr(ord('d') + cycle)}")
            time.sleep(0.25)
        time.sleep(1.0)

        # 7. Telemetria de IRQ e PIC no QEMU
        irq_text = qcmd("info irq")
        for line in irq_text.splitlines():
            line_str = line.strip()
            parts = line_str.split(":")
            if len(parts) >= 2:
                irq_num = parts[0].strip()
                if irq_num == "1":
                    try: res["irq1"] = int(parts[1].strip().split()[0])
                    except: pass
                elif irq_num == "12":
                    try: res["irq12"] = int(parts[1].strip().split()[0])
                    except: pass

        pic_text = qcmd("info pic")
        res["pic_info"] = pic_text.strip()

        try:
            qcmd("quit")
        except:
            pass
        time.sleep(0.5)

        # 8. Contabilização de eventos recebidos em Ring 3
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            content = f.read()

        keys = content.count("[ALADDIN-INPUT] KeyDown")
        moves = content.count("[ALADDIN-INPUT] MouseMove")
        clicks = content.count("[ALADDIN-INPUT] MouseBtn")

        res["keys"] = keys
        res["moves"] = moves
        res["clicks"] = clicks

        print(f"\n  [TELEMETRIA {mode_label}]")
        print(f"    - Eventos KeyDown (Ring 3):   {keys}")
        print(f"    - Eventos MouseMove (Ring 3): {moves}")
        print(f"    - Eventos MouseBtn (Ring 3):  {clicks}")
        print(f"    - Contagem IRQ 1 (QEMU):      {res['irq1']}")
        print(f"    - Contagem IRQ 12 (QEMU):     {res['irq12']}")
        print(f"    - Estado PIC 8259:\n{res['pic_info']}")

        # Validação de cada cenário:
        # Cenário A: mínimo 6 teclas de 'hello\n' + 2 de boot = 8
        if keys >= 8: res["scenario_a"] = True
        # Cenário B: mínimo 2 moves e 2 cliques de botão
        if moves >= 2 and clicks >= 2: res["scenario_b"] = True
        # Cenário C: fluxo simultâneo elevando total para >= 12 keys, >= 4 moves, >= 4 clicks
        if keys >= 12 and moves >= 4 and clicks >= 4: res["scenario_c"] = True
        # Cenário D: estresse contínuo comprovando sustentação prolongada de digitação e movimentação
        if keys >= 14 and moves >= 6: res["scenario_d"] = True

    finally:
        if s:
            try: s.close()
            except: pass
        if proc.poll() is None:
            proc.kill()

    return res

def main():
    print("=================================================================")
    print("  JINN OS / ALADDIN — HOMOLOGAÇÃO DE ENTRADA PS/2 (A, B, C, D)   ")
    print("=================================================================")

    res_smp4 = run_suite_for_mode(smp_cores=4, mode_label="SMP (4 Núcleos, 64 MB RAM)")
    time.sleep(2)
    res_smp1 = run_suite_for_mode(smp_cores=1, mode_label="Uniprocessador (1 Núcleo, 64 MB RAM)")

    print("\n\n=================================================================")
    print("                   MATRIZ DE RESULTADOS FINAIS                   ")
    print("=================================================================")

    for label, r in [("SMP (4 Núcleos, 64 MB)", res_smp4), ("UP (1 Núcleo, 64 MB)", res_smp1)]:
        print(f"\nAmbiente: {label}")
        print(f"  - Boot Aladdin Ring 3:           {'PASS' if r['boot'] else 'FAIL'}")
        print(f"  - Cenário A (Somente Teclado):    {'PASS' if r['scenario_a'] else 'FAIL'} ({r['keys']} teclas recebidas)")
        print(f"  - Cenário B (Somente Mouse):      {'PASS' if r['scenario_b'] else 'FAIL'} ({r['moves']} movs, {r['clicks']} cliques)")
        print(f"  - Cenário C (Teclado+Mouse Sim):  {'PASS' if r['scenario_c'] else 'FAIL'}")
        print(f"  - Cenário D (Stress Sustentado):  {'PASS' if r['scenario_d'] else 'FAIL'} ({r['keys']} teclas, {r['moves']} movs, {r['clicks']} cliques)")
        print(f"  - Telemetria IRQ:                 IRQ1={r['irq1']}, IRQ12={r['irq12']}")

    all_passed = (
        res_smp4["boot"] and res_smp4["scenario_a"] and res_smp4["scenario_b"] and res_smp4["scenario_c"] and res_smp4["scenario_d"] and
        res_smp1["boot"] and res_smp1["scenario_a"] and res_smp1["scenario_b"] and res_smp1["scenario_c"] and res_smp1["scenario_d"]
    )

    if all_passed:
        print("\n\n>>> HOMOLOGAÇÃO APROVADA: TODOS OS CENÁRIOS PASSARAM COM SUCESSO! (PASS) <<<")
        sys.exit(0)
    else:
        print("\n\n>>> HOMOLOGAÇÃO REPROVADA: FALHA EM UM OU MAIS CENÁRIOS! (FAIL) <<<")
        sys.exit(1)

if __name__ == "__main__":
    main()
