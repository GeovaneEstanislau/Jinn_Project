import subprocess
import time
import socket
import os
import sys

def convert_ppm_to_png(ppm_path, png_path):
    # Parse PPM (P6 binary or P3 ascii)
    with open(ppm_path, "rb") as f:
        header = f.readline().decode().strip()
        if header not in ("P6", "P3"):
            print(f"Unknown PPM header: {header}")
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
        
        if header == "P6":
            raw_data = f.read()
        else:
            raw_tokens = f.read().decode().split()
            raw_data = bytes([int(x) for x in raw_tokens])

    # Try PIL if available, or write simple uncompressed BMP/PNG
    try:
        from PIL import Image
        img = Image.frombytes("RGB", (width, height), raw_data)
        img.save(png_path)
        print(f"[PNG] Converted {ppm_path} to {png_path} via PIL")
        return True
    except ImportError:
        pass

    # Fallback: write a basic 24-bit BMP if PIL is not installed, or write PNG using python zlib
    import zlib
    import struct

    def make_png(width, height, rgb_data):
        raw = bytearray()
        stride = width * 3
        for y in range(height):
            raw.append(0) # filter type none
            raw.extend(rgb_data[y * stride : (y + 1) * stride])
        
        compressed = zlib.compress(bytes(raw), 9)
        
        png = bytearray(b"\x89PNG\r\n\x1a\n")
        
        # IHDR
        ihdr_data = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
        ihdr_crc = zlib.crc32(b"IHDR" + ihdr_data)
        png.extend(struct.pack(">I", 13))
        png.extend(b"IHDR")
        png.extend(ihdr_data)
        png.extend(struct.pack(">I", ihdr_crc))
        
        # IDAT
        png.extend(struct.pack(">I", len(compressed)))
        png.extend(b"IDAT")
        png.extend(compressed)
        png.extend(struct.pack(">I", zlib.crc32(b"IDAT" + compressed)))
        
        # IEND
        png.extend(struct.pack(">I", 0))
        png.extend(b"IEND")
        png.extend(struct.pack(">I", zlib.crc32(b"IEND")))
        
        return bytes(png)

    png_bytes = make_png(width, height, raw_data)
    with open(png_path, "wb") as f:
        f.write(png_bytes)
    print(f"[PNG] Converted {ppm_path} to {png_path} via pure-python fallback")
    return True

def main():
    log_file = "qemu_fm.log"
    if os.path.exists(log_file):
        os.remove(log_file)

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    port = 5588

    qemu_cmd = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", "jinn.iso",
        "-drive", "file=disk.img,format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "virtio-net-pci,netdev=net0",
        "-m", "512M",
        "-smp", "4",
        "-display", "none",
        "-serial", f"file:{log_file}",
        "-monitor", f"tcp:127.0.0.1:{port},server,nowait"
    ]

    print(f"[TEST] Starting QEMU on monitor port {port}...")
    proc = subprocess.Popen(qemu_cmd)

    s = None
    try:
        for attempt in range(30):
            time.sleep(1)
            try:
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.settimeout(3.0)
                s.connect(("127.0.0.1", port))
                print(f"[TEST] Connected to QEMU monitor on attempt {attempt+1}")
                break
            except Exception:
                if s:
                    s.close()
                s = None

        def qemu_send(cmd):
            s.sendall((cmd + "\n").encode("utf-8"))
            time.sleep(0.2)
            try:
                s.recv(1024)
            except Exception:
                pass

        time.sleep(1.0)
        try:
            s.recv(4096)
        except Exception:
            pass

        print("[TEST] Waiting 28s for boot and desktop initialization...")
        for sec in range(28):
            time.sleep(1.0)
            if (sec + 1) % 10 == 0:
                print(f"  ... waited {sec+1}s")

        print("[TEST] Sending 'files' command to Terminal to open Aladdin File Manager...")
        for ch in "files":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")

        time.sleep(3.0)

        ppm_path = "screen_filemanager.ppm"
        if os.path.exists(ppm_path):
            os.remove(ppm_path)

        print(f"[TEST] Capturing screenshot via screendump {ppm_path}...")
        qemu_send(f"screendump {ppm_path}")
        time.sleep(2.0)

        # Also send command 'df' to terminal
        for ch in "df":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        time.sleep(1.0)

        # Done testing
        qemu_send("quit")
        time.sleep(1.0)

    finally:
        if s:
            s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    # Convert PPM to PNG
    dest_png = r"C:\Users\Geovane\.gemini\antigravity\brain\eaa57f41-8f93-4d37-ac6b-b3746d021032\screen_filemanager.png"
    if os.path.exists("screen_filemanager.ppm"):
        convert_ppm_to_png("screen_filemanager.ppm", dest_png)
        print(f"[TEST] Successfully saved screenshot to {dest_png}")
    else:
        print("[TEST] Warning: screen_filemanager.ppm was not created")

    print("[TEST] Inspecting qemu_fm.log...")
    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            lines = f.readlines()
            print(f"[TEST] Log contains {len(lines)} lines.")
            for l in lines[-35:]:
                print("  KERNEL LOG:", l.strip())

if __name__ == "__main__":
    main()

