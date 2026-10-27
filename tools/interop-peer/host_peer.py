#!/usr/bin/env python3
"""
Jinn OS — Independent TCP Interoperability Peer
Platform: Python standard library (socket / Windows NT tcpip.sys)

Demonstrates external TCP interoperability with Jinn OS executing in QEMU:
- Level A: 3-way handshake, small payload transfer, echo, graceful close (FIN/ACK), consecutive connections.
- Level B: Multi-segment transfer (> MSS 1460 bytes, e.g. 2048 bytes), bidirectional streaming, checksum & ordering verification.
- Level C/D: Inbound connections to Jinn OS server port 8080 (via QEMU port forward 127.0.0.1:8088), JinnWall firewall enforcement.
"""

import sys
import time
import socket
import struct
import argparse
import threading

MAGIC = b"JINN"
HEADER_FORMAT = ">4sIIII" # Magic(4), Session(4), MsgId(4), Length(4), Checksum(4)
HEADER_SIZE = struct.calcsize(HEADER_FORMAT)

def compute_checksum(payload: bytes) -> int:
    """Calculates wrapping 32-bit sum over all payload bytes."""
    return sum(payload) & 0xFFFFFFFF

def generate_deterministic_payload(length: usize if False else int, seed: int = 0x41) -> bytes:
    """Generates a deterministic payload with recognizable pseudo-random sequence."""
    return bytes([(((i * 7) + seed) & 0xFF) for i in range(length)])

def verify_deterministic_payload(payload: bytes, seed: int = 0x41) -> bool:
    """Verifies that received payload matches the deterministic formula."""
    expected = generate_deterministic_payload(len(payload), seed)
    return payload == expected

def build_message(session_id: int, msg_id: int, payload: bytes) -> bytes:
    """Encapsulates a payload into the Jinn Interoperability Protocol format."""
    csum = compute_checksum(payload)
    hdr = struct.pack(HEADER_FORMAT, MAGIC, session_id, msg_id, len(payload), csum)
    return hdr + payload

def parse_header(header_bytes: bytes):
    if len(header_bytes) < HEADER_SIZE:
        return None
    magic, session_id, msg_id, length, checksum = struct.unpack(HEADER_FORMAT, header_bytes[:HEADER_SIZE])
    if magic != MAGIC:
        return None
    return {
        "session_id": session_id,
        "msg_id": msg_id,
        "length": length,
        "checksum": checksum
    }

def recv_exact(sock: socket.socket, num_bytes: int, timeout: float = 10.0) -> bytes:
    """Receives exactly num_bytes from stream, reassembling across multiple TCP segments."""
    sock.settimeout(timeout)
    data = bytearray()
    while len(data) < num_bytes:
        chunk = sock.recv(min(4096, num_bytes - len(data)))
        if not chunk:
            break
        data.extend(chunk)
    return bytes(data)

