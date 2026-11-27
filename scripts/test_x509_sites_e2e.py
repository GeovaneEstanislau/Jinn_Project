#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR Jinn-Commercial-1.0
import os, shutil, socket, struct, subprocess, tempfile, time, zlib

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
    def mk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    out = bytearray(b"\x89PNG\r\n\x1a\n")
    out.extend(mk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)))
    out.extend(mk(b"IDAT", compressed))
    out.extend(mk(b"IEND", b""))
    os.makedirs(os.path.dirname(png_path), exist_ok=True)
    with open(png_path, "wb") as f:
        f.write(out)

QEMU = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
ISO = r"D:\Projetos\Jinn\jinn.iso"
DISK = r"D:\Projetos\Jinn\disk.img"

KEY_MAP = {
    '/': 'slash', ':': 'shift-semicolon', '.': 'dot', '-': 'minus', '_': 'shift-minus',
    '?': 'shift-slash', '=': 'equal', '&': 'shift-7', ' ': 'spc', '\n': 'ret',
}
for c in 'abcdefghijklmnopqrstuvwxyz':
    KEY_MAP[c] = c
for c in '0123456789':
    KEY_MAP[c] = c

def qsend(sock, cmd, wait=0.10):
    sock.sendall((cmd + "\n").encode())
    time.sleep(wait)
    try:
        sock.recv(4096)
    except Exception:
        pass

def send_text(sock, s):
    for ch in s:
        k = KEY_MAP.get(ch.lower())
        if k:
            d = 0.12 if "shift-" in k else 0.08
            qsend(sock, f"sendkey {k}", d)

def read_log(p):
    if not os.path.exists(p): return ""
    with open(p, "r", encoding="utf-8", errors="replace") as f:
        return f.read()

def wait_for(p, needle, off=0, timeout=15.0):
    t0 = time.time()
    while time.time() - t0 < timeout:
        txt = read_log(p)
        idx = txt.find(needle, off)
        if idx != -1:
            return txt, idx
        time.sleep(0.2)
    return read_log(p), -1

tmp = tempfile.mkdtemp(prefix="jinn_probe_")
work_disk = os.path.join(tmp, "disk.img")
shutil.copyfile(DISK, work_disk)
log = os.path.join(tmp, "serial.log")
proc = subprocess.Popen([
    QEMU, "-boot", "order=d", "-cdrom", ISO,
    "-drive", f"file={work_disk},format=raw,index=0,media=disk",
    "-netdev", "user,id=n0", "-device", "e1000,netdev=n0",
    "-m", "512M", "-smp", "4", "-display", "none",
    "-serial", f"file:{log}", "-monitor", "tcp:127.0.0.1:5579,server,nowait",
])
sock = None
try:
    for _ in range(30):
        time.sleep(0.3)
        try:
            sock = socket.socket()
            sock.settimeout(0.5)
            sock.connect(("127.0.0.1", 5579))
            break
        except Exception:
            sock = None
    if not sock:
        raise RuntimeError("No monitor socket")
    time.sleep(0.4)
    try: sock.recv(4096)
    except Exception: pass
    txt, idx_g = wait_for(log, "Servidor grafico em execucao", 0, 45)
    if idx_g == -1:
        raise RuntimeError("No GUI boot:\n" + txt[-1000:])
    txt_a, idx_a = wait_for(log, "[NET-ARP]", 0, 12)
    time.sleep(1.5)
    send_text(sock, "browser")
    qsend(sock, "sendkey ret", 0.3)
    txt_w, idx_w = wait_for(log, "[BROWSER-INTERACTION] WINDOW_CREATED", 0, 20)
    if idx_w == -1:
        raise RuntimeError("Browser failed to open:\n" + txt_w[-1000:])
    time.sleep(1.0)

    urls = [
        "https://google.com",
        "https://github.com",
        "https://en.wikipedia.org",
        "https://news.ycombinator.com",
        "https://example.com",
        "https://jinn-os.dev.br",
        "frogfind.com",
        "68k.news",
        "pudim.com.br",
        "https://www.kernel.org",
        "https://www.rust-lang.org",
        "https://www.cloudflare.com",
        "https://www.mozilla.org",
        "https://g1.globo.com",
    ]
    SHOT_MAP = {
        "https://google.com": r"D:\Projetos\Jinn\web\assets\browser-multisan-google.png",
        "https://jinn-os.dev.br": r"D:\Projetos\Jinn\web\assets\browser-rsapss-jinn-os.png",
        "68k.news": r"D:\Projetos\Jinn\web\assets\browser-fallback-68k.png",
    }
    passed = 0
    for u in urls:
        off = len(read_log(log))
        qsend(sock, "sendkey slash", 0.15)
        send_text(sock, u)
        qsend(sock, "sendkey ret", 0.2)
        t0 = time.time()
        res_txt = ""
        while time.time() - t0 < 18.0:
            txt = read_log(log)
            seg = txt[off:]
            if "PAGE_LOADED" in seg or "PAGE_NET_ERROR" in seg or "FORMATTED_ERROR" in seg:
                time.sleep(0.5)
                res_txt = read_log(log)[off:]
                break
            time.sleep(0.25)
        if not res_txt:
            res_txt = read_log(log)[off:]
        print(f"\n=== URL: {u} ===", flush=True)
        for line in res_txt.splitlines():
            if any(k in line for k in ("[DIAG-FLOW]", "[BROWSER", "[TLS", "[X509", "Status:", "Falha", "BLOQUEADO", "PAGE_LOADED", "PAGE_NET_ERROR", "ERR", "Redirect", "Redirec")):
                print(" ", line, flush=True)
        assert "PAGE_LOADED" in res_txt, f"Falha ao carregar {u}"
        assert "CertificateVerify: assinatura" not in res_txt, f"Falso erro RSA-PSS/CertVerify em {u}"
        assert "SAN/CN mismatch" not in res_txt, f"Falso erro SAN mismatch em {u}"
        passed += 1
        if u in SHOT_MAP:
            ppm = os.path.join(tmp, f"shot_{passed}.ppm")
            qsend(sock, f"screendump {ppm}", 0.5)
            for _ in range(15):
                if os.path.exists(ppm) and os.path.getsize(ppm) > 10000:
                    break
                time.sleep(0.2)
            ppm_to_png(ppm, SHOT_MAP[u])
            print(f"  [SHOT] {SHOT_MAP[u]}", flush=True)
    print(f"\n[SUCCESS] {passed}/{len(urls)} sites carregados com sucesso no Jinn Browser!", flush=True)
    qsend(sock, "quit", 0.3)
finally:
    if sock: sock.close()
    if proc and proc.poll() is None: proc.kill(); proc.wait()
    shutil.rmtree(tmp, ignore_errors=True)
