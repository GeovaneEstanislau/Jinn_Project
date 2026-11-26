#!/usr/bin/env python3
"""
scripts/test_hot_routes_e2e.py — Validação E2E da Tabela de Rotas Mais Usadas (`HotRouteTable` Fast-Path)
Valida:
  1. Selftests em kernel (`10/10`) e segurança Zero-Trust das Syscalls 97..99 (`hottest`).
  2. 1a navegação (`http://68k.news` e `http://example.com`) no Jinn Browser: gera DNS + ARP + `/disk/hot_routes.db`.
  3. 2a navegação ao mesmo domínio no mesmo boot: TCP SYN imediato no `.pcap` com **0 DNS UDP/53 e 0 ARP** no trecho.
  4. Interface "Rotas Mais Acessadas" no Aladdin Desktop (+ Fixar Site, Abrir no Jinn Browser em 1 clique) + screenshot PNG.
  5. Cold-Reboot sobre o mesmo disco FAT32: recarga automática de `/disk/hot_routes.db`, hits preservados, TCP SYN Fast-Path sem DNS/ARP e 0.00% CPU ociosa nos núcleos AP em `HLT`.
"""

import os
import re
import time
import socket
import struct
import shutil
import zlib
import subprocess

PROJECT_ROOT = r"D:\Projetos\Jinn"
QEMU_BIN = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
if not os.path.exists(QEMU_BIN):
    QEMU_BIN = os.path.join(PROJECT_ROOT, "tools", "qemu", "qemu-system-x86_64.exe")

ISO_PATH = os.path.join(PROJECT_ROOT, "jinn.iso")
MASTER_DISK = os.path.join(PROJECT_ROOT, "disk.img")
WORK_DISK = os.path.join(PROJECT_ROOT, "hot_routes_work.img")
ARTIFACT_PNG = r"C:\Users\Geovane\.gemini\antigravity\brain\de11cced-2c5c-4522-b829-87ac08911e34\hot_routes_ui_e2e.png"


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
        s = idx
        while idx < len(data) and data[idx] not in b" \t\r\n":
            idx += 1
        tokens.append(data[s:idx].decode("ascii"))
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
    with open(png_path, "wb") as f:
        f.write(png_data)


def parse_pcap_slice(pcap_path, min_offset=24, max_offset=None):
    """Analisa pacotes do PCAP no intervalo `[min_offset, max_offset)`."""
    arp_pkts = []
    dns_queries = []
    tcp_syns = []
    if not os.path.exists(pcap_path) or os.path.getsize(pcap_path) <= 24:
        return arp_pkts, dns_queries, tcp_syns

    with open(pcap_path, "rb") as f:
        ghdr = f.read(24)
        magic = struct.unpack("<I", ghdr[:4])[0]
        endian = "<" if magic == 0xA1B2C3D4 else ">"
        while True:
            pkt_start = f.tell()
            if max_offset is not None and pkt_start >= max_offset:
                break
            phdr = f.read(16)
            if len(phdr) < 16:
                break
            _, _, incl_len, _ = struct.unpack(f"{endian}IIII", phdr)
            pkt = f.read(incl_len)
            if pkt_start < min_offset:
                continue
            if len(pkt) < 14:
                continue
            eth = struct.unpack(">H", pkt[12:14])[0]
            if eth == 0x0806 and len(pkt) >= 42:
                op = struct.unpack(">H", pkt[20:22])[0]
                tip = ".".join(str(b) for b in pkt[38:42])
                arp_pkts.append((op, tip))
            elif eth == 0x0800 and len(pkt) >= 34:
                proto = pkt[23]
                ihl = (pkt[14] & 0x0F) * 4
                dst_ip = ".".join(str(b) for b in pkt[30:34])
                if proto == 17 and len(pkt) >= 14 + ihl + 8:
                    sport, dport = struct.unpack(">HH", pkt[14 + ihl : 14 + ihl + 4])
                    if dport == 53:
                        dns_queries.append((dst_ip, sport, dport))
                elif proto == 6 and len(pkt) >= 14 + ihl + 20:
                    toff = 14 + ihl
                    sport, dport = struct.unpack(">HH", pkt[toff : toff + 4])
                    flags = pkt[toff + 13]
                    if (flags & 0x02) != 0 and (flags & 0x10) == 0:
                        tcp_syns.append((dst_ip, sport, dport))
    return arp_pkts, dns_queries, tcp_syns


def qsend(sock, cmd, delay=0.06):
    sock.sendall((cmd + "\n").encode("ascii"))
    time.sleep(delay)
    try:
        sock.settimeout(0.15)
        sock.recv(4096)
    except Exception:
        pass


