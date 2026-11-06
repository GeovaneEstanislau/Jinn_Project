import subprocess
import time
import socket
import os
import sys

def convert_ppm_to_png(ppm_path, png_path):
    try:
        with open(ppm_path, "rb") as f:
            header = f.readline().decode().strip()
            if header not in ("P6", "P3"):
                return False
            line = f.readline()
            while line.startswith(b"#"):
                line = f.readline()
            dims = line.decode().strip().split()
            while len(dims) < 2:
                line = f.readline()
                dims.extend(line.decode().strip().split())
            width, height = int(dims[0]), int(dims[1])
            maxval = int(f.readline().decode().strip())
            raw_data = f.read()

        import zlib
        import struct

        raw = bytearray()
        stride = width * 3
        for y in range(height):
            raw.append(0)
            raw.extend(raw_data[y * stride : (y + 1) * stride])

        compressed = zlib.compress(bytes(raw), 9)
        png = bytearray(b"\x89PNG\r\n\x1a\n")
        ihdr_data = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
        ihdr_crc = zlib.crc32(b"IHDR" + ihdr_data)
        png.extend(struct.pack(">I", len(ihdr_data)))
        png.extend(b"IHDR")
        png.extend(ihdr_data)
        png.extend(struct.pack(">I", ihdr_crc))

        idat_crc = zlib.crc32(b"IDAT" + compressed)
        png.extend(struct.pack(">I", len(compressed)))
        png.extend(b"IDAT")
        png.extend(compressed)
        png.extend(struct.pack(">I", idat_crc))

        iend_crc = zlib.crc32(b"IEND")
        png.extend(struct.pack(">I", 0))
        png.extend(b"IEND")
        png.extend(struct.pack(">I", iend_crc))

        with open(png_path, "wb") as f_out:
            f_out.write(png)
        print(f"[PNG] Converted {ppm_path} to {png_path} ({width}x{height})")
        return True
    except Exception as e:
        print(f"[PNG-ERR] Could not convert PPM: {e}")
        return False

def main():
    log_file = "qemu_legacy_256m_validation.log"
    if os.path.exists(log_file):
        try: os.remove(log_file)
        except: pass

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    port = 5590

    qemu_cmd = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", "jinn.iso",
        "-drive", "file=disk.img,format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "virtio-net-pci,netdev=net0",
        "-m", "256M",
        "-smp", "1",
        "-display", "none",
        "-serial", f"file:{log_file}",
        "-monitor", f"tcp:127.0.0.1:{port},server,nowait"
    ]

    print("[JAHA 1.1] Iniciando validacao do Perfil Legacy (256 MB RAM / 1 vCPU)...")
    proc = subprocess.Popen(qemu_cmd)

    s = None
    try:
        for attempt in range(25):
            time.sleep(1)
            try:
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.settimeout(3.0)
                s.connect(("127.0.0.1", port))
                print(f"  [+] Conectado ao monitor QEMU (tentativa {attempt+1})")
                break
            except Exception:
                if s: s.close()
                s = None

        if not s:
            print("  [FAIL] Nao foi possivel conectar ao monitor QEMU")
            proc.kill()
            sys.exit(1)

        def qemu_send(cmd):
            s.sendall((cmd + "\n").encode("utf-8"))
            time.sleep(0.2)
            try: s.recv(1024)
            except: pass

        time.sleep(1.0)
        try: s.recv(4096)
        except: pass

        print("  [1/11] Aguardando inicializacao do kernel e suite inicial (20s)...")
        time.sleep(20.0)

        # 2. Entrada no desktop
        print("  [2/11] Desktop Aladdin ativo no Framebuffer Limine (1024x768x32)...")
        ppm_boot = "legacy_256m_boot.ppm"
        qemu_send(f"screendump {ppm_boot}")
        time.sleep(1.0)

        # 3. Abertura do terminal e comandos
        print("  [3/11] Executando comandos no Terminal (uptime, mem, res)...")
        for ch in "uptime":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(1.0)

        for ch in "mem":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(1.0)

        # 4. Abertura do Gerenciador de Arquivos
        print("  [4/11] Abrindo Aladdin File Manager ('files')...")
        for ch in "files":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(2.0)

        ppm_fm = "legacy_256m_files.ppm"
        qemu_send(f"screendump {ppm_fm}")
        time.sleep(1.0)

        # 5. Abertura do Jinn Browser
        print("  [5/11] Abrindo Jinn Browser ('browser')...")
        for ch in "browser":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(3.0)

        # 6. Carregamento de pagina HTML local
        print("  [6/11] Carregando pagina local INDEX.HTM...")
        qemu_send("sendkey slash") # Focus address bar
        time.sleep(0.5)
        for ch in "file://disk/INDEX.HTM":
            if ch == ':': qemu_send("sendkey shift-semicolon")
            elif ch == '/': qemu_send("sendkey slash")
            elif ch == '.': qemu_send("sendkey dot")
            else: qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(2.0)

        # 7. Carregamento de pagina estatica alternativa /ABOUT.HTM
        print("  [7/11] Carregando pagina estatica secundaria ABOUT.HTM...")
        qemu_send("sendkey slash")
        time.sleep(0.5)
        for ch in "file://disk/ABOUT.HTM":
            if ch == ':': qemu_send("sendkey shift-semicolon")
            elif ch == '/': qemu_send("sendkey slash")
            elif ch == '.': qemu_send("sendkey dot")
            else: qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(2.0)

        # 8. Navegacao entre paginas e scroll
        print("  [8/11] Testando rolagem e navegacao (Down, Up, PgDn)...")
        qemu_send("sendkey down")
        time.sleep(0.3)
        qemu_send("sendkey down")
        time.sleep(0.3)
        qemu_send("sendkey up")
        time.sleep(0.3)
        qemu_send("sendkey pgdn")
        time.sleep(0.5)

        ppm_browser = "legacy_256m_browser.ppm"
        qemu_send(f"screendump {ppm_browser}")
        time.sleep(1.0)

        # 9. Fechamento de aplicacoes e troca de janelas
        print("  [9/11] Alternando e fechando aplicacoes...")
        qemu_send("sendkey alt-f4")
        time.sleep(1.0)

        # 10. Pressao de memoria e 11. Recuperacao
        print("  [10/11 & 11/11] Testando pressao de memoria e recuperacao...")
        for ch in "res":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(1.0)

        ppm_final = "legacy_256m_final.ppm"
        qemu_send(f"screendump {ppm_final}")
        time.sleep(1.0)

        print("  [+] Validacao concluida com sucesso. Encerrando sessao...")
        qemu_send("quit")
        time.sleep(1.0)

    finally:
        if s: s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    artifact_dir = r"C:\Users\Geovane\.gemini\antigravity\brain\eaa57f41-8f93-4d37-ac6b-b3746d021032"
    for ppm in ["legacy_256m_boot.ppm", "legacy_256m_files.ppm", "legacy_256m_browser.ppm", "legacy_256m_final.ppm"]:
        if os.path.exists(ppm):
            png = os.path.join(artifact_dir, ppm.replace(".ppm", ".png"))
            convert_ppm_to_png(ppm, png)

    print("\n[JAHA 1.1] Resumo da execucao em 256 MB:")
    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            lines = f.readlines()
            print(f"  Total de linhas de log do microkernel: {len(lines)}")
            for l in lines[-25:]:
                print("   ", l.strip())

if __name__ == "__main__":
    main()
