import subprocess
import time
import socket
import os
import sys
import struct

def main():
    log_file = "qemu_dns_audit.log"
    pcap_file = "qemu_dns_audit.pcap"
    for f in [log_file, pcap_file]:
        if os.path.exists(f):
            try:
                os.remove(f)
            except Exception as e:
                print(f"Warning removing {f}: {e}")

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    port = 5578

    qemu_cmd = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", "jinn.iso",
        "-drive", "file=disk.img,format=raw,index=0,media=disk",
        "-netdev", "user,id=net0",
        "-device", "virtio-net-pci,netdev=net0",
        "-object", f"filter-dump,id=f0,netdev=net0,file={pcap_file}",
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
        for attempt in range(25):
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

        if not s:
            print("[FAIL] Could not connect to QEMU monitor!")
            proc.kill()
            sys.exit(1)

        def qemu_send(cmd):
            s.sendall((cmd + "\n").encode("utf-8"))
            time.sleep(0.18)
            try:
                s.recv(1024)
            except Exception:
                pass

        time.sleep(1.0)
        try:
            s.recv(4096)
        except Exception:
            pass

        print("[TEST] Waiting 36s for boot sequence and settling...")
        for sec in range(36):
            time.sleep(1.0)
            if (sec + 1) % 10 == 0:
                print(f"  ... waited {sec+1}s")

        time.sleep(2.0)

        # 1. Launch Browser
        print("[TEST] Spawning browser: 'browser' + Return")
        for ch in "browser":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")

        # Wait for window creation
        for _ in range(15):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        if "[BROWSER-INTERACTION] WINDOW_CREATED" in f.read():
                            print("  [PASS] Browser window created successfully!")
                            break
                except Exception:
                    pass

        time.sleep(2.5)

        def type_url(url):
            qemu_send("sendkey slash")
            time.sleep(0.6)
            for ch in url:
                if ch == ':':
                    qemu_send("sendkey shift-semicolon")
                elif ch == '/':
                    qemu_send("sendkey slash")
                elif ch == '.':
                    qemu_send("sendkey dot")
                elif ch == '-':
                    qemu_send("sendkey minus")
                else:
                    qemu_send(f"sendkey {ch}")
            qemu_send("sendkey ret")

        # Test A: http://example.com
        print("\n=== TEST A: http://example.com ===")
        type_url("http://example.com")
        for sec in range(25):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        content = f.read()
                        if "PAGE_LOADED target=http://example.com" in content:
                            print(f"  [PASS] example.com loaded successfully in {sec+1}s!")
                            break
                except Exception:
                    pass

        time.sleep(2.0)

        # Test B: http://google.com
        print("\n=== TEST B: http://google.com ===")
        type_url("http://google.com")
        for sec in range(25):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        content = f.read()
                        if "PAGE_LOADED target=http://google.com" in content:
                            print(f"  [PASS] google.com loaded and rendered in {sec+1}s!")
                            break
                except Exception:
                    pass

        time.sleep(2.0)

        # Test C: non-existent domain (NXDOMAIN)
        print("\n=== TEST C: non-existent domain (NXDOMAIN verification) ===")
        type_url("http://nonexistent-jinn-domain-xyz.com")
        for sec in range(20):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        content = f.read()
                        if "ERRO: Dominio inexistente (NXDOMAIN)" in content:
                            print(f"  [PASS] Non-existent domain correctly detected as NXDOMAIN in {sec+1}s!")
                            break
                except Exception:
                    pass

        time.sleep(3.0)
        qemu_send("quit")
        time.sleep(1.0)

    finally:
        if s:
            s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    print("\n=== AUDIT REPORT ===")
    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            lines = f.readlines()
            print(f"Log lines: {len(lines)}")
            for line in lines:
                if any(k in line for k in ["[DIAG-FLOW]", "[BROWSER-INTERACTION]", "Placa de Rede", "Resolvendo host"]):
                    print("  LOG:", line.strip())

    if os.path.exists(pcap_file):
        pcap_sz = os.path.getsize(pcap_file)
        print(f"\nPCAP capture: {pcap_sz} bytes captured.")
        with open(pcap_file, 'rb') as f:
            hdr = f.read(24)
            pkt_idx = 0
            while True:
                ph = f.read(16)
                if len(ph) < 16:
                    break
                s_sec, s_usec, incl, orig = struct.unpack('<IIII', ph)
                data = f.read(incl)
                pkt_idx += 1
                eth_type = struct.unpack('>H', data[12:14])[0]
                src_m = ':'.join(f'{b:02x}' for b in data[6:12])
                dst_m = ':'.join(f'{b:02x}' for b in data[0:6])
                if eth_type == 0x0800 and len(data) >= 34:
                    proto = data[23]
                    src_ip = '.'.join(str(b) for b in data[26:30])
                    dst_ip = '.'.join(str(b) for b in data[30:34])
                    if proto == 17 and len(data) >= 42:
                        sp, dp = struct.unpack('>HH', data[34:38])
                        if sp == 53 or dp == 53:
                            tx_id, flags = struct.unpack('>HH', data[42:46])
                            qr = (flags >> 15) & 1
                            rcode = flags & 0xF
                            print(f"  [PKT #{pkt_idx}] DNS {'RESP' if qr else 'QUERY'} {src_ip}:{sp}->{dst_ip}:{dp} txid={hex(tx_id)} rcode={rcode} ({src_m}->{dst_m})")
                    elif proto == 6 and len(data) >= 54:
                        sp, dp = struct.unpack('>HH', data[34:38])
                        flags = data[47]
                        f_names = []
                        if flags & 2: f_names.append("SYN")
                        if flags & 16: f_names.append("ACK")
                        if flags & 1: f_names.append("FIN")
                        if flags & 8: f_names.append("PSH")
                        if len(data) > 60:
                            print(f"  [PKT #{pkt_idx}] TCP {src_ip}:{sp}->{dst_ip}:{dp} [{'+'.join(f_names)}] len={len(data)}")
                elif eth_type == 0x0806:
                    op = struct.unpack('>H', data[20:22])[0]
                    print(f"  [PKT #{pkt_idx}] ARP op={'REQ' if op==1 else 'REPLY'} ({src_m}->{dst_m})")

if __name__ == "__main__":
    main()
