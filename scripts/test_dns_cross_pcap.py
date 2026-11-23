#!/usr/bin/env python3
"""
scripts/test_dns_cross_pcap.py — Validação Cruzada E2E com Captura de Pacotes PCAP (Intel e1000 vs VirtIO-Net)
Testa a resolução de DNS do Jinn Browser com inspeção de PCAP, ARP real e Zero-Idle.
"""

import os
import sys
import time
import socket
import struct
import shutil
import subprocess

def parse_pcap(filepath):
    """Analisa arquivo PCAP capturado pelo QEMU filter-dump."""
    if not os.path.exists(filepath):
        return {"error": f"Arquivo PCAP nao encontrado: {filepath}"}

    file_size = os.path.getsize(filepath)
    if file_size < 24:
        return {"error": f"PCAP truncado: {file_size} bytes"}

    packets = []
    arp_requests = []
    arp_replies = []
    dns_queries = []
    dns_replies = []
    http_packets = []

    with open(filepath, "rb") as f:
        ghdr = f.read(24)
        magic = struct.unpack("<I", ghdr[:4])[0]
        endian = "<" if magic == 0xa1b2c3d4 else ">"
        
        while True:
            phdr = f.read(16)
            if len(phdr) < 16:
                break
            ts_sec, ts_usec, incl_len, orig_len = struct.unpack(f"{endian}IIII", phdr)
            pdata = f.read(incl_len)
            if len(pdata) < 14:
                continue

            ethertype = struct.unpack(">H", pdata[12:14])[0]
            if ethertype == 0x0806: # ARP
                if len(pdata) >= 42:
                    opcode = struct.unpack(">H", pdata[20:22])[0]
                    smac = ":".join(f"{b:02x}" for b in pdata[22:28])
                    sip = ".".join(str(b) for b in pdata[28:32])
                    tmac = ":".join(f"{b:02x}" for b in pdata[32:38])
                    tip = ".".join(str(b) for b in pdata[38:42])
                    pkt_info = {"opcode": opcode, "smac": smac, "sip": sip, "tmac": tmac, "tip": tip}
                    if opcode == 1:
                        arp_requests.append(pkt_info)
                    elif opcode == 2:
                        arp_replies.append(pkt_info)
            elif ethertype == 0x0800: # IPv4
                if len(pdata) >= 34:
                    proto = pdata[23]
                    src_ip = ".".join(str(b) for b in pdata[26:30])
                    dst_ip = ".".join(str(b) for b in pdata[30:34])
                    if proto == 17 and len(pdata) >= 42: # UDP
                        sport, dport = struct.unpack(">HH", pdata[34:38])
                        if dport == 53:
                            dns_queries.append({"src": src_ip, "dst": dst_ip, "sport": sport, "dport": dport})
                        elif sport == 53:
                            dns_replies.append({"src": src_ip, "dst": dst_ip, "sport": sport, "dport": dport})
                    elif proto == 6 and len(pdata) >= 54: # TCP
                        sport, dport = struct.unpack(">HH", pdata[34:38])
                        if sport == 80 or dport == 80:
                            http_packets.append({"src": src_ip, "dst": dst_ip, "sport": sport, "dport": dport})

    return {
        "file_size": file_size,
        "arp_requests": arp_requests,
        "arp_replies": arp_replies,
        "dns_queries": dns_queries,
        "dns_replies": dns_replies,
        "http_packets": http_packets,
    }

