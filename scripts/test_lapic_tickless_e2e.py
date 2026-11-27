#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR Jinn-Commercial-1.0
"""
scripts/test_lapic_tickless_e2e.py

Validacao E2E do Timer LAPIC One-Shot Tickless no QEMU (-device e1000 -smp 4 -m 512M):
  1. Calibracao LAPIC via PIT Canal 2 ([APIC-CAL] one_shot=1 pit_irq0_masked=1)
  2. Janela ociosa de 5.0 segundos comprovando queda drastica de IRQs de timer
     (PIT0 delta == 0, lapic_timer_irqs reduzido vs 100 Hz periodico, idle=100.00% em APs)
  3. Despertar imediato por evento externo de rede (NIC IRQ + HTTP 10.0.2.2) e precisao de sleep (1000 ms)
  4. Captura visual real do Monitor de Recursos JAHA / Terminal em web/assets/lapic-tickless-zero-idle.png
"""

import http.server
import os
import re
import shutil
import socket
import socketserver
import struct
import subprocess
import sys
import tempfile
import threading
import time
import zlib

PROJECT_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
ISO_PATH = os.path.join(PROJECT_ROOT, "jinn.iso")
MASTER_DISK = os.path.join(PROJECT_ROOT, "disk.img")
SCREENSHOT_PNG = os.path.join(PROJECT_ROOT, "web", "assets", "lapic-tickless-zero-idle.png")


def find_qemu():
    for candidate in [
        os.path.join(PROJECT_ROOT, "tools", "qemu", "qemu-system-x86_64.exe"),
        r"C:\Program Files\qemu\qemu-system-x86_64.exe",
        r"C:\Program Files\QEMU\qemu-system-x86_64.exe",
        shutil.which("qemu-system-x86_64"),
    ]:
        if candidate and os.path.exists(candidate):
            return candidate
    raise FileNotFoundError("qemu-system-x86_64 nao encontrado")


def ppm_to_png(ppm_path, png_path):
    with open(ppm_path, "rb") as f:
        data = f.read()
    idx = 0
    tokens = []
    while len(tokens) < 4:
        while idx < len(data) and data[idx] in b" \t\r\n":
            idx += 1
        if idx < len(data) and data[idx] == ord("#"):
            while idx < len(data) and data[idx] != ord("\n"):
                idx += 1
            continue
        start = idx
        while idx < len(data) and data[idx] not in b" \t\r\n":
            idx += 1
        tokens.append(data[start:idx].decode("ascii"))
    width, height = int(tokens[1]), int(tokens[2])
    idx += 1
    raw_pixels = data[idx : idx + width * height * 3]
    raw_scanlines = bytearray()
    row_bytes = width * 3
    for y in range(height):
        raw_scanlines.append(0)
        raw_scanlines.extend(raw_pixels[y * row_bytes : (y + 1) * row_bytes])
    compressed = zlib.compress(bytes(raw_scanlines), 9)

    def make_chunk(chunk_type, chunk_data):
        return (
            struct.pack(">I", len(chunk_data))
            + chunk_type
            + chunk_data
            + struct.pack(">I", zlib.crc32(chunk_type + chunk_data) & 0xFFFFFFFF)
        )

    png_data = bytearray(b"\x89PNG\r\n\x1a\n")
    png_data.extend(make_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)))
    png_data.extend(make_chunk(b"IDAT", compressed))
    png_data.extend(make_chunk(b"IEND", b""))
    os.makedirs(os.path.dirname(png_path), exist_ok=True)
    with open(png_path, "wb") as f:
        f.write(png_data)


def qsend(sock, cmd, delay=0.14):
    sock.sendall((cmd + "\n").encode("utf-8"))
    time.sleep(delay)
    try:
        sock.recv(4096)
    except Exception:
        pass


