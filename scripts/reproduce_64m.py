import subprocess
import time
import socket
import os
import sys

def run_test(use_disk=True, mem="64M", smp="4"):
    log_file = f"qemu_{mem}_{'disk' if use_disk else 'nodisk'}_{smp}.log"
    if os.path.exists(log_file):
        os.remove(log_file)

    qemu_bin = r"D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe"
    port = 5565

    qemu_cmd = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", "jinn.iso",
        "-m", mem,
        "-smp", smp,
        "-display", "none",
        "-serial", f"file:{log_file}",
        "-monitor", f"tcp:127.0.0.1:{port},server,nowait"
    ]
    if use_disk:
        qemu_cmd.extend(["-drive", "file=disk.img,format=raw,index=0,media=disk"])

    print(f"\n[TEST] Starting QEMU (mem={mem}, disk={use_disk}, smp={smp})...")
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
            time.sleep(0.15)
            try:
                s.recv(1024)
            except Exception:
                pass

        time.sleep(1.0)
        try:
            s.recv(4096)
        except Exception:
            pass

        print("[TEST] Waiting 20s for boot...")
        for sec in range(20):
            time.sleep(1.0)
            if (sec + 1) % 5 == 0:
                print(f"  ... waited {sec+1}s")

        print("[TEST] Sending 'browser' + Ret into terminal...")
        for ch in "browser":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")

        print("[TEST] Waiting 8s for execution...")
        time.sleep(8.0)

        qemu_send("quit")
        time.sleep(1.0)

    finally:
        if s:
            s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    print(f"[TEST] Reading {log_file}...")
    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            lines = f.readlines()
            print(f"Total log lines: {len(lines)}")
            for line in lines[-40:]:
                print("  >", line.strip())

if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "all"
    if mode in ("nodisk", "all"):
        run_test(use_disk=False, mem="64M", smp="4")
    if mode in ("disk", "all"):
        run_test(use_disk=True, mem="64M", smp="4")

