#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR Jinn-Commercial-1.0
"""
scripts/test_ciclo1_security_observability_e2e.py

Validacao E2E em QEMU (-device e1000 -smp 4) do Ciclo 1:
  1. Rejeicao X.509 SAN divergente (1A) e assinatura CertificateVerify corrompida (1B)
  2. Bloqueio de downgrade https:// -> http:// em redirect HTTP 302 (2A) e subrecurso <img> (2B)
  3. Fallback automatico https:// -> http:// apenas sem esquema digitado com badge HTTP (Sem TLS) (3)
  4. Medicao real em ciclos rdtsc e us das Syscalls 88 e 97 ([DIAG-BENCH]) + screendumps PNG em web/assets/
"""

import datetime
import hashlib
import hmac
import http.server
import ipaddress
import os
import re
import shutil
import socket
import socketserver
import ssl
import struct
import subprocess
import sys
import tempfile
import threading
import time
import zlib

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, x25519
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.x509.oid import NameOID

PROJECT_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
ISO_PATH = os.path.join(PROJECT_ROOT, "jinn.iso")
MASTER_DISK = os.path.join(PROJECT_ROOT, "disk.img")
ASSETS_DIR = os.path.join(PROJECT_ROOT, "web", "assets")


def find_qemu():
    for candidate in [
        os.path.join(PROJECT_ROOT, "tools", "qemu", "qemu-system-x86_64.exe"),
        r"C:\Program Files\qemu\qemu-system-x86_64.exe",
        r"C:\Program Files\QEMU\qemu-system-x86_64.exe",
        shutil.which("qemu-system-x86_64"),
    ]:
        if candidate and os.path.exists(candidate):
            return candidate
    raise FileNotFoundError("qemu-system-x86_64 nao encontrado")


def ppm_to_png(ppm_path, png_path):
    with open(ppm_path, "rb") as f:
        data = f.read()
    idx = 0
    tokens = []
    while len(tokens) < 4:
        while idx < len(data) and data[idx] in b" \t\r\n":
            idx += 1
        if idx < len(data) and data[idx] == ord("#"):
            while idx < len(data) and data[idx] != ord("\n"):
                idx += 1
            continue
        start = idx
        while idx < len(data) and data[idx] not in b" \t\r\n":
            idx += 1
        tokens.append(data[start:idx].decode("ascii"))
    width, height = int(tokens[1]), int(tokens[2])
    idx += 1
    raw_pixels = data[idx : idx + width * height * 3]
    raw_scanlines = bytearray()
    row_bytes = width * 3
    for y in range(height):
        raw_scanlines.append(0)
        raw_scanlines.extend(raw_pixels[y * row_bytes : (y + 1) * row_bytes])
    compressed = zlib.compress(bytes(raw_scanlines), 9)

    def make_chunk(chunk_type, chunk_data):
        return (
            struct.pack(">I", len(chunk_data))
            + chunk_type
            + chunk_data
            + struct.pack(">I", zlib.crc32(chunk_type + chunk_data) & 0xFFFFFFFF)
        )

    png_data = bytearray(b"\x89PNG\r\n\x1a\n")
    png_data.extend(make_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)))
    png_data.extend(make_chunk(b"IDAT", compressed))
    png_data.extend(make_chunk(b"IEND", b""))
    os.makedirs(os.path.dirname(png_path), exist_ok=True)
    with open(png_path, "wb") as f:
        f.write(png_data)


def qsend(sock, cmd, delay=0.14):
    sock.sendall((cmd + "\n").encode("utf-8"))
    time.sleep(delay)
    try:
        sock.recv(4096)
    except Exception:
        pass


def send_text(sock, text):
    for ch in text:
        if ch == ":":
            qsend(sock, "sendkey shift-semicolon", 0.12)
        elif ch == "/":
            qsend(sock, "sendkey slash", 0.08)
        elif ch == ".":
            qsend(sock, "sendkey dot", 0.07)
        elif ch == "-":
            qsend(sock, "sendkey minus", 0.07)
        elif ch == "_":
            qsend(sock, "sendkey shift-minus", 0.10)
        elif ch == " ":
            qsend(sock, "sendkey spc", 0.07)
        elif "A" <= ch <= "Z":
            qsend(sock, f"sendkey shift-{ch.lower()}", 0.10)
        else:
            qsend(sock, f"sendkey {ch}", 0.07)