def send_text(sock, text):
    for ch in text:
        if ch == ":":
            qsend(sock, "sendkey shift-semicolon", 0.12)
        elif ch == "/":
            qsend(sock, "sendkey slash", 0.08)
        elif ch == ".":
            qsend(sock, "sendkey dot", 0.07)
        elif ch == "-":
            qsend(sock, "sendkey minus", 0.07)
        elif ch == "_":
            qsend(sock, "sendkey shift-minus", 0.10)
        elif ch == " ":
            qsend(sock, "sendkey spc", 0.07)
        elif "A" <= ch <= "Z":
            qsend(sock, f"sendkey shift-{ch.lower()}", 0.10)
        else:
            qsend(sock, f"sendkey {ch}", 0.07)


def read_log(path):
    if not os.path.exists(path):
        return ""
    with open(path, "r", encoding="utf-8", errors="ignore") as f:
        return f.read()


def wait_for(path, needle, start=0, timeout=30.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        txt = read_log(path)
        idx = txt.find(needle, start)
        if idx != -1:
            return txt, idx
        time.sleep(0.2)
    return read_log(path), -1


def parse_stats_snapshot(chunk):
    m_tick = re.search(
        r"\[TICKLESS\] active=(\d+) lapic_timer_irqs=(\d+) oneshot_arms=(\d+) pit_irq0=(\d+)",
        chunk,
    )
    if not m_tick:
        raise ValueError(f"Marcador [TICKLESS] ausente em:\n{chunk}")
    tickless = {
        "active": int(m_tick.group(1)),
        "lapic_timer_irqs": int(m_tick.group(2)),
        "oneshot_arms": int(m_tick.group(3)),
        "pit_irq0": int(m_tick.group(4)),
    }
    cores = {}
    for m in re.finditer(
        r"cpu(\d+): total=(\d+) hlt=(\d+) task=(\d+) irq=(\d+) trans=(\d+) wake=(\d+)",
        chunk,
    ):
        cid = int(m.group(1))
        cores[cid] = {
            "total": int(m.group(2)),
            "hlt": int(m.group(3)),
            "task": int(m.group(4)),
            "irq": int(m.group(5)),
            "trans": int(m.group(6)),
            "wake": int(m.group(7)),
        }
    return tickless, cores


class ThreadedHTTPServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True


def main():
    print("=" * 84)
    print("  JINN OS ETAPA 3 — TESTE E2E TIMER LAPIC ONE-SHOT TICKLESS & AUDITORIA ZERO-IDLE")
    print("=" * 84)

    qemu_bin = find_qemu()
    tmpdir = tempfile.mkdtemp(prefix="jinn_tickless_")
    http_srv = None
    proc = None
    sock = None
    http_hits = [0]

    try:
        class WakeHandler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                http_hits[0] += 1
                body = b"<html><head><title>Tickless Wake</title></head><body><h1>NIC IRQ Wakeup OK</h1></body></html>"
                self.send_response(200)
                self.send_header("Content-Type", "text/html")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        http_srv = ThreadedHTTPServer(("127.0.0.1", 0), WakeHandler)
        http_port = http_srv.server_address[1]
        threading.Thread(target=http_srv.serve_forever, daemon=True).start()

        work_disk = os.path.join(tmpdir, "tickless_disk.img")
        shutil.copyfile(MASTER_DISK, work_disk)
        log_file = os.path.join(tmpdir, "qemu_tickless.log")
        pcap_file = os.path.join(tmpdir, "qemu_tickless.pcap")
        mon_port = 5573

        qemu_cmd = [
            qemu_bin,
            "-boot", "order=d",
            "-cdrom", ISO_PATH,
            "-drive", f"file={work_disk},format=raw,index=0,media=disk",
            "-netdev", "user,id=n0",
            "-device", "e1000,netdev=n0",
            "-object", f"filter-dump,id=f0,netdev=n0,file={pcap_file}",
            "-m", "512M",
            "-smp", "4",
            "-display", "none",
            "-serial", f"file:{log_file}",
            "-monitor", f"tcp:127.0.0.1:{mon_port},server,nowait",
        ]
        print("[QEMU] Booting Jinn OS (-device e1000, -smp 4, -m 512M)...")
        proc = subprocess.Popen(qemu_cmd)

        for _ in range(30):
            time.sleep(0.4)
            try:
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sock.settimeout(3.0)
                sock.connect(("127.0.0.1", mon_port))
                break
            except Exception:
                if sock:
                    sock.close()
                    sock = None
        if not sock:
            raise RuntimeError("Falha ao conectar ao monitor QEMU")
        time.sleep(0.4)
        try:
            sock.recv(4096)
        except Exception:
            pass

        txt, idx_gui = wait_for(log_file, "Servidor grafico em execucao", 0, timeout=45.0)
        if idx_gui == -1:
            print(txt[-2000:])
            raise RuntimeError("Servidor grafico nao inicializou em 45s")

        # Confirma calibracao LAPIC One-Shot no boot e aguarda DHCP/ARP estabilizar
        txt_arp, idx_arp = wait_for(log_file, "[NET-ARP]", 0, timeout=12.0)
        if "[APIC-CAL] one_shot=1 pit_irq0_masked=1" not in txt_arp:
            raise AssertionError("Marcador [APIC-CAL] de calibracao tickless ausente no boot!")
        for line in txt_arp.splitlines():
            if "[APIC-CAL]" in line or "[SMP]" in line:
                print(" ", line)

        # Aguarda desktop entrar em repouso profundo Zero-Idle (idle_elapsed > 300 ticks / 3s)
        time.sleep(4.0)

        def query_stats(label):
            start_off = len(read_log(log_file))
            send_text(sock, "stats")
            qsend(sock, "sendkey ret", 0.25)
            txt_s, idx_end = wait_for(log_file, "cpu3:", start_off, timeout=10.0)
            if idx_end == -1:
                raise RuntimeError(f"Timeout em stats ({label}):\n{txt_s[start_off:]}")
            time.sleep(0.5)
            chunk = read_log(log_file)[start_off:]
            return parse_stats_snapshot(chunk), len(read_log(log_file))

        def probe_f12(label):
            start_off = len(read_log(log_file))
            qsend(sock, "sendkey f12", 0.08)
            txt_s, idx_end = wait_for(log_file, "cpu3:", start_off, timeout=10.0)
            if idx_end == -1:
                raise RuntimeError(f"Timeout em F12 snapshot ({label}):\n{txt_s[start_off:]}")
            chunk = read_log(log_file)[start_off:]
            return parse_stats_snapshot(chunk), len(read_log(log_file))

        print("\n[TEST 1] Medindo janela ociosa pura de 5.0 segundos (One-Shot Tickless vs 100 Hz)...")
        (tk0, cores0), _ = probe_f12("t0")
        t_start = time.time()
        time.sleep(5.0)
        (tk1, cores1), end_t1 = probe_f12("t1")
        elapsed = time.time() - t_start

        d_lapic = tk1["lapic_timer_irqs"] - tk0["lapic_timer_irqs"]
        d_arms = tk1["oneshot_arms"] - tk0["oneshot_arms"]
        d_pit0 = tk1["pit_irq0"] - tk0["pit_irq0"]
        print(
            f"  [IDLE {elapsed:.2f}s] tickless={tk1['active']} | d_lapic_timer_irqs={d_lapic} "
            f"(vs ~2000 periodico 4 cores / 500 BSP 100Hz) | d_oneshot_arms={d_arms} | d_pit_irq0={d_pit0}"
        )

        all_cores_idle_ok = True
        zero_task_aps = 0
        bsp_idle_pct = 0.0
        for cid in sorted(cores1.keys()):
            b, a = cores0[cid], cores1[cid]
            dt = a["total"] - b["total"]
            dh = a["hlt"] - b["hlt"]
            dtsk = a["task"] - b["task"]
            dirq = a["irq"] - b["irq"]
            dwak = a["wake"] - b["wake"]
            pct = (dh * 100.0 / dt) if dt > 0 else 0.0
            if cid == 0:
                bsp_idle_pct = pct
            else:
                if dtsk == 0 and pct >= 99.99:
                    zero_task_aps += 1
            if pct < 99.5:
                all_cores_idle_ok = False
            print(
                f"  cpu{cid}: d_total={dt} d_hlt={dh} d_task={dtsk} d_irq={dirq} d_wake={dwak} idle={pct:.2f}%"
            )

        assert tk1["active"] == 1, "Tickless precisa estar ativo"
        assert d_pit0 == 0, f"PIT IRQ0 disparou em modo tickless: d_pit_irq0={d_pit0}"
        assert d_lapic <= 12, f"Muitas interrupcoes de timer em repouso: d_lapic={d_lapic}"
        assert bsp_idle_pct >= 99.5, f"BSP abaixo de 99.5% idle: {bsp_idle_pct:.2f}%"
        assert all_cores_idle_ok, "Algum nucleo ficou abaixo de 99.5% HLT em repouso"
        assert zero_task_aps >= 2, f"Esperado ao menos 2 APs com d_task=0, obtido {zero_task_aps}"

        print("\n[TEST 2] Validando sleep(1000 ms) + relogio TSC via jhcs-test...")
        send_text(sock, "spawn /disk/jhcs-test.elf")
        qsend(sock, "sendkey ret", 0.25)
        txt_j, idx_j = wait_for(
            log_file,
            "[jhcs-test] [SUCCESS] TODOS OS 8 TESTES RING 3 FORAM APROVADOS!",
            end_t1,
            timeout=25.0,
        )
        if idx_j == -1:
            print(txt_j[end_t1:])
            raise RuntimeError("jhcs-test nao concluiu em 25s")
        for line in txt_j[end_t1:].splitlines():
            if "Test 8c" in line or "[PERCPU-DELTA]" in line:
                print(" ", line)

        print(f"\n[TEST 3] Validando wakeup imediato por evento externo NIC IRQ (http://10.0.2.2:{http_port}/)...")
        cur_off = len(read_log(log_file))
        send_text(sock, "browser")
        qsend(sock, "sendkey ret", 0.25)
        txt_b, idx_wc = wait_for(log_file, "[BROWSER-INTERACTION] WINDOW_CREATED", cur_off, timeout=15.0)
        if idx_wc == -1:
            raise RuntimeError("Browser nao abriu para teste de wakeup NIC IRQ")
        time.sleep(1.5)  # Entra em repouso HLT antes do evento de rede
        cur_nav = len(read_log(log_file))
        t_wake0 = time.time()
        qsend(sock, "sendkey slash", 0.25)
        send_text(sock, f"http://10.0.2.2:{http_port}/")
        qsend(sock, "sendkey ret", 0.25)
        txt_nav, idx_nav = wait_for(log_file, "[BROWSER-INTERACTION] PAGE_LOADED", cur_nav, timeout=15.0)
        wake_ms = (time.time() - t_wake0) * 1000.0
        if idx_nav == -1:
            print(txt_nav[cur_nav:])
            raise RuntimeError("Falha ao despertar e carregar HTTP via NIC IRQ")
        print(f"  [PASS] NIC IRQ acordou o microkernel e concluiu HTTP GET em {wake_ms:.1f} ms (hits={http_hits[0]})")
        assert http_hits[0] >= 1

        qsend(sock, "sendkey q", 0.4)
        time.sleep(1.5)

        print("\n[SCREENSHOT] Atualizando terminal com 'stats' e capturando Monitor JAHA...")
        (tk2, _), _ = query_stats("screenshot")
        time.sleep(0.5)
        ppm = os.path.join(tmpdir, "lapic_tickless.ppm")
        qsend(sock, f"screendump {ppm}", 0.6)
        for _ in range(15):
            if os.path.exists(ppm) and os.path.getsize(ppm) > 10000:
                break
            time.sleep(0.2)
        ppm_to_png(ppm, SCREENSHOT_PNG)
        print(f"  [PASS] Screenshot salvo em {SCREENSHOT_PNG} (lapic_timer_irqs={tk2['lapic_timer_irqs']})")

        qsend(sock, "quit", 0.5)
        print("\n=== SUITE LAPIC ONE-SHOT TICKLESS 100% APROVADA! ===")
    finally:
        if sock:
            try:
                sock.close()
            except Exception:
                pass
        if proc and proc.poll() is None:
            proc.kill()
            proc.wait()
        if http_srv:
            try:
                http_srv.shutdown()
                http_srv.server_close()
            except Exception:
                pass
        shutil.rmtree(tmpdir, ignore_errors=True)


if __name__ == "__main__":
    main()