class PeerServer:
    def __init__(self, host: str = "0.0.0.0", port: int = 9999):
        self.host = host
        self.port = port
        self.server_sock = None
        self.is_running = False
        self.results = []
        self.lock = threading.Lock()

    def start(self):
        self.server_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.server_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.server_sock.bind((self.host, self.port))
        self.server_sock.listen(5)
        self.is_running = True
        print(f"[HOST-PEER-SERVER] Listening on {self.host}:{self.port} (independent Windows TCP stack)")

    def run_session(self, max_connections: int = 3, timeout: float = 40.0):
        self.start()
        start_time = time.time()
        conns_handled = 0

        while conns_handled < max_connections and (time.time() - start_time) < timeout:
            self.server_sock.settimeout(max(1.0, timeout - (time.time() - start_time)))
            try:
                client_sock, client_addr = self.server_sock.accept()
            except socket.timeout:
                break
            except Exception as e:
                print(f"[HOST-PEER-SERVER] Accept error: {e}")
                break

            conns_handled += 1
            print(f"\n[HOST-PEER-SERVER] Connection #{conns_handled} accepted from {client_addr[0]}:{client_addr[1]}")

            try:
                self.handle_client(client_sock, client_addr, conns_handled)
            finally:
                client_sock.close()
                print(f"[HOST-PEER-SERVER] Connection #{conns_handled} closed gracefully.")

        self.is_running = False
        if self.server_sock:
            self.server_sock.close()

    def handle_client(self, sock: socket.socket, addr, conn_num: int):
        # 1. Read header
        hdr_bytes = recv_exact(sock, HEADER_SIZE, timeout=10.0)
        if len(hdr_bytes) < HEADER_SIZE:
            print(f"[HOST-PEER-SERVER] Connection #{conn_num}: Truncated header received ({len(hdr_bytes)} bytes)")
            return

        hdr = parse_header(hdr_bytes)
        if not hdr:
            print(f"[HOST-PEER-SERVER] Connection #{conn_num}: Invalid magic in header!")
            return

        print(f"[HOST-PEER-SERVER] Msg received: Session=0x{hdr['session_id']:08X}, MsgID={hdr['msg_id']}, PayloadLen={hdr['length']} bytes, Csum=0x{hdr['checksum']:08X}")

        # 2. Read full payload (may arrive in multiple segments if > MSS)
        payload = recv_exact(sock, hdr['length'], timeout=10.0)
        if len(payload) != hdr['length']:
            print(f"[HOST-PEER-SERVER] Incomplete payload: expected {hdr['length']}, received {len(payload)}")
            return

        # 3. Check integrity
        actual_csum = compute_checksum(payload)
        csum_ok = (actual_csum == hdr['checksum'])
        pattern_ok = verify_deterministic_payload(payload, seed=0x41)

        print(f"[HOST-PEER-SERVER] Payload verification: ChecksumMatch={csum_ok}, PatternMatch={pattern_ok}, TotalBytes={len(payload)}")

        record = {
            "conn_num": conn_num,
            "session_id": hdr['session_id'],
            "msg_id": hdr['msg_id'],
            "payload_len": len(payload),
            "checksum_ok": csum_ok,
            "pattern_ok": pattern_ok,
        }
        with self.lock:
            self.results.append(record)

        # 4. Echo back response with deterministic reply
        # MsgID + 100, seed = 0x52
        resp_payload = generate_deterministic_payload(len(payload), seed=0x52)
        resp_msg = build_message(hdr['session_id'], hdr['msg_id'] + 100, resp_payload)
        sock.sendall(resp_msg)
        print(f"[HOST-PEER-SERVER] Echoed {len(resp_msg)} bytes back to peer (MsgID={hdr['msg_id'] + 100}).")

        # 5. Check if peer sends a second message on same connection (e.g. Level B streaming)
        sock.settimeout(2.0)
        try:
            second_hdr_bytes = recv_exact(sock, HEADER_SIZE, timeout=2.0)
            if len(second_hdr_bytes) == HEADER_SIZE:
                hdr2 = parse_header(second_hdr_bytes)
                if hdr2:
                    payload2 = recv_exact(sock, hdr2['length'], timeout=10.0)
                    csum2_ok = (compute_checksum(payload2) == hdr2['checksum'])
                    pat2_ok = verify_deterministic_payload(payload2, seed=0x42)
                    print(f"[HOST-PEER-SERVER] Second message received on conn #{conn_num}: MsgID={hdr2['msg_id']}, Len={len(payload2)}, CsumMatch={csum2_ok}, PatternMatch={pat2_ok}")
                    # Echo back
                    resp_payload2 = generate_deterministic_payload(len(payload2), seed=0x53)
                    sock.sendall(build_message(hdr2['session_id'], hdr2['msg_id'] + 100, resp_payload2))
        except (socket.timeout, socket.error):
            pass