def read_log(path):
    if not os.path.exists(path):
        return ""
    with open(path, "r", encoding="utf-8", errors="ignore") as f:
        return f.read()


def wait_for(path, needle, start=0, timeout=30.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        txt = read_log(path)
        idx = txt.find(needle, start)
        if idx != -1:
            return txt, idx
        time.sleep(0.25)
    return read_log(path), -1


def capture_screendump(sock, tmpdir, png_path, name):
    ppm = os.path.join(tmpdir, f"{name}.ppm")
    if os.path.exists(ppm):
        os.remove(ppm)
    qsend(sock, f"screendump {ppm}", 0.55)
    for _ in range(15):
        if os.path.exists(ppm) and os.path.getsize(ppm) > 10000:
            break
        time.sleep(0.2)
    ppm_to_png(ppm, png_path)


def generate_ecdsa_cert(tmpdir, name, san_entries, cn):
    key = ec.generate_private_key(ec.SECP256R1())
    now = datetime.datetime.now(datetime.timezone.utc)
    sub = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, cn)])
    cert = (
        x509.CertificateBuilder()
        .subject_name(sub)
        .issuer_name(sub)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - datetime.timedelta(days=1))
        .not_valid_after(now + datetime.timedelta(days=30))
        .add_extension(x509.SubjectAlternativeName(san_entries), critical=False)
        .sign(key, hashes.SHA256())
    )
    cert_pem = os.path.join(tmpdir, f"{name}.crt")
    key_pem = os.path.join(tmpdir, f"{name}.key")
    with open(cert_pem, "wb") as f:
        f.write(cert.public_bytes(serialization.Encoding.PEM))
    with open(key_pem, "wb") as f:
        f.write(
            key.private_bytes(
                serialization.Encoding.PEM,
                serialization.PrivateFormat.PKCS8,
                serialization.NoEncryption(),
            )
        )
    return cert_pem, key_pem, cert.public_bytes(serialization.Encoding.DER), key


# ── Helpers HKDF / TLS 1.3 para servidor de assinatura invalida (Cenario 1B) ────

def hkdf_extract(salt, ikm):
    return hmac.new(salt, ikm, hashlib.sha256).digest()


def hkdf_expand(prk, info, length):
    t = b""
    okm = b""
    c = 1
    while len(okm) < length:
        t = hmac.new(prk, t + info + bytes([c]), hashlib.sha256).digest()
        okm += t
        c += 1
    return okm[:length]


def hkdf_expand_label(secret, label, context, length):
    full_label = b"tls13 " + label
    hkdf_label = (
        struct.pack(">H", length)
        + bytes([len(full_label)])
        + full_label
        + bytes([len(context)])
        + context
    )
    return hkdf_expand(secret, hkdf_label, length)


def parse_client_hello_x25519(hs_msg):
    """Extrai legacy_session_id e chave publica X25519 (0x001D) do ClientHello."""
    # hs_msg: type(1)=1, len(3), ver(2), random(32)
    off = 4 + 2 + 32
    sid_len = hs_msg[off]
    off += 1
    sid = hs_msg[off : off + sid_len]
    off += sid_len
    cs_len = struct.unpack(">H", hs_msg[off : off + 2])[0]
    off += 2 + cs_len
    comp_len = hs_msg[off]
    off += 1 + comp_len
    ext_total = struct.unpack(">H", hs_msg[off : off + 2])[0]
    off += 2
    end = off + ext_total
    peer_x25519 = None
    while off + 4 <= end:
        etype, elen = struct.unpack(">HH", hs_msg[off : off + 4])
        off += 4
        edata = hs_msg[off : off + elen]
        off += elen
        if etype == 0x0033 and len(edata) >= 2:  # key_share
            ks_len = struct.unpack(">H", edata[0:2])[0]
            koff = 2
            while koff + 4 <= 2 + ks_len:
                grp, klen = struct.unpack(">HH", edata[koff : koff + 4])
                koff += 4
                if grp == 0x001D and klen == 32:
                    peer_x25519 = edata[koff : koff + 32]
                koff += klen
    return sid, peer_x25519


