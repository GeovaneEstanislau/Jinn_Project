import subprocess
import time
import socket
import os
import sys

def main():
    log_file = "qemu_serial.log"
    if os.path.exists(log_file):
        try:
            os.remove(log_file)
        except Exception as e:
            print(f"Warning removing log: {e}")

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    port = 5555

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

    print(f"[TEST] Starting QEMU with external monitor on port {port}...")
    proc = subprocess.Popen(qemu_cmd)

    s = None
    try:
        # Wait for QEMU monitor socket to be ready
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
            # print(f"  -> QEMU: {cmd}")
            s.sendall((cmd + "\n").encode("utf-8"))
            time.sleep(0.18)
            try:
                s.recv(1024)
            except Exception:
                pass

        # Read banner
        time.sleep(1.0)
        try:
            s.recv(4096)
        except Exception:
            pass

        print("[TEST] Waiting for boot sequence and initial Ring 3 suite to settle...")
        settled = False
        for sec in range(35):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        content = f.read()
                        if "Encerrando processo com sys_exit(0)..." in content:
                            print(f"[TEST] System settled (task 2 exited) at {sec+1}s.")
                            settled = True
                            break
                except Exception:
                    pass

        if not settled:
            print("[WARN] System did not reach settlement marker; proceeding anyway...")

        # Wait 2 extra seconds for desktop composition to be completely idle
        time.sleep(2.0)

        # 1. Launch Browser from Terminal by typing 'browser' and pressing Return
        print("[TEST] Injecting keyboard input to spawn browser: 'browser' + Return")
        for ch in "browser":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")

        # Wait for browser process to spawn and create window
        print("[TEST] Waiting for browser window creation...")
        created = False
        for _ in range(12):
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

        # 2. Test Scrolling via hardware keys (Down, Up, PageDown, PageUp)
        print("[TEST] Testing Scroll Down (Down arrow)...")
        qemu_send("sendkey down")
        time.sleep(0.6)

        print("[TEST] Testing Scroll Up (Up arrow)...")
        qemu_send("sendkey up")
        time.sleep(0.6)

        print("[TEST] Testing Page Down...")
        qemu_send("sendkey pgdn")
        time.sleep(0.6)

        print("[TEST] Testing Page Up...")
        qemu_send("sendkey pgup")
        time.sleep(0.6)

        # 3. Test Address Bar Focus and Navigation via keyboard
        print("[TEST] Testing Address Bar Focus ('/')...")
        qemu_send("sendkey slash")
        time.sleep(0.6)

        print("[TEST] Typing URL 'about:version' into address bar and pressing Return...")
        url = "about:version"
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
        time.sleep(2.5)

        # 4. Test History Navigation: Back, Forward, Reload
        print("[TEST] Testing History: Back ('b')...")
        qemu_send("sendkey b")
        time.sleep(1.5)

        print("[TEST] Testing History: Forward ('f')...")
        qemu_send("sendkey f")
        time.sleep(1.5)

        print("[TEST] Testing History: Reload ('r')...")
        qemu_send("sendkey r")
        time.sleep(1.5)

        # 5. Test Window Close via Key ('q')
        print("[TEST] Testing Window Close ('q')...")
        qemu_send("sendkey q")
        time.sleep(1.5)

        # Done testing
        qemu_send("quit")
        time.sleep(1.0)

    finally:
        if s:
            s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    print("[TEST] QEMU terminated. Analyzing serial log...")
    if not os.path.exists(log_file):
        print(f"[FAIL] {log_file} does not exist!")
        sys.exit(1)

    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
        log_content = f.read()

    print(f"[TEST] Serial log length: {len(log_content)} chars")

    # Check for expected interaction markers
    required_markers = [
        "[BROWSER-INTERACTION] WINDOW_CREATED",
        "[BROWSER-INTERACTION] ACTION ScrollDown",
        "[BROWSER-INTERACTION] ACTION ScrollUp",
        "[BROWSER-INTERACTION] ACTION PageDown",
        "[BROWSER-INTERACTION] ACTION PageUp",
        "[BROWSER-INTERACTION] ACTION KeyFocusAddress",
        "[BROWSER-INTERACTION] ACTION AddressBar Navigate target=about:version",
        "[BROWSER-INTERACTION] PAGE_LOADED target=about:version",
        "[BROWSER-INTERACTION] ACTION KeyBack",
        "[BROWSER-INTERACTION] ACTION KeyForward",
        "[BROWSER-INTERACTION] ACTION KeyReload",
        "[BROWSER-INTERACTION] ACTION KeyClose",
    ]

    all_passed = True
    print("\n--- Telemetry Verification ---")
    for marker in required_markers:
        if marker in log_content:
            print(f"  [PASS] Found: {marker}")
        else:
            print(f"  [FAIL] Missing: {marker}")
            all_passed = False

    if all_passed:
        print("\n=== ALL BROWSER HARDWARE INTERACTION TESTS PASSED! ===")
        sys.exit(0)
    else:
        print("\n=== SOME INTERACTION TESTS FAILED ===")
        print("\n--- Excerpt of log containing [BROWSER ---")
        for line in log_content.splitlines():
            if "[BROWSER" in line or "[OK] Jinn Browser" in line or "spawn" in line:
                print("  " + line)
        sys.exit(1)

if __name__ == "__main__":
    main()