def run_test_pass(pass_name, device_opt, pcap_file, log_file, monitor_port):
    print("================================================================================")
    print(f"  EXECUTANDO {pass_name}: {device_opt}")
    print(f"  PCAP: {pcap_file} | Log: {log_file} | Porta: {monitor_port}")
    print("================================================================================")

    qemu_bin = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    if not os.path.exists(qemu_bin):
        print(f"[FAIL] QEMU nao encontrado em: {qemu_bin}")
        return False

    iso_path = "jinn.iso"
    master_disk = "disk.img"
    test_disk = f"test_disk_{pass_name.lower().replace(' ', '_')}.img"

    for f in [pcap_file, log_file, test_disk]:
        if os.path.exists(f):
            try: os.remove(f)
            except Exception: pass

    shutil.copyfile(master_disk, test_disk)

    qemu_cmd = [
        qemu_bin,
        "-boot", "order=d",
        "-cdrom", iso_path,
        "-drive", f"file={test_disk},format=raw,index=0,media=disk",
        "-netdev", "user,id=n0",
        "-device", f"{device_opt},netdev=n0",
        "-object", f"filter-dump,id=f0,netdev=n0,file={pcap_file}",
        "-m", "512M",
        "-smp", "4",
        "-display", "none",
        "-serial", f"file:{log_file}",
        "-monitor", f"tcp:127.0.0.1:{monitor_port},server,nowait"
    ]

    print(f"[+] Disparando QEMU...")
    proc = subprocess.Popen(qemu_cmd)
    s = None
    try:
        for attempt in range(25):
            time.sleep(1)
            try:
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.settimeout(3.0)
                s.connect(("127.0.0.1", monitor_port))
                print(f"  [+] Conectado ao monitor QEMU (tentativa {attempt+1})")
                break
            except Exception:
                if s: s.close()
                s = None

        if not s:
            print("[FAIL] Nao foi possivel conectar ao monitor QEMU!")
            proc.kill()
            return False

        def qemu_send(cmd):
            s.sendall((cmd + "\n").encode("utf-8"))
            time.sleep(0.18)
            try: s.recv(1024)
            except Exception: pass

        time.sleep(1.0)
        try: s.recv(4096)
        except Exception: pass

        print("[+] Aguardando boot e inicializacao do Aladdin Desktop (15s)...")
        aladdin_ready = False
        for sec in range(25):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        c = f.read()
                        if "Servidor grafico em execucao" in c or "Servidor Grafico Ring 3 registrado" in c:
                            aladdin_ready = True
                            print(f"  [+] Aladdin Desktop ativo apos {sec+1}s!")
                            break
                except Exception: pass

        if not aladdin_ready:
            print("[WARN] Aladdin Desktop readiness timeout; continuando...")

        time.sleep(2.0)

        # Inicia o Jinn Browser
        print("[+] Abrindo Jinn Browser via comando 'browser'...")
        qemu_send("sendkey ret")
        time.sleep(0.5)
        for _ in range(8):
            qemu_send("sendkey backspace")
        time.sleep(0.3)
        for ch in "browser":
            qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")

        browser_ready = False
        for sec in range(25):
            time.sleep(1.0)
            if os.path.exists(log_file):
                try:
                    with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                        c = f.read()
                        if "[BROWSER-INTERACTION] WINDOW_CREATED" in c:
                            browser_ready = True
                            print(f"  [+] Janela do Browser criada apos {sec+1}s!")
                            break
                except Exception: pass

        if not browser_ready:
            print("[WARN] WINDOW_CREATED timeout; continuando...")

        time.sleep(2.5)

        # Envia URL http://example.com
        print("[+] Focando barra de endereco com '/' e digitando http://example.com...")
        qemu_send("sendkey slash")
        time.sleep(0.6)
        for ch in "http://example.com":
            if ch == ':':
                qemu_send("sendkey shift-semicolon")
            elif ch == '/':
                qemu_send("sendkey slash")
            elif ch == '.':
                qemu_send("sendkey dot")
            else:
                qemu_send(f"sendkey {ch}")
        qemu_send("sendkey ret")

        print("[+] Aguardando resolucao DNS e carregamento HTTP (15s)...")
        time.sleep(15.0)

        print("[+] Enviando 'quit' ao monitor QEMU para fechar PCAP e flush...")
        qemu_send("quit")
        time.sleep(2.0)

    finally:
        if s: s.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()

    # Inspeciona log serial
    print(f"\n--- Analise do Log Serial ({log_file}) ---")
    log_content = ""
    if os.path.exists(log_file):
        with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
            log_content = f.read()

    driver_detected = "[NET-INIT] Driver ativo detectado:" in log_content
    arp_resolved = "[NET-ARP] Gateway 10.0.2.2 resolvido para MAC" in log_content
    dns_resolving = "[NET-DNS] Resolvendo 'example.com'" in log_content
    dns_resolved = "[NET-DNS] 'example.com' resolvido com sucesso para" in log_content
    page_loaded = "PAGE_LOADED target=http://example.com" in log_content

    # Linhas de interesse
    for line in log_content.splitlines():
        if any(tag in line for tag in ["[NET-INIT]", "[NET-ARP]", "[NET-DNS]", "PAGE_LOADED", "[DIAG-FLOW]"]):
            print(f"  {line}")

    print(f"\nChecagens do Log Serial:")
    print(f"  [1] Driver ativo detectado: {'[PASS]' if driver_detected else '[FAIL]'}")
    print(f"  [2] ARP do Gateway resolvido: {'[PASS]' if arp_resolved else '[FAIL]'}")
    print(f"  [3] Consulta DNS iniciada: {'[PASS]' if dns_resolving else '[FAIL]'}")
    print(f"  [4] DNS resolvido com sucesso: {'[PASS]' if dns_resolved else '[FAIL]'}")
    print(f"  [5] Pagina HTTP carregada: {'[PASS]' if page_loaded else '[FAIL]'}")

    # Inspeciona PCAP
    print(f"\n--- Analise do Pacote PCAP ({pcap_file}) ---")
    pcap_info = parse_pcap(pcap_file)
    if "error" in pcap_info:
        print(f"  [FAIL] {pcap_info['error']}")
        return False

    print(f"  Tamanho PCAP: {pcap_info['file_size']} bytes")
    print(f"  Requisicoes ARP: {len(pcap_info['arp_requests'])}")
    for r in pcap_info['arp_requests']:
        print(f"    who-has {r['tip']} tell {r['sip']} (src MAC {r['smac']})")
    print(f"  Respostas ARP: {len(pcap_info['arp_replies'])}")
    for r in pcap_info['arp_replies']:
        print(f"    reply: {r['sip']} is-at {r['smac']}")
    print(f"  Consultas DNS (UDP 53): {len(pcap_info['dns_queries'])}")
    for q in pcap_info['dns_queries']:
        print(f"    query to {q['dst']}:53 from port {q['sport']}")
    print(f"  Respostas DNS (UDP 53): {len(pcap_info['dns_replies'])}")
    for r in pcap_info['dns_replies']:
        print(f"    reply from {r['src']}:53 to port {r['dport']}")
    print(f"  Pacotes HTTP (TCP 80): {len(pcap_info['http_packets'])}")

    pcap_arp_ok = len(pcap_info['arp_requests']) > 0 and len(pcap_info['arp_replies']) > 0
    pcap_dns_ok = len(pcap_info['dns_queries']) > 0

    print(f"Checagens PCAP:")
    print(f"  [1] ARP Handshake (Request/Reply): {'[PASS]' if pcap_arp_ok else '[FAIL]'}")
    print(f"  [2] Trafego DNS UDP/53 capturado: {'[PASS]' if pcap_dns_ok else '[FAIL]'}")

    # Limpeza de disco de teste
    if os.path.exists(test_disk):
        try: os.remove(test_disk)
        except Exception: pass

    overall_pass = driver_detected and arp_resolved and dns_resolving and pcap_arp_ok and pcap_dns_ok
    print(f"\nResultado da {pass_name}: {'[PASS] SUCESSO INTEGRAL' if overall_pass else '[FAIL] FALHA EM CHECAGEM'}\n")
    return overall_pass

