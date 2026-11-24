#!/usr/bin/env python3
"""
scripts/make_pkg.py - Empacotador de Binários e Recursos no Formato .jpkg para o Jinn OS
Gera contêineres verificáveis com cabeçalho de 128 bytes e hash SHA-256.
"""

import hashlib
import os
import struct
import sys

JPKG_MAGIC = b"JPKG"
JPKG_FORMAT_VERSION = 1
HEADER_SIZE = 128

def create_jpkg(name: str, major: int, minor: int, patch: int, category: str, manifest: str, elf_path: str, out_path: str):
    if not os.path.exists(elf_path):
        raise FileNotFoundError(f"Arquivo ELF nao encontrado: {elf_path}")

    with open(elf_path, "rb") as f:
        elf_bytes = f.read()

    manifest_bytes = manifest.encode("utf-8")
    manifest_size = len(manifest_bytes)
    elf_size = len(elf_bytes)
    resources_size = 0
    elf_offset = HEADER_SIZE + manifest_size

    # Hash SHA-256 do payload (manifesto + ELF)
    payload_hasher = hashlib.sha256()
    payload_hasher.update(manifest_bytes)
    payload_hasher.update(elf_bytes)
    checksum = payload_hasher.digest()

    name_bytes = name.encode("utf-8")[:31].ljust(32, b"\x00")
    category_bytes = category.encode("utf-8")[:31].ljust(32, b"\x00")

    header = bytearray(HEADER_SIZE)
    header[0:4] = JPKG_MAGIC
    struct.pack_into("<H", header, 4, JPKG_FORMAT_VERSION)
    struct.pack_into("<H", header, 6, 0) # flags
    header[8:40] = name_bytes
    struct.pack_into("<H", header, 40, major)
    struct.pack_into("<H", header, 42, minor)
    struct.pack_into("<H", header, 44, patch)
    struct.pack_into("<H", header, 46, 0) # reserved
    header[48:80] = category_bytes
    struct.pack_into("<I", header, 80, manifest_size)
    struct.pack_into("<I", header, 84, elf_size)
    struct.pack_into("<I", header, 88, resources_size)
    struct.pack_into("<I", header, 92, elf_offset)
    header[96:128] = checksum

    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "wb") as f:
        f.write(header)
        f.write(manifest_bytes)
        f.write(elf_bytes)

    print(f"[OK] Pacote {name} v{major}.{minor}.{patch} gerado em {out_path} ({len(header) + len(manifest_bytes) + len(elf_bytes)} bytes, SHA-256: {checksum[:8].hex()}...)")

if __name__ == "__main__":
    if len(sys.argv) < 5:
        print("Uso: make_pkg.py <nome> <versao> <categoria> <elf_path> <out_path> [manifesto]")
        sys.exit(1)

    name = sys.argv[1]
    ver_parts = [int(p) for p in sys.argv[2].lstrip("v").split(".")]
    major = ver_parts[0] if len(ver_parts) > 0 else 1
    minor = ver_parts[1] if len(ver_parts) > 1 else 0
    patch = ver_parts[2] if len(ver_parts) > 2 else 0
    cat = sys.argv[3]
    elf = sys.argv[4]
    out = sys.argv[5]
    man = sys.argv[6] if len(sys.argv) > 6 else f"Pacote {name} oficial para Jinn OS"

    create_jpkg(name, major, minor, patch, cat, man, elf, out)