class BadCertVerifyTls13Server:
    """Servidor TLS 1.3 minimo com ChaCha20-Poly1305 que envia CertificateVerify adulterado."""

    def __init__(self, cert_der, priv_key):
        self.cert_der = cert_der
        self.priv_key = priv_key
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(4)
        self.port = self.sock.getsockname()[1]
        self.stop_ev = threading.Event()
        self.hits = 0
        self.thread = threading.Thread(target=self._serve, daemon=True)
        self.thread.start()

    def _serve(self):
        self.sock.settimeout(0.5)
        while not self.stop_ev.is_set():
            try:
                conn, _ = self.sock.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            self.hits += 1
            threading.Thread(target=self._handle, args=(conn,), daemon=True).start()

    def _handle(self, conn):
        try:
            conn.settimeout(3.0)
            buf = b""
            while len(buf) < 5:
                chunk = conn.recv(4096)
                if not chunk:
                    return
                buf += chunk
            rec_len = struct.unpack(">H", buf[3:5])[0]
            while len(buf) < 5 + rec_len:
                chunk = conn.recv(4096)
                if not chunk:
                    return
                buf += chunk
            ch_hs = buf[5 : 5 + rec_len]
            sid, peer_pub_bytes = parse_client_hello_x25519(ch_hs)
            if not peer_pub_bytes:
                return

            srv_priv = x25519.X25519PrivateKey.generate()
            srv_pub_bytes = srv_priv.public_key().public_bytes(
                serialization.Encoding.Raw, serialization.PublicFormat.Raw
            )
            shared = srv_priv.exchange(x25519.X25519PublicKey.from_public_bytes(peer_pub_bytes))

            ext_sv = struct.pack(">HHH", 0x002B, 2, 0x0304)
            ext_ks = struct.pack(">HHHH", 0x0033, 36, 0x001D, 32) + srv_pub_bytes
            exts = ext_sv + ext_ks
            sh_body = (
                b"\x03\x03"
                + os.urandom(32)
                + bytes([len(sid)])
                + sid
                + struct.pack(">H", 0x1303)  # TLS_CHACHA20_POLY1305_SHA256
                + b"\x00"
                + struct.pack(">H", len(exts))
                + exts
            )
            sh_hs = b"\x02" + len(sh_body).to_bytes(3, "big") + sh_body
            sh_rec = b"\x16\x03\x03" + struct.pack(">H", len(sh_hs)) + sh_hs
            ccs_rec = b"\x14\x03\x03\x00\x01\x01"
            conn.sendall(sh_rec + ccs_rec)

            # Key schedule RFC 8446
            zeros = b"\x00" * 32
            early = hkdf_extract(zeros, zeros)
            empty_h = hashlib.sha256(b"").digest()
            derived = hkdf_expand_label(early, b"derived", empty_h, 32)
            hs_secret = hkdf_extract(derived, shared)
            tr_sh = hashlib.sha256(ch_hs + sh_hs).digest()
            s_hs_traffic = hkdf_expand_label(hs_secret, b"s hs traffic", tr_sh, 32)
            key = hkdf_expand_label(s_hs_traffic, b"key", b"", 32)
            iv = hkdf_expand_label(s_hs_traffic, b"iv", b"", 12)
            aead = ChaCha20Poly1305(key)
            seq = [0]

            def enc_record(hs_msg):
                nonlocal seq
                n_bytes = seq[0].to_bytes(12, "big")
                nonce = bytes(a ^ b for a, b in zip(iv, n_bytes))
                seq[0] += 1
                plain = hs_msg + b"\x16"
                aad = b"\x17\x03\x03" + struct.pack(">H", len(plain) + 16)
                ct = aead.encrypt(nonce, plain, aad)
                return aad + ct

            ee_hs = b"\x08\x00\x00\x02\x00\x00"
            cert_entry = len(self.cert_der).to_bytes(3, "big") + self.cert_der + b"\x00\x00"
            cert_body = b"\x00" + len(cert_entry).to_bytes(3, "big") + cert_entry
            cert_hs = b"\x0b" + len(cert_body).to_bytes(3, "big") + cert_body

            tr_before_cv = hashlib.sha256(ch_hs + sh_hs + ee_hs + cert_hs).digest()
            signing_input = (
                (b"\x20" * 64)
                + b"TLS 1.3, server CertificateVerify"
                + b"\x00"
                + tr_before_cv
            )
            real_sig = bytearray(self.priv_key.sign(signing_input, ec.ECDSA(hashes.SHA256())))
            # Adultera 1 byte da assinatura real mantendo ASN.1 DER integro no envelope
            real_sig[-1] ^= 0x55
            cv_body = struct.pack(">HH", 0x0403, len(real_sig)) + bytes(real_sig)
            cv_hs = b"\x0f" + len(cv_body).to_bytes(3, "big") + cv_body

            conn.sendall(enc_record(ee_hs) + enc_record(cert_hs) + enc_record(cv_hs))
            time.sleep(0.3)
        except Exception:
            pass
        finally:
            try:
                conn.close()
            except Exception:
                pass

    def close(self):
        self.stop_ev.set()
        try:
            self.sock.close()
        except Exception:
            pass


class ThreadedHTTPServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True


def start_http_server(handler_cls, ssl_ctx=None):
    srv = ThreadedHTTPServer(("127.0.0.1", 0), handler_cls)
    if ssl_ctx is not None:
        srv.socket = ssl_ctx.wrap_socket(srv.socket, server_side=True)
    port = srv.server_address[1]
    th = threading.Thread(target=srv.serve_forever, daemon=True)
    th.start()
    return srv, port


def main():
    print("=" * 84)
    print("  JINN OS CICLO 1 — TESTE E2E: X.509/SAN, ANTI-DOWNGRADE HTTPS E SYSCALLS 88/97")
    print("=" * 84)

    qemu_bin = find_qemu()
    tmpdir = tempfile.mkdtemp(prefix="jinn_ciclo1_")
    servers = []
    bad_sig_srv = None
    proc = None
    sock = None

    trap_hits = [0]
    fallback_http_hits = [0]

    try:
        # Certificados X.509 P-256 reais
        ok_crt, ok_key, ok_der, ok_priv = generate_ecdsa_cert(
            tmpdir,
            "ok_san",
            [x509.IPAddress(ipaddress.IPv4Address("10.0.2.2")), x509.DNSName("jinn.local")],
            "10.0.2.2",
        )
        bad_crt, bad_key, _, _ = generate_ecdsa_cert(
            tmpdir,
            "bad_san",
            [x509.DNSName("divergent-host.evil.invalid")],
            "divergent-host.evil.invalid",
        )

        ctx_ok = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx_ok.minimum_version = ssl.TLSVersion.TLSv1_3
        ctx_ok.load_cert_chain(ok_crt, ok_key)

        ctx_bad_san = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx_bad_san.minimum_version = ssl.TLSVersion.TLSv1_3
        ctx_bad_san.load_cert_chain(bad_crt, bad_key)

        # Servidor Trap HTTP (NAO deve receber trafego dos bloqueios!)
        class TrapHandler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                trap_hits[0] += 1
                body = b"<html><body><h1>VAZAMENTO HTTP</h1></body></html>"
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        srv_trap, port_trap = start_http_server(TrapHandler)
        servers.append(srv_trap)

        # Cenario 2B: HTTPS valido com subrecurso <img src="http://...">
        class MixedImgHandler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                body = (
                    f"<html><head><title>Jinn TLS Mixed Block</title></head><body>"
                    f"<h1>HTTPS Verificado (X.509 SAN + CertificateVerify)</h1>"
                    f"<p>Subrecurso inseguro &lt;img src=http://10.0.2.2:{port_trap}/tracker.png&gt; bloqueado!</p>"
                    f"<img src=\"http://10.0.2.2:{port_trap}/tracker.png\" />"
                    f"</body></html>"
                ).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "text/html")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        srv_mixed, port_mixed = start_http_server(MixedImgHandler, ctx_ok)
        servers.append(srv_mixed)

        # Cenario 1A: TLS 1.3 com SAN divergente
        class BadSanHandler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                body = b"<html><body>OK</body></html>"
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        srv_bad_san, port_bad_san = start_http_server(BadSanHandler, ctx_bad_san)
        servers.append(srv_bad_san)

        # Cenario 1B: TLS 1.3 bruto com CertificateVerify corrompido
        bad_sig_srv = BadCertVerifyTls13Server(ok_der, ok_priv)
        port_bad_sig = bad_sig_srv.port

        # Cenario 2A: TLS 1.3 valido que redireciona 302 -> http://
        class RedirectDowngradeHandler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                self.send_response(302)
                self.send_header("Location", f"http://10.0.2.2:{port_trap}/downgraded.html")
                self.send_header("Content-Length", "0")
                self.end_headers()

        srv_redir, port_redir = start_http_server(RedirectDowngradeHandler, ctx_ok)
        servers.append(srv_redir)

        # Cenario 3: Servidor HTTP-only para testar fallback automatico sem esquema + badge ambar
        class PlainFallbackHandler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def handle_one_request(self):
                # Rejeita TLS ClientHello silenciosamente para acelerar fallback HTTP no navegador
                try:
                    peek = self.connection.recv(1, socket.MSG_PEEK)
                    if peek == b"\x16":
                        self.close_connection = True
                        return
                except Exception:
                    self.close_connection = True
                    return
                super().handle_one_request()

            def do_GET(self):
                fallback_http_hits[0] += 1
                body = (
                    b"<html><head><title>Portal Legado HTTP</title></head><body>"
                    b"<h1>Servidor Legado Sem TLS (Porta HTTP)</h1>"
                    b"<p>Acesso resolvido via fallback inicial sem esquema digitado.</p>"
                    b"<p>Confira o selo ambar HTTP (Sem TLS) na barra de endereco e status.</p>"
                    b"</body></html>"
                )
                self.send_response(200)
                self.send_header("Content-Type", "text/html")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        srv_plain, port_plain = start_http_server(PlainFallbackHandler)
        servers.append(srv_plain)

        print(
            f"[SERVERS] bad_san=:{port_bad_san}, bad_sig=:{port_bad_sig}, "
            f"redir=:{port_redir}, mixed=:{port_mixed}, trap=:{port_trap}, plain_fb=:{port_plain}"
        )

        # Disco temporario e logs QEMU
        work_disk = os.path.join(tmpdir, "ciclo1_disk.img")
        shutil.copyfile(MASTER_DISK, work_disk)
        log_file = os.path.join(tmpdir, "qemu_ciclo1.log")
        pcap_file = os.path.join(tmpdir, "qemu_ciclo1.pcap")
        mon_port = 5571

        qemu_cmd = [
            qemu_bin,
            "-boot", "order=d",
            "-cdrom", ISO_PATH,
            "-drive", f"file={work_disk},format=raw,index=0,media=disk",
            "-netdev", "user,id=n0",
            "-device", "e1000,netdev=n0",
            "-object", f"filter-dump,id=f0,netdev=n0,file={pcap_file}",
            "-m", "512M",
            "-smp", "4",
            "-display", "none",
            "-serial", f"file:{log_file}",
            "-monitor", f"tcp:127.0.0.1:{mon_port},server,nowait",
        ]
        print("[QEMU] Iniciando VM com -device e1000 -smp 4...")
        proc = subprocess.Popen(qemu_cmd)

        for _ in range(30):
            time.sleep(0.4)
            try:
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sock.settimeout(3.0)
                sock.connect(("127.0.0.1", mon_port))
                break
            except Exception:
                if sock:
                    sock.close()
                    sock = None
        if not sock:
            raise RuntimeError("Falha ao conectar ao monitor QEMU")
        time.sleep(0.5)
        try:
            sock.recv(4096)
        except Exception:
            pass

        txt, settled_idx = wait_for(log_file, "Servidor grafico em execucao", 0, timeout=45.0)
        if settled_idx == -1:
            print("[DEBUG BOOT LOG TAIL]\n" + txt[-2500:])
            raise RuntimeError("Aladdin/Jinn OS nao iniciou servidor grafico em 45s")

        # Aguarda rede/ARP de gateway 10.0.2.2 estabilizar antes de abrir browser
        txt_arp, arp_idx = wait_for(log_file, "[NET-ARP]", 0, timeout=12.0)
        cursor_boot = len(txt_arp) if arp_idx != -1 else len(txt)
        time.sleep(1.5)

        print("[QEMU] Abrindo Jinn Browser e ativando modo tela cheia F11...")
        send_text(sock, "browser")
        qsend(sock, "sendkey ret", 0.25)

        txt, wc_idx = wait_for(log_file, "[BROWSER-INTERACTION] WINDOW_CREATED", cursor_boot, timeout=20.0)
        if wc_idx == -1:
            print("[DEBUG WINDOW_CREATED TAIL]\n" + txt[-2500:])
            raise RuntimeError("Jinn Browser nao abriu janela")
        time.sleep(0.8)
        qsend(sock, "sendkey f11", 0.4)
        cursor = len(txt)

        def navigate_and_wait(url, marker, step_name):
            nonlocal cursor
            print(f"[SCENARIO] {step_name}: {url}")
            qsend(sock, "sendkey slash", 0.25)
            send_text(sock, url)
            qsend(sock, "sendkey ret", 0.2)
            full, idx = wait_for(log_file, marker, cursor, timeout=25.0)
            if idx == -1:
                print(f"  [DEBUG LOG TAIL]\n{full[cursor:]}")
                raise RuntimeError(f"Timeout aguardando '{marker}' em {step_name}")
            cursor = len(full)
            time.sleep(0.6)
            return full

        # 1A: SAN divergente
        navigate_and_wait(
            f"https://10.0.2.2:{port_bad_san}/",
            "[BROWSER-INTERACTION] PAGE_NET_ERROR",
            "1A - Bloqueio X.509 SAN Divergente",
        )
        capture_screendump(
            sock, tmpdir, os.path.join(ASSETS_DIR, "browser-ciclo1-x509-san-blocked.png"), "san_blocked"
        )

        # 1B: CertificateVerify corrompido
        navigate_and_wait(
            f"https://10.0.2.2:{port_bad_sig}/",
            "[BROWSER-INTERACTION] PAGE_NET_ERROR",
            "1B - Bloqueio CertificateVerify Adulterado",
        )
        capture_screendump(
            sock, tmpdir, os.path.join(ASSETS_DIR, "browser-ciclo1-certverify-blocked.png"), "cv_blocked"
        )

        # 2A: Redirect 302 HTTPS -> HTTP
        navigate_and_wait(
            f"https://10.0.2.2:{port_redir}/",
            "[BROWSER-INTERACTION] PAGE_NET_ERROR",
            "2A - Bloqueio Redirect 302 HTTPS->HTTP",
        )
        capture_screendump(
            sock, tmpdir, os.path.join(ASSETS_DIR, "browser-ciclo1-redirect-downgrade.png"), "redir_blocked"
        )

        # 2B: HTTPS valido com <img src="http://..."> bloqueada
        navigate_and_wait(
            f"https://10.0.2.2:{port_mixed}/",
            "[BROWSER-INTERACTION] PAGE_LOADED",
            "2B - Bloqueio Downgrade Subrecurso <img>",
        )
        capture_screendump(
            sock, tmpdir, os.path.join(ASSETS_DIR, "browser-ciclo1-downgrade-blocked.png"), "img_blocked"
        )

        # 3: Navegacao sem esquema -> Fallback HTTP com selo HTTP (Sem TLS)
        navigate_and_wait(
            f"10.0.2.2:{port_plain}/",
            "[BROWSER-INTERACTION] PAGE_LOADED",
            "3 - Fallback Sem Esquema + Badge HTTP (Sem TLS)",
        )
        capture_screendump(
            sock, tmpdir, os.path.join(ASSETS_DIR, "browser-ciclo1-http-fallback-badge.png"), "http_fallback"
        )

        qsend(sock, "sendkey q", 0.4)
        cursor_after_q = len(read_log(log_file))
        print("[QEMU] Validando Suite Ring 3 TEST 19 (Triade TLS 1.3 + 19.10 Sys88/97)...")
        send_text(sock, "spawn /disk/ola.elf")
        qsend(sock, "sendkey ret", 0.25)
        txt_t19, idx_t19 = wait_for(
            log_file,
            "Triade Criptografica e TLS 1.3 Homologados em Ring 3!",
            cursor_after_q,
            timeout=25.0,
        )
        if idx_t19 == -1:
            print("[DEBUG TEST 19 TAIL]\n" + txt_t19[cursor_after_q:])
            raise RuntimeError("Suite TEST 19 em /disk/ola.elf nao concluiu em 25s")

        qsend(sock, "quit", 0.5)
        time.sleep(0.5)

        final_log = read_log(log_file)
        checks = [
            ("TEST 19.10: Observabilidade Sys88/97 em Ring 3", "[PASS] 19.10: Observabilidade Ring3<->Ring0 (Sys88 & Sys97)"),
            ("TEST 19: Triade Criptografica & TLS 1.3", "[PASS] test_tls13_cryptography_and_syscalls: Triade Criptografica e TLS 1.3 Homologados em Ring 3!"),
            ("Cenario 1A: SAN Mismatch bloqueado", "[BROWSER-SEC] BLOQUEADO certificado TLS (SAN/CN mismatch)"),
            ("Cenario 1B: CertificateVerify bloqueado", "[BROWSER-SEC] BLOQUEADO certificado/assinatura TLS"),
            ("Cenario 2A: Redirect 302 downgrade bloqueado", "[BROWSER-SEC] BLOQUEADO downgrade HTTPS -> HTTP em redirecionamento"),
            ("Cenario 2B: <img> HTTP downgrade bloqueado", "[BROWSER-SEC] BLOQUEADO downgrade HTTPS -> HTTP em subrecurso <img>"),
            ("Cenario 3: Badge HTTP (Sem TLS) exibido", "[BROWSER-SEC] BADGE_HTTP_SEM_TLS ativo"),
            ("Telemetria [DIAG-BENCH]", "[DIAG-BENCH] tsc_khz="),
        ]

        print("\n=== VERIFICACAO DE ASSERCOES E2E ===")
        failed = False
        for label, marker in checks:
            ok = marker in final_log
            print(f"  [{'PASS' if ok else 'FAIL'}] {label}")
            if not ok:
                failed = True

        print(f"  [{'PASS' if trap_hits[0] == 0 else 'FAIL'}] Trap HTTP server hits == 0 (observed={trap_hits[0]})")
        if trap_hits[0] != 0:
            failed = True

        print(f"  [{'PASS' if fallback_http_hits[0] >= 1 else 'FAIL'}] Plain HTTP fallback server hits >= 1 (observed={fallback_http_hits[0]})")
        if fallback_http_hits[0] < 1:
            failed = True

        print("\n=== MEDICOES REAIS [DIAG-BENCH] (QEMU -device e1000 -smp 4) ===")
        for line in final_log.splitlines():
            if "[DIAG-BENCH]" in line or "[BROWSER-SEC]" in line or "19.10:" in line:
                print("  " + line)

        if failed:
            print("\n[FAIL] Um ou mais criterios do Ciclo 1 falharam!")
            sys.exit(1)

        print("\n=== TODOS OS CENARIOS E2E DO CICLO 1 PASSARAM COM EXITO! ===")
    finally:
        if sock:
            try:
                sock.close()
            except Exception:
                pass
        if proc and proc.poll() is None:
            proc.kill()
            proc.wait()
        for srv in servers:
            try:
                srv.shutdown()
                srv.server_close()
            except Exception:
                pass
        if bad_sig_srv:
            bad_sig_srv.close()
        shutil.rmtree(tmpdir, ignore_errors=True)


if __name__ == "__main__":
    main()
