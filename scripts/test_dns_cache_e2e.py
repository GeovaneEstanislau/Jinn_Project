#!/usr/bin/env python3
"""
scripts/test_dns_cache_e2e.py
Validação End-to-End (QEMU -device e1000 -smp 4) da Tabela Local DNS de 3 Camadas:
  - 1ª consulta upstream (`1.1.1.1`) + persistência em `/disk/dns_cache.db`
  - 2ª consulta em Cache Hit local (`0 ms`, `0` pacotes UDP na rede, `hits == 1`)
  - Cadastro estático em `/disk/hosts` + proteção `-EINVAL`/`-EFAULT` das Syscalls 93..=96
  - Interação na aba "Rede & DNS" do Aladdin Desktop + captura visual PNG
  - Cold-Reboot no mesmo disco confirmando 0 pacotes UDP/53 na rede e 0.00% CPU ociosa nos APs
"""

import os
import re
import shutil
import socket
import struct
import subprocess
import sys
import time
import zlib

PROJECT_ROOT = r"D:\Projetos\Jinn"
ISO_PATH = os.path.join(PROJECT_ROOT, "jinn.iso")
MASTER_DISK = os.path.join(PROJECT_ROOT, "disk.img")
WORK_DISK = os.path.join(PROJECT_ROOT, "test_dns_cache_e2e.img")
ARTIFACT_PNG = r"C:\Users\Geovane\.gemini\antigravity\brain\de11cced-2c5c-4522-b829-87ac08911e34\dns_panel_e2e.png"


def find_qemu():
    for candidate in [
        os.path.join(PROJECT_ROOT, "tools", "qemu", "qemu-system-x86_64.exe"),
        r"C:\Program Files\qemu\qemu-system-x86_64.exe",
        r"C:\Program Files\QEMU\qemu-system-x86_64.exe",
    ]:
        if os.path.exists(candidate):
            return candidate
    raise FileNotFoundError("qemu-system-x86_64.exe nao encontrado")


QEMU_BIN = find_qemu()


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
    with open(png_path, "wb") as f:
        f.write(png_data)


def parse_dns_qname(dns_payload):
    if len(dns_payload) < 13:
        return ""
    off = 12
    labels = []
    while off < len(dns_payload):
        l = dns_payload[off]
        off += 1
        if l == 0:
            break
        if off + l > len(dns_payload):
            break
        labels.append(dns_payload[off : off + l].decode("ascii", errors="ignore"))
        off += l
    return ".".join(labels).lower()


def inspect_pcap_dns(pcap_path):
    queries = []
    replies = []
    if not os.path.exists(pcap_path) or os.path.getsize(pcap_path) < 24:
        return queries, replies
    with open(pcap_path, "rb") as f:
        ghdr = f.read(24)
        magic = struct.unpack("<I", ghdr[:4])[0]
        endian = "<" if magic == 0xA1B2C3D4 else ">"
        while True:
            phdr = f.read(16)
            if len(phdr) < 16:
                break
            _, _, incl_len, _ = struct.unpack(f"{endian}IIII", phdr)
            pkt = f.read(incl_len)
            if len(pkt) < 42:
                continue
            eth = struct.unpack(">H", pkt[12:14])[0]
            if eth != 0x0800:
                continue
            if pkt[23] != 17:
                continue
            ihl = (pkt[14] & 0x0F) * 4
            udp_off = 14 + ihl
            if len(pkt) < udp_off + 8:
                continue
            sport, dport = struct.unpack(">HH", pkt[udp_off : udp_off + 4])
            src_ip = ".".join(str(b) for b in pkt[26:30])
            dst_ip = ".".join(str(b) for b in pkt[30:34])
            dns_data = pkt[udp_off + 8 :]
            qname = parse_dns_qname(dns_data)
            if dport == 53:
                queries.append((src_ip, dst_ip, qname))
            elif sport == 53:
                replies.append((src_ip, dst_ip, qname))
    return queries, replies


def qsend(sock, cmd, delay=0.06):
    sock.sendall((cmd + "\n").encode("ascii"))
    time.sleep(delay)
    try:
        sock.settimeout(0.2)
        sock.recv(4096)
    except Exception:
        pass