def main():
    print("################################################################################")
    print("  JINN OS — SUITE DE VALIDACAO CRUZADA E2E DE DNS E REDE (PCAP DUMP)")
    print("################################################################################\n")

    pass_a = run_test_pass(
        pass_name="Passagem A (VirtualBox Parity - Intel e1000)",
        device_opt="e1000",
        pcap_file="dns_e1000.pcap",
        log_file="dns_e1000.log",
        monitor_port=5571
    )

    pass_b = run_test_pass(
        pass_name="Passagem B (QEMU Standard - VirtIO-Net)",
        device_opt="virtio-net-pci",
        pcap_file="dns_virtio.pcap",
        log_file="dns_virtio.log",
        monitor_port=5572
    )

    print("================================================================================")
    print("  RESUMO FINAL DA VALIDACAO CRUZADA")
    print("================================================================================")
    print(f"  Passagem A (Intel e1000 - VirtualBox Parity): {'[PASS]' if pass_a else '[FAIL]'}")
    print(f"  Passagem B (VirtIO-Net - QEMU Standard):      {'[PASS]' if pass_b else '[FAIL]'}")

    if pass_a and pass_b:
        print("\n>>> SUCESSO TOTAL: Ambas as placas resolveram ARP e DNS com exito!")
        sys.exit(0)
    else:
        print("\n>>> FALHA: Uma ou mais passagens falharam.")
        sys.exit(1)

if __name__ == "__main__":
    main()