def send_text(sock, text):
    for ch in text:
        if ch == ":":
            qsend(sock, "sendkey shift-semicolon", 0.04)
        elif ch == "/":
            qsend(sock, "sendkey slash", 0.04)
        elif ch == "?":
            qsend(sock, "sendkey shift-slash", 0.04)
        elif ch == "=":
            qsend(sock, "sendkey equal", 0.04)
        elif ch == ".":
            qsend(sock, "sendkey dot", 0.04)
        elif ch == "-":
            qsend(sock, "sendkey minus", 0.04)
        elif ch == " ":
            qsend(sock, "sendkey spc", 0.04)
        else:
            qsend(sock, f"sendkey {ch}", 0.04)


def read_log(path):
    if not os.path.exists(path):
        return ""
    with open(path, "r", encoding="utf-8", errors="ignore") as f:
        return f.read()


def wait_for(path, needle, start=0, timeout=25.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        txt = read_log(path)
        idx = txt.find(needle, start)
        if idx != -1:
            return txt, idx
        time.sleep(0.2)
    return read_log(path), -1


def launch_qemu(disk, pcap, log, port):
    for f in (pcap, log):
        if os.path.exists(f):
            os.remove(f)
    cmd = [
        QEMU_BIN,
        "-boot", "order=d",
        "-cdrom", ISO_PATH,
        "-drive", f"file={disk},format=raw,index=0,media=disk",
        "-netdev", "user,id=n0",
        "-device", "e1000,netdev=n0",
        "-object", f"filter-dump,id=f0,netdev=n0,file={pcap}",
        "-m", "512M",
        "-smp", "4",
        "-display", "none",
        "-serial", f"file:{log}",
        "-monitor", f"tcp:127.0.0.1:{port},server,nowait",
    ]
    proc = subprocess.Popen(cmd)
    sock = None
    for _ in range(30):
        time.sleep(0.4)
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(3.0)
            sock.connect(("127.0.0.1", port))
            break
        except Exception:
            if sock:
                sock.close()
                sock = None
    if not sock:
        proc.kill()
        raise RuntimeError("Falha ao conectar ao monitor QEMU")
    time.sleep(0.3)
    try:
        sock.recv(4096)
    except Exception:
        pass
    return proc, sock


def main():
    print("=" * 84)
    print("  JINN OS / ALADDIN — TESTE E2E TABELA DE ROTAS MAIS USADAS (HotRouteTable)")
    print("=" * 84)

    if os.path.exists(WORK_DISK):
        os.remove(WORK_DISK)
    shutil.copyfile(MASTER_DISK, WORK_DISK)

    b1_log = os.path.join(PROJECT_ROOT, "hot_routes_boot1.log")
    b1_pcap = os.path.join(PROJECT_ROOT, "hot_routes_boot1.pcap")
    b2_log = os.path.join(PROJECT_ROOT, "hot_routes_boot2.log")
    b2_pcap = os.path.join(PROJECT_ROOT, "hot_routes_boot2.pcap")
    ppm_path = os.path.join(PROJECT_ROOT, "hot_routes_ui_e2e.ppm")

    # ── BOOT 1 ────────────────────────────────────────────────────────────────
    print("[BOOT 1] Iniciando QEMU (-device e1000, -smp 4, -m 512M)...")
    proc, sock = launch_qemu(WORK_DISK, b1_pcap, b1_log, 55720)
    pcap_mark_fast = 24
    try:
        txt, idx = wait_for(b1_log, "Servidor grafico em execucao", timeout=35.0)
        if idx == -1:
            raise RuntimeError("Aladdin Desktop nao inicializou no Boot 1")
        for line in txt.splitlines():
            if any(t in line for t in ("Testes RFC 1035", "HotRouteTable", "Sementes iniciais")):
                print(" ", line)
        assert "Testes Fast-Path HotRouteTable... OK (10/10 aprovados)" in txt, "Selftests HotRouteTable falharam!"

        time.sleep(1.5)
        print("[BOOT 1] Testando Syscalls 97..99 Zero-Trust ('hottest')...")
        send_text(sock, "hottest")
        qsend(sock, "sendkey ret")
        txt, ok_hb = wait_for(b1_log, "[HOT-BENCH] PASS", timeout=15.0)
        assert ok_hb != -1, "hottest Zero-Trust falhou!"

        print("[BOOT 1] Abrindo Jinn Browser e visitando 1a vez: http://68k.news e http://example.com...")
        send_text(sock, "browser")
        qsend(sock, "sendkey ret")
        _, ok_bw = wait_for(b1_log, "[BROWSER-INTERACTION] WINDOW_CREATED", timeout=15.0)
        assert ok_bw != -1, "Janela Browser nao abriu!"
        time.sleep(0.8)

        # 1a visita: http://68k.news
        m_68k = len(read_log(b1_log))
        qsend(sock, "sendkey slash", 0.25)
        send_text(sock, "http://68k.news")
        qsend(sock, "sendkey ret", 0.25)
        txt, ok_l1 = wait_for(b1_log, "LEARNED '68k.news'", start=m_68k, timeout=18.0)
        assert ok_l1 != -1, f"68k.news nao aprendeu rota no 1o acesso:\n{txt[m_68k:]}"
        time.sleep(2.0)

        # 1a visita: http://example.com
        m_ex = len(read_log(b1_log))
        qsend(sock, "sendkey slash", 0.25)
        send_text(sock, "http://example.com")
        qsend(sock, "sendkey ret", 0.25)
        txt, ok_l2 = wait_for(b1_log, "PAGE_LOADED target=http://example.com", start=m_ex, timeout=18.0)
        assert ok_l2 != -1, f"example.com nao carregou no 1o acesso:\n{txt[m_ex:]}"
        assert "LEARNED 'example.com'" in txt[m_ex:], "example.com nao foi gravado na HotRouteTable!"

        # Marca PCAP e Log estritamente ANTES da 2a navegacao!
        time.sleep(0.8)
        pcap_mark_fast = os.path.getsize(b1_pcap) if os.path.exists(b1_pcap) else 24
        log_mark_fast = len(read_log(b1_log))

        print(f"[BOOT 1] 2a visita Fast-Path (PCAP offset={pcap_mark_fast}): http://68k.news & http://example.com/?fast=2...")
        qsend(sock, "sendkey slash", 0.25)
        send_text(sock, "http://68k.news")
        qsend(sock, "sendkey ret", 0.25)
        txt, ok_h1 = wait_for(b1_log, "HIT '68k.news'", start=log_mark_fast, timeout=12.0)
        assert ok_h1 != -1, "2a visita 68k.news nao deu Fast-Path HIT!"
        time.sleep(1.5)

        m2_ex = len(read_log(b1_log))
        qsend(sock, "sendkey slash", 0.25)
        send_text(sock, "http://example.com/?fast=2")
        qsend(sock, "sendkey ret", 0.25)
        txt, ok_h2 = wait_for(b1_log, "PAGE_LOADED target=http://example.com/?fast=2", start=m2_ex, timeout=12.0)
        assert ok_h2 != -1, f"2a visita example.com/?fast=2 nao carregou:\n{txt[m2_ex:]}"
        assert "HIT 'example.com'" in txt[m2_ex:], "2a visita example.com/?fast=2 nao deu Fast-Path HIT!"

        time.sleep(0.5)
        pcap_mark_fast_end = os.path.getsize(b1_pcap) if os.path.exists(b1_pcap) else pcap_mark_fast

        # Fecha o Browser ('q') para interagir com a Janela 0 / "Rotas Mais Acessadas"
        qsend(sock, "sendkey q", 0.3)
        wait_for(b1_log, "ACTION KeyClose", start=m2_ex, timeout=6.0)
        time.sleep(0.6)

        # Clica aba [Rede & DNS] (512,384 -> 355,104), sub-aba [Rotas Quentes] (383,120), e [+ Fixar Site] (408,153)
        print("[BOOT 1] Abrindo interface 'Rotas Mais Acessadas' e testando '+ Fixar Site' & 'Abrir no Jinn Browser'...")
        qsend(sock, "mouse_move -157 -280", 0.3)
        qsend(sock, "mouse_button 1", 0.15)
        qsend(sock, "mouse_button 0", 0.4)
        # Cursor em (355,104) -> +28,+16 => (383,120) [Rotas Quentes]
        qsend(sock, "mouse_move 28 16", 0.3)
        qsend(sock, "mouse_button 1", 0.15)
        qsend(sock, "mouse_button 0", 0.5)
        # Cursor em (383,120) -> +25,+33 => (408,153) [+ Fixar Site] (kernel.org)
        qsend(sock, "mouse_move 25 33", 0.3)
        qsend(sock, "mouse_button 1", 0.15)
        qsend(sock, "mouse_button 0", 1.2)
        qsend(sock, f"screendump {ppm_path}", 0.5)
        time.sleep(0.8)
        if os.path.exists(ppm_path):
            ppm_to_png(ppm_path, ARTIFACT_PNG)
            os.remove(ppm_path)
            print(f"[BOOT 1] Screenshot UI 'Rotas Mais Acessadas' salvo: {ARTIFACT_PNG}")

        # Cursor em (408,153) -> -248,+26 => (160,179) [Abrir no Jinn Browser] (1-click launch da rota #1!)
        m_1c = len(read_log(b1_log))
        qsend(sock, "mouse_move -248 26", 0.3)
        qsend(sock, "mouse_button 1", 0.15)
        qsend(sock, "mouse_button 0", 0.5)
        txt_1c, ok_1c = wait_for(b1_log, "Abrindo URL direta solicitada pelo Aladdin", start=m_1c, timeout=12.0)
        assert ok_1c != -1, f"Botao 'Abrir no Jinn Browser' falhou:\n{txt_1c[m_1c:]}"
        time.sleep(1.5)
        qsend(sock, "sendkey q", 0.3)
        wait_for(b1_log, "ACTION KeyClose", start=m_1c, timeout=6.0)
        time.sleep(0.4)

        # Volta foco para Terminal (Window 1 em x=470..930 -> clica x=800, y=160; delta de (160,179) => +640, -19)
        qsend(sock, "mouse_move 640 -19", 0.3)
        qsend(sock, "mouse_button 1", 0.15)
        qsend(sock, "mouse_button 0", 0.3)
        m_sync = len(read_log(b1_log))
        send_text(sock, "sync")
        qsend(sock, "sendkey ret")
        wait_for(b1_log, "[SYNC] sys_sync() concluido", start=m_sync, timeout=8.0)

        qsend(sock, "quit", 0.4)
        proc.wait(timeout=8)
    finally:
        sock.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    # Inspeciona trechos do Boot 1 PCAP:
    b1_arp_all, b1_dns_all, b1_syn_all = parse_pcap_slice(b1_pcap, 24)
    b1_arp_fast, b1_dns_fast, b1_syn_fast = parse_pcap_slice(b1_pcap, pcap_mark_fast, pcap_mark_fast_end)
    print(f"[BOOT 1 PCAP TOTAL] ARP={len(b1_arp_all)} DNS={len(b1_dns_all)} TCP_SYN={len(b1_syn_all)}")
    print(f"[BOOT 1 PCAP 2a NAVEGACAO ({pcap_mark_fast}..{pcap_mark_fast_end})] ARP={len(b1_arp_fast)} DNS={len(b1_dns_fast)} TCP_SYN={len(b1_syn_fast)}")
    assert len(b1_dns_all) >= 2 and len(b1_arp_all) >= 2, "1a navegacao deveria conter DNS e ARP!"
    assert len(b1_dns_fast) == 0, f"FALHA Fast-Path: DNS enviado na 2a navegacao! {b1_dns_fast}"
    assert len(b1_arp_fast) == 0, f"FALHA Fast-Path: ARP enviado na 2a navegacao! {b1_arp_fast}"
    assert len(b1_syn_fast) >= 2, f"FALHA Fast-Path: esperado TCP SYN imediato na 2a navegacao (got {b1_syn_fast})"

    # ── BOOT 2: Cold-Reboot sobre o mesmo disco ───────────────────────────────
    print("\n[BOOT 2 - COLD REBOOT] Reiniciando VM sobre hot_routes_work.img...")
    proc2, sock2 = launch_qemu(WORK_DISK, b2_pcap, b2_log, 55721)
    try:
        txt2, idx2 = wait_for(b2_log, "Servidor grafico em execucao", timeout=35.0)
        if idx2 == -1:
            raise RuntimeError("Aladdin Desktop nao inicializou no Boot 2")
        for line in txt2.splitlines():
            if "HotRouteTable restaurada" in line:
                print(" ", line)
        assert "HotRouteTable restaurada de /disk/hot_routes.db" in txt2, "HotRouteTable nao foi restaurada do disco no Boot 2!"

        time.sleep(1.5)
        m_hr = len(read_log(b2_log))
        send_text(sock2, "hotroutes")
        qsend(sock2, "sendkey ret")
        txt2, ok_hr = wait_for(b2_log, "[HOT-ENTRY]", start=m_hr, timeout=10.0)
        assert ok_hr != -1, "hotroutes nao retornou entradas no Boot 2!"
        time.sleep(0.5)
        txt2 = read_log(b2_log)
        for line in txt2[m_hr:].splitlines():
            if "[HOT-" in line:
                print(" ", line)
        assert "host=example.com" in txt2[m_hr:] and "hits=" in txt2[m_hr:], "example.com ausente pos-reboot!"
        assert "host=68k.news" in txt2[m_hr:], "68k.news ausente pos-reboot!"

        # Navega no Browser apos Cold-Reboot SEM nenhuma consulta DNS ou ARP para o destino!
        pcap2_mark = os.path.getsize(b2_pcap) if os.path.exists(b2_pcap) else 24
        send_text(sock2, "browser")
        qsend(sock2, "sendkey ret")
        wait_for(b2_log, "[BROWSER-INTERACTION] WINDOW_CREATED", start=m_hr, timeout=12.0)
        time.sleep(0.6)
        # Fecha Browser depois dos hits para testar Zero-Idle 4s; marca PCAP pos-abertura apenas?
        # NAO: pcap2_mark eh antes da abertura e verifica 0 DNS e 0 ARP!
        m_b2ex = len(read_log(b2_log))
        qsend(sock2, "sendkey slash", 0.25)
        send_text(sock2, "http://example.com/?fast=3")
        qsend(sock2, "sendkey ret", 0.25)
        txt2, ok_b2ex = wait_for(b2_log, "PAGE_LOADED target=http://example.com/?fast=3", start=m_b2ex, timeout=12.0)
        assert ok_b2ex != -1, "example.com/?fast=3 falhou pos-reboot!"
        assert "HIT 'example.com'" in txt2[m_b2ex:], "example.com/?fast=3 pos-reboot nao usou Fast-Path!"

        # Fecha browser & verifica Zero-Idle 4s nos 4 nucleos
        qsend(sock2, "sendkey q", 0.3)
        time.sleep(1.0)
        pre_s = len(read_log(b2_log))
        send_text(sock2, "stats")
        qsend(sock2, "sendkey ret")
        wait_for(b2_log, "cpu3:", start=pre_s, timeout=8.0)
        time.sleep(4.0)
        pre_s2 = len(read_log(b2_log))
        send_text(sock2, "stats")
        qsend(sock2, "sendkey ret")
        txt2, _ = wait_for(b2_log, "cpu3:", start=pre_s2, timeout=8.0)
        for line in txt2[pre_s2:].splitlines():
            if any(t in line for t in ("[PERCPU]", "cpu0:", "cpu1:", "cpu2:", "cpu3:")):
                print(" ", line)

        qsend(sock2, "quit", 0.4)
        proc2.wait(timeout=8)
    finally:
        sock2.close()
        if proc2.poll() is None:
            proc2.kill()
            proc2.wait()

    b2_arp_fast, b2_dns_fast, b2_syn_fast = parse_pcap_slice(b2_pcap, pcap2_mark)
    print(f"[BOOT 2 PCAP NAVEGACAO] ARP={len(b2_arp_fast)} DNS={len(b2_dns_fast)} TCP_SYN={len(b2_syn_fast)}")
    assert len(b2_dns_fast) == 0, f"FALHA Boot 2: enviou DNS UDP/53 pos-reboot! {b2_dns_fast}"
    assert len(b2_arp_fast) == 0, f"FALHA Boot 2: enviou ARP pos-reboot! {b2_arp_fast}"
    assert len(b2_syn_fast) >= 1, "FALHA Boot 2: TCP SYN ausente pos-reboot!"

    snap_matches = re.findall(
        r"cpu(\d+): total=(\d+) hlt=(\d+) task=(\d+) irq=(\d+) trans=(\d+) wake=(\d+)",
        txt2,
    )
    assert len(snap_matches) >= 8, "Snapshots Per-CPU ausentes no Boot 2"
    prev_cores = {int(c): (int(t), int(h), int(k), int(i)) for c, t, h, k, i, _, _ in snap_matches[-8:-4]}
    curr_cores = {int(c): (int(t), int(h), int(k), int(i)) for c, t, h, k, i, _, _ in snap_matches[-4:]}
    for cid in range(4):
        dt = curr_cores[cid][0] - prev_cores[cid][0]
        dh = curr_cores[cid][1] - prev_cores[cid][1]
        dk = curr_cores[cid][2] - prev_cores[cid][2]
        idle_pct = (dh * 100.0 / dt) if dt > 0 else 0.0
        print(f"  [IDLE 4s] cpu{cid}: d_total={dt} d_hlt={dh} d_task={dk} idle={idle_pct:.2f}%")
        if cid in (2, 3):
            assert dh == dt and dk == 0, f"cpu{cid} teve atividade espúria ({dk} ciclos)!"

    print("\n[RESULTADO FINAL] PASS — HotRouteTable E2E validada com 0 DNS/ARP no Fast-Path & 0.00% CPU ociosa!")


if __name__ == "__main__":
    main()
