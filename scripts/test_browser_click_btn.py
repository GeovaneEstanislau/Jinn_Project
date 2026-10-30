import subprocess
import time
import socket
import os
import sys

def main():
    log_file = "qemu_btn_test.log"
    if os.path.exists(log_file):
        os.remove(log_file)

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    port = 5560

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

        print("[TEST] Waiting 36s for boot sequence...")
        for sec in range(36):
            time.sleep(1.0)
            if (sec + 1) % 10 == 0:
                print(f"  ... waited {sec+1}s")

        time.sleep(2.0)

        # Move mouse to button (x=550, y=298)
        # Note: in QEMU monitor, mouse_move uses dx, dy, or mouse_set
        print("[TEST] Clicking button '+ Executar Jinn Browser' via mouse_set or key...")
        # First reset mouse or move
        # Let's see: QEMU monitor supports 'mouse_move <dx> <dy> [dz]'
        # But we also have start menu / dock!
        # In start menu: item 3 is "Navegador Web"
        # In dock: slot 3 is Browser icon
        # In terminal: we can type 'browser' or click
        # Let's send 'browser' + Return into terminal:
        for ch in "browser":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")

        time.sleep(5.0)

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
            content = f.read()
            if "[BROWSER-INTERACTION] WINDOW_CREATED" in content:
                print(">>> SUCCESS: Jinn Browser spawned and created window! <<<")
            else:
                print(">>> FAILED: Window not created <<<")
                for line in content.splitlines()[-30:]:
                    print("  LOG:", line)

if __name__ == "__main__":
    main()

