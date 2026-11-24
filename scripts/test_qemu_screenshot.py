import subprocess
import socket
import time
import sys
import os

def ppm_to_png(ppm_path, png_path):
    import struct
    import zlib
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
    qemu_cmd = [
        r"D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe",
        "-m", "512M",
        "-smp", "4",
        "-boot", "order=d",
        "-cdrom", r"D:\Projetos\Jinn\jinn.iso",
        "-drive", r"file=D:\Projetos\Jinn\disk.img,format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "e1000,netdev=net0",
        "-serial", r"file:D:\Projetos\Jinn\qemu_vfs_test.log",
        "-display", "none",
        "-monitor", "tcp:127.0.0.1:55555,server,nowait"
    ]

    print("[*] Iniciando QEMU com monitor em 127.0.0.1:55555...")
    proc = subprocess.Popen(qemu_cmd)

    s = None
    connected = False
    for attempt in range(20):
        time.sleep(1.0)
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(5.0)
            s.connect(("127.0.0.1", 55555))
            connected = True
            print(f"[+] Conectado ao monitor do QEMU na tentativa {attempt+1}")
            break
        except Exception:
            s.close()

    if not connected:
        print("[-] Falha ao conectar ao monitor do QEMU")
        proc.terminate()
        sys.exit(1)

    print("[*] Aguardando inicialização do Aladdin Desktop e montagem do VFS...")
    time.sleep(6.0)

    try:
        banner = s.recv(1024)
        print("[*] Monitor:", banner.decode("ascii", errors="ignore").strip())

        ppm_path = os.path.abspath(r"D:\Projetos\Jinn\qemu_desktop.ppm")
        print(f"[*] Solicitando screendump para {ppm_path}...")
        cmd = f"screendump {ppm_path}\r\n".encode("ascii")
        s.sendall(cmd)
        time.sleep(2.0)
        resp = s.recv(1024)
        print("[*] Resposta screendump:", resp.decode("ascii", errors="ignore").strip())

        s.sendall(b"quit\r\n")
        time.sleep(1.0)
    finally:
        s.close()
        proc.wait(timeout=10)

    if os.path.exists(ppm_path):
        size = os.path.getsize(ppm_path)
        print(f"[+] Screendump gerado com sucesso! Tamanho: {size} bytes ({ppm_path})")

        artifact_png = r"C:\Users\Geovane\.gemini\antigravity\brain\de11cced-2c5c-4522-b829-87ac08911e34\qemu_desktop.png"
        ppm_to_png(ppm_path, artifact_png)
    else:
        print("[-] Arquivo PPM de screendump não foi encontrado!")
        sys.exit(1)

if __name__ == "__main__":
    main()
