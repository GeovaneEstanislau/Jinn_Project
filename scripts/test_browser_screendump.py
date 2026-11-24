import subprocess
import socket
import time
import sys
import os
import struct
import zlib

def ppm_to_png(ppm_path, png_path):
    with open(ppm_path, "rb") as f:
        data = f.read()

    idx = 0
    tokens = []
    while len(tokens) < 4:
        while idx < len(data) and data[idx] in b" \t\r\n":
            idx += 1
        if idx < len(data) and data[idx] == ord('#'):
            while idx < len(data) and data[idx] != ord('\n'):
                idx += 1
            continue
        start = idx
        while idx < len(data) and data[idx] not in b" \t\r\n":
            idx += 1
        tokens.append(data[start:idx].decode('ascii'))

    magic = tokens[0]
    width = int(tokens[1])
    height = int(tokens[2])
    maxval = int(tokens[3])
    idx += 1

    raw_pixels = data[idx:idx + width * height * 3]
    raw_scanlines = bytearray()
    row_bytes = width * 3
    for y in range(height):
        raw_scanlines.append(0)
        raw_scanlines.extend(raw_pixels[y * row_bytes : (y + 1) * row_bytes])

    compressed = zlib.compress(bytes(raw_scanlines), 9)

    def make_chunk(chunk_type, chunk_data):
        return struct.pack(">I", len(chunk_data)) + chunk_type + chunk_data + struct.pack(">I", zlib.crc32(chunk_type + chunk_data) & 0xFFFFFFFF)

    png_data = bytearray(b"\x89PNG\r\n\x1a\n")
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    png_data.extend(make_chunk(b"IHDR", ihdr))
    png_data.extend(make_chunk(b"IDAT", compressed))
    png_data.extend(make_chunk(b"IEND", b""))

    with open(png_path, "wb") as f:
        f.write(png_data)
    print(f"[+] Imagem convertida para PNG com sucesso: {png_path} ({len(png_data)} bytes, {width}x{height})")

def main():
    log_file = r"D:\Projetos\Jinn\qemu_browser_test.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except Exception: pass

    qemu_cmd = [
        r"D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe",
        "-m", "512M",
        "-smp", "4",
        "-boot", "order=d",
        "-cdrom", r"D:\Projetos\Jinn\jinn.iso",
        "-drive", r"file=D:\Projetos\Jinn\disk.img,format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "e1000,netdev=net0",
        "-serial", f"file:{log_file}",
        "-display", "none",
        "-monitor", "tcp:127.0.0.1:55556,server,nowait"
    ]

    print("[*] Iniciando QEMU com monitor em 127.0.0.1:55556...")
    proc = subprocess.Popen(qemu_cmd)

    s = None
    connected = False
    for attempt in range(25):
        time.sleep(1.0)
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(5.0)
            s.connect(("127.0.0.1", 55556))
            connected = True
            print(f"[+] Conectado ao monitor do QEMU na tentativa {attempt+1}")
            break
        except Exception:
            if s: s.close()

    if not connected:
        print("[-] Falha ao conectar ao monitor do QEMU")
        proc.terminate()
        sys.exit(1)

    def qemu_send(cmd):
        s.sendall((cmd + "\r\n").encode("ascii"))
        time.sleep(0.15)
        try:
            s.recv(1024)
        except Exception:
            pass

    print("[*] Aguardando inicialização do desktop...")
    time.sleep(8.0)

    try:
        # Abre o navegador digitando "browser" no terminal
        print("[*] Digitando 'browser' para abrir o navegador...")
        for ch in "browser":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(3.0)

        # Foca a barra de endereço e navega para um endereço interno ou jinn-os
        print("[*] Navegador inicializado. Testando navegação...")
        qemu_send("sendkey slash")
        time.sleep(0.5)

        ppm_path = os.path.abspath(r"D:\Projetos\Jinn\qemu_desktop.ppm")
        print(f"[*] Solicitando screendump para {ppm_path}...")
        qemu_send(f"screendump {ppm_path}")
        time.sleep(2.0)

        qemu_send("quit")
        time.sleep(1.0)
    finally:
        s.close()
        proc.wait(timeout=10)

    if os.path.exists(ppm_path):
        size = os.path.getsize(ppm_path)
        print(f"[+] Screendump gerado com sucesso! Tamanho: {size} bytes")
        artifact_png = r"C:\Users\Geovane\.gemini\antigravity\brain\de11cced-2c5c-4522-b829-87ac08911e34\qemu_desktop.png"
        ppm_to_png(ppm_path, artifact_png)
    else:
        print("[-] Arquivo PPM não encontrado!")

    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            lines = f.readlines()
        print(f"--- LOG SERIAL ({len(lines)} linhas) ---")
        for line in lines[-50:]:
            print(line, end="")

if __name__ == "__main__":
    main()
