#!/usr/bin/env python3
"""
scripts/publish_release.py

Utilitário de linha de comando para publicação e difusão de novos marcos (releases)
do Jinn OS.
Executa:
1. Reconstrução dos feeds RSS (web/build_feed.py);
2. Criação opcional de tag git (ex: v0.4.3);
3. Execução do pipeline de difusão multicanal (scripts/syndicate_broadcast.py);
4. Exibição dos rascunhos formatados prontos para publicação no Hacker News e Reddit.
"""

import sys
import os
import argparse
import subprocess
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parent.parent

def run_cmd(cmd, check=True):
    print(f"  -> Executando: {' '.join(cmd)}")
    res = subprocess.run(cmd, cwd=ROOT_DIR, text=True, capture_output=True)
    if check and res.returncode != 0:
        print(f"[ERRO] Comando falhou:\n{res.stderr}")
        sys.exit(res.returncode)
    return res.stdout.strip()

def main():
    parser = argparse.ArgumentParser(description="Publicador e Difusor de Releases do Jinn OS")
    parser.add_argument("--tag", type=str, help="Tag de versão (ex: v0.4.3)")
    parser.add_argument("--title", type=str, help="Título do marco técnico")
    parser.add_argument("--summary", type=str, help="Sumário técnico executivo")
    parser.add_argument("--dry-run", action="store_true", help="Apenas simula a difusão sem chamadas de rede externas")
    parser.add_argument("--git-push", action="store_true", help="Efetua push da tag / commits para o GitHub")

    args = parser.parse_args()

    title = args.title or "Native SIMD image decoding in Ring 3, SMP spawn race fix, and forensic VFS cold-reboot tests"
    summary = args.summary or (
        "Consolidamos o suporte a decodificação de imagens bare-metal no Jinn Browser com SIMD AVX2, "
        "resolução de concorrência SMP no boot e validação de persistência FAT32 com zero-idle."
    )

    print("================================================================================")
    print("  JINN OS — PUBLICADOR DE RELEASE & SINDICAÇÃO AUTOMÁTICA")
    print("================================================================================")
    print(f"Título  : {title}")
    if args.tag: print(f"Tag     : {args.tag}")
    print(f"Dry-Run : {args.dry_run}")
    print("================================================================================\n")

    # 1. Reconstrói os feeds RSS
    print("[1/3] Reconstruindo feeds RSS (web/feed.xml e web/rss.xml)...")
    run_cmd([sys.executable, "web/build_feed.py"])

    # 2. Executa a sindicação multicanal
    print("\n[2/3] Executando sindicação multicanal...")
    broadcast_cmd = [
        sys.executable, "scripts/syndicate_broadcast.py",
        "--title", title,
        "--summary", summary,
        "--update-feed"
    ]
    if args.tag:
        broadcast_cmd.extend(["--version", args.tag])
    if args.dry_run:
        broadcast_cmd.append("--dry-run")

    run_cmd(broadcast_cmd)

    # 3. Criação de tag e push opcional
    if args.tag:
        print(f"\n[3/3] Criando tag git {args.tag}...")
        run_cmd(["git", "tag", "-a", args.tag, "-m", f"release: {title}"], check=False)
        if args.git_push:
            print(f"  -> Enviando tag {args.tag} para o GitHub...")
            run_cmd(["git", "push", "origin", args.tag])

    print("\n================================================================================")
    print("  MARCO PUBLICADO COM SUCESSO! RASCUNHOS PRONTOS PARA COPIAR:")
    print("================================================================================")
    print("\n--- HACKER NEWS (Show HN) / r/osdev ---")
    print(f"Título: Jinn OS (Rust microkernel): {title}")
    print(f"""Texto:
Just pushed a major milestone for Jinn OS (https://jinn-os.dev.br), an experimental x86_64 microkernel written in Rust (`#![no_std]`) with capability-based security and user-space drivers.

Highlights:
* Ring 3 #![no_std] Image Decoding & SIMD: Added native PPM/BMP decoders and 16.16 fixed-point bilinear scaling offloaded to our SIMD engine (jhcs-rs).
* Network & DNS Hardening: Fixed IPv4/ARP SLIRP gateway routing with non-blocking fallback DNS cascade (1.1.1.1 -> 8.8.8.8 -> SLIRP).
* SMP Concurrency & Zero-Idle: Eliminated dual-spawn race condition during boot while maintaining 0.00% idle CPU across 4 vCPUs.

Source: https://github.com/GeovaneEstanislau/Jinn_Project
Live portal & RSS: https://jinn-os.dev.br/feed.xml
""")
    print("--------------------------------------------------------------------------------\n")

if __name__ == "__main__":
    main()