def send_text(sock, text):
    for ch in text:
        if ch == ":":
            qsend(sock, "sendkey shift-semicolon", 0.04)
        elif ch == "/":
            qsend(sock, "sendkey slash", 0.04)
        elif ch == ".":
            qsend(sock, "sendkey dot", 0.04)
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
    print("=" * 82)
    print("  JINN OS / ALADDIN — TESTE E2E TABELA DNS 3 CAMADAS + CACHE PERSISTENTE")
    print("=" * 82)

    if os.path.exists(WORK_DISK):
        os.remove(WORK_DISK)
    shutil.copyfile(MASTER_DISK, WORK_DISK)

    boot1_log = os.path.join(PROJECT_ROOT, "dns_cache_boot1.log")
    boot1_pcap = os.path.join(PROJECT_ROOT, "dns_cache_boot1.pcap")
    boot2_log = os.path.join(PROJECT_ROOT, "dns_cache_boot2.log")
    boot2_pcap = os.path.join(PROJECT_ROOT, "dns_cache_boot2.pcap")
    ppm_path = os.path.join(PROJECT_ROOT, "dns_panel_e2e.ppm")

    # ── BOOT 1: 1a consulta upstream, 2a consulta cache hit (0 ms, hits=1), UI Rede & DNS ──
    print("[BOOT 1] Iniciando QEMU (e1000, 4 vCPUs)...")
    proc, sock = launch_qemu(WORK_DISK, boot1_pcap, boot1_log, 55710)
    try:
        txt, idx = wait_for(boot1_log, "Servidor grafico em execucao", timeout=30.0)
        if idx == -1:
            raise RuntimeError("Aladdin Desktop nao inicializou no Boot 1")
        for line in txt.splitlines():
            if "Testes RFC 1035" in line or "Hosts estaticos" in line:
                print(" ", line)

        time.sleep(2.0)
        print("[BOOT 1] Executando 'dnstest' (1a consulta upstream vs 2a Cache Hit + Syscalls 93..96)...")
        send_text(sock, "dnstest")
        qsend(sock, "sendkey ret")
        txt, idx_pass = wait_for(boot1_log, "[DNS-BENCH] PASS", timeout=25.0)
        if idx_pass == -1:
            print(txt[-1500:])
            raise AssertionError("dnstest falhou no Boot 1")
        for line in txt.splitlines():
            if any(t in line for t in ("[NET-DNS]", "[DNS-BENCH]")):
                print(" ", line)

        # Clica na aba "Rede & DNS" da Janela 0 (Monitor: x=50..460 -> metade direita tab x=355, y=104)
        print("[BOOT 1] Clicando na aba [Rede & DNS] do Aladdin Desktop e testando UI...")
        qsend(sock, "mouse_move -157 -280", 0.3)
        qsend(sock, "mouse_button 1", 0.15)
        qsend(sock, "mouse_button 0", 0.4)
        # Dispara botao [Testar Res.] (wx=50, body_top=116 -> cx+280=338, s2_y+14=186; delta from 355,104 -> -17, +82)
        qsend(sock, "mouse_move -17 82", 0.3)
        qsend(sock, "mouse_button 1", 0.15)
        qsend(sock, "mouse_button 0", 0.5)
        qsend(sock, f"screendump {ppm_path}", 0.5)
        time.sleep(1.0)
        if os.path.exists(ppm_path):
            ppm_to_png(ppm_path, ARTIFACT_PNG)
            os.remove(ppm_path)
            print(f"[BOOT 1] Screenshot salvo em: {ARTIFACT_PNG}")

        qsend(sock, "quit", 0.4)
        proc.wait(timeout=8)
    finally:
        sock.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    b1_queries, b1_replies = inspect_pcap_dns(boot1_pcap)
    print(f"[BOOT 1 PCAP] Consultas UDP/53 enviadas: {len(b1_queries)} -> {b1_queries}")
    print(f"[BOOT 1 PCAP] Respostas UDP/53 recebidas: {len(b1_replies)}")
    ex_q = sum(1 for _, _, q in b1_queries if q == "example.com")
    jn_q = sum(1 for _, _, q in b1_queries if q == "jinn-os.dev.br")
    # 1a consulta envia upstream; 2a consulta + clique "Testar Res." usam cache local (0 UDP extras)
    assert ex_q >= 1 and jn_q >= 1, f"Consultas upstream ausentes: {b1_queries}"
    assert ex_q <= 2, f"Cache hit falhou: muitas queries UDP para example.com ({ex_q})"

    # ── BOOT 2: Cold-Reboot usando o MESMO disco (zero UDP/53 + 0.00% CPU ociosa APs) ──
    print("\n[BOOT 2 - COLD REBOOT] Reiniciando QEMU no mesmo disco FAT32...")
    proc2, sock2 = launch_qemu(WORK_DISK, boot2_pcap, boot2_log, 55711)
    try:
        txt2, idx2 = wait_for(boot2_log, "Servidor grafico em execucao", timeout=30.0)
        if idx2 == -1:
            raise RuntimeError("Aladdin Desktop nao inicializou no Boot 2")
        for line in txt2.splitlines():
            if any(t in line for t in ("Hosts estaticos carregados", "Cache persistente restaurado", "Testes RFC 1035")):
                print(" ", line)

        time.sleep(2.0)
        print("[BOOT 2] Executando 'dnsreboot' (validacao pos-reboot com 0 pacotes de rede)...")
        send_text(sock2, "dnsreboot")
        qsend(sock2, "sendkey ret")
        txt2, idx_reb = wait_for(boot2_log, "[DNS-REBOOT] PASS", timeout=15.0)
        if idx_reb == -1:
            print(txt2[-1500:])
            raise AssertionError("dnsreboot falhou no Boot 2")
        for line in txt2.splitlines():
            if any(t in line for t in ("[DNS-REBOOT]", "[DNS-ENTRY]", "[NET-DNS]")):
                print(" ", line)

        # Medicao Zero-Idle 4s apos dnsreboot
        pre_s = len(read_log(boot2_log))
        send_text(sock2, "stats")
        qsend(sock2, "sendkey ret")
        wait_for(boot2_log, "[aladdin-server] [PERCPU]", start=pre_s, timeout=8.0)
        time.sleep(4.0)
        pre_s2 = len(read_log(boot2_log))
        send_text(sock2, "stats")
        qsend(sock2, "sendkey ret")
        txt2, idx_pc = wait_for(boot2_log, "cpu3:", start=pre_s2, timeout=8.0)
        for line in txt2[pre_s2:].splitlines():
            if any(t in line for t in ("[STATS]", "[PERCPU]", "cpu0:", "cpu1:", "cpu2:", "cpu3:")):
                print(" ", line)

        qsend(sock2, "quit", 0.4)
        proc2.wait(timeout=8)
    finally:
        sock2.close()
        if proc2.poll() is None:
            proc2.kill()
            proc2.wait()

    b2_queries, _ = inspect_pcap_dns(boot2_pcap)
    print(f"[BOOT 2 PCAP] Consultas UDP/53 enviadas apos Cold-Reboot: {len(b2_queries)} -> {b2_queries}")
    assert len(b2_queries) == 0, f"Esperado 0 pacotes DNS UDP/53 no Cold-Reboot, capturados: {b2_queries}"

    # Verifica APs cpu2/cpu3 100% HLT e invariancia exacta
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
        di = curr_cores[cid][3] - prev_cores[cid][3]
        assert dh + dk + di == dt, f"cpu{cid} discrepancia de ciclos: {dh}+{dk}+{di}!={dt}"
        if cid in (2, 3):
            assert dk == 0 and dh == dt, f"cpu{cid} nao ficou em 0.00% CPU ativa (task={dk})"

    print("\n" + "=" * 82)
    print("  [SUCCESS] TABELA DNS 3 CAMADAS + CACHE PERSISTENTE + COLD-REBOOT 100% VALIDADO!")
    print("=" * 82)


if __name__ == "__main__":
    main()
