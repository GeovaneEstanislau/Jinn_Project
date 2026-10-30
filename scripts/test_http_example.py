import subprocess
import time
import socket
import os
import sys

def main():
    log_file = "qemu_http_test.log"
    pcap_file = "qemu_http_test.pcap"
    for f in [log_file, pcap_file]:
        if os.path.exists(f):
            try:
                os.remove(f)
            except Exception as e:
                print(f"Warning removing {f}: {e}")

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    port = 5561

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

    print(f"[TEST] Starting QEMU with external monitor on port {port}...")
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

        print("[TEST] Waiting 36s for boot sequence and CPU-hog tasks to exhaust quotas...")
        for sec in range(36):
            time.sleep(1.0)
            if (sec + 1) % 10 == 0:
                print(f"  ... waited {sec+1}s")

        time.sleep(2.0)

        # 1. Launch Browser from Terminal by typing 'browser' and pressing Return
        print("[TEST] Injecting keyboard input to spawn browser: 'browser' + Return")
        for ch in "browser":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")

        # Wait for browser process to spawn and create window
        print("[TEST] Waiting for browser window creation...")
        created = False
        for _ in range(15):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        if "[BROWSER-INTERACTION] WINDOW_CREATED" in f.read():
                            created = True
                            print("  [PASS] Window created detected in serial log!")
                            break
                except Exception:
                    pass

        if not created:
            print("[WARN] WINDOW_CREATED not detected yet in serial log, will continue sequence...")

        # Wait 2.5s for initial frame composition and event loop to start polling
        time.sleep(2.5)

        # 2. Test Scrolling via hardware keys (Down, Up, PageDown, PageUp)
        print("[TEST] Testing Scroll Down (Down arrow)...")
        qemu_send("sendkey down")
        time.sleep(0.6)

        print("[TEST] Testing Scroll Up (Up arrow)...")
        qemu_send("sendkey up")
        time.sleep(0.6)

        # 3. Test Address Bar Focus and Navigation via keyboard
        print("[TEST] Testing Address Bar Focus ('/')...")
        qemu_send("sendkey slash")
        time.sleep(0.8)

        print("[TEST] Typing URL 'http://example.com' into address bar and pressing Return...")
        url = "http://example.com"
        for ch in url:
            if ch == ':':
                qemu_send("sendkey shift-semicolon")
            elif ch == '/':
                qemu_send("sendkey slash")
            elif ch == '.':
                qemu_send("sendkey dot")
            else:
                qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")
        print("[TEST] Sent URL and Return! Waiting for HTTP load and rendering...")

        # Wait up to 30s for navigation, DNS resolution and HTTP rendering
        for sec in range(30):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        content = f.read()
                        if "[DIAG-FLOW] SUCESSO" in content or "Example Domain" in content:
                            print(f"[TEST] Success detected in log at {sec+1}s!")
                            break
                except Exception:
                    pass

        # Done testing
        qemu_send("quit")
        time.sleep(1.0)

    finally:
        if s:
            s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    print("[TEST] Checking log file...")
    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            lines = f.readlines()
            for line in lines[-80:]:
                print("  LOG:", line.strip())

    if os.path.exists(pcap_file):
        print(f"[TEST] PCAP file size: {os.path.getsize(pcap_file)} bytes")

if __name__ == "__main__":
    main()