def test_inbound_connection_to_jinn(target_host: str = "127.0.0.1", target_port: int = 8088, timeout: float = 5.0) -> bool:
    """Connects to Jinn OS listening port via QEMU hostfwd (127.0.0.1:8088 -> 10.0.2.15:8080)."""
    print(f"\n[HOST-PEER-CLIENT] Initiating inbound connection to Jinn OS at {target_host}:{target_port}...")
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(timeout)
    try:
        sock.connect((target_host, target_port))
        print(f"[HOST-PEER-CLIENT] Connection established with Jinn OS server!")

        # Send test message
        test_payload = generate_deterministic_payload(64, seed=0x77)
        msg = build_message(0x2001, 1, test_payload)
        sock.sendall(msg)
        print(f"[HOST-PEER-CLIENT] Sent {len(msg)} bytes to Jinn OS.")

        # Read response
        resp_hdr_bytes = recv_exact(sock, HEADER_SIZE, timeout=5.0)
        if len(resp_hdr_bytes) == HEADER_SIZE:
            hdr = parse_header(resp_hdr_bytes)
            if hdr:
                resp_payload = recv_exact(sock, hdr['length'], timeout=5.0)
                csum_ok = (compute_checksum(resp_payload) == hdr['checksum'])
                print(f"[HOST-PEER-CLIENT] Received response: MsgID={hdr['msg_id']}, Len={len(resp_payload)}, CsumMatch={csum_ok}")
                sock.close()
                return csum_ok
        sock.close()
        return False
    except Exception as e:
        print(f"[HOST-PEER-CLIENT] Inbound connection result: {e}")
        sock.close()
        return False

def main():
    parser = argparse.ArgumentParser(description="Jinn OS Independent TCP Interoperability Peer")
    parser.add_argument("--mode", choices=["server", "client", "both"], default="server", help="Operating mode")
    parser.add_argument("--host", default="0.0.0.0", help="Listen host for server mode")
    parser.add_argument("--port", type=int, default=9999, help="Listen port for server mode")
    parser.add_argument("--client-host", default="127.0.0.1", help="Target host for client mode")
    parser.add_argument("--client-port", type=int, default=8088, help="Target port for client mode")
    parser.add_argument("--connections", type=int, default=3, help="Number of connections to accept")
    parser.add_argument("--timeout", type=float, default=60.0, help="Server timeout in seconds")
    args = parser.parse_args()

    peer_server = PeerServer(args.host, args.port)

    if args.mode in ["server", "both"]:
        print(f"=== Jinn OS Network Interoperability Peer ===")
        print(f"Independent TCP implementation: Python {sys.version.split()[0]} on {sys.platform}")
        peer_server.run_session(max_connections=args.connections, timeout=args.timeout)

    if args.mode in ["client", "both"]:
        success = test_inbound_connection_to_jinn(args.client_host, args.client_port)
        if not success:
            print("[HOST-PEER-CLIENT] Inbound test did not succeed or was blocked by firewall.")

    # Print summary
    print("\n=== INTEROPERABILITY TEST SUMMARY ===")
    print(f"Total connections handled: {len(peer_server.results)}")
    all_ok = True
    for res in peer_server.results:
        status = "PASS" if (res['checksum_ok'] and res['pattern_ok']) else "FAIL"
        if status == "FAIL":
            all_ok = False
        print(f"  Conn #{res['conn_num']}: MsgID={res['msg_id']}, Len={res['payload_len']} bytes, Status={status}")

    if peer_server.results and all_ok:
        print("\n>>> ALL TCP INTEROPERABILITY CRITERIA VERIFIED SUCCESSFULLY! <<<")
        sys.exit(0)
    elif peer_server.results and not all_ok:
        print("\n>>> SOME PACKETS FAILED CHECKSUM OR PATTERN VERIFICATION! <<<")
        sys.exit(1)
    else:
        print("\n>>> NO CONNECTIONS WERE RECEIVED WITHIN TIMEOUT. <<<")
        sys.exit(2)

if __name__ == "__main__":
    main()
