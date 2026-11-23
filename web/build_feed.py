#!/usr/bin/env python3
"""
web/build_feed.py

Gerador estático do Feed RSS 2.0 / Atom para o portal Jinn OS (jinn-os.dev.br).
Lê os commits técnicos marcados com [announce], feat, perf ou fix, e gera:
  - web/feed.xml
  - web/rss.xml
"""

import os
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

SITE_URL = "https://jinn-os.dev.br"
REPO_URL = "https://github.com/GeovaneEstanislau/Jinn_Project"
WEB_DIR = Path(__file__).resolve().parent
FEED_PATH = WEB_DIR / "feed.xml"
RSS_PATH = WEB_DIR / "rss.xml"

FEATURED_POSTS = [
    {
        "guid": "jinn-os-post-mortem-pic-eoi-zero-gpu",
        "date": "Sun, 22 Nov 2026 21:00:00 -0300",
        "title": "Post-Mortem: Como o EOI Incondicional no PIC 8259A e o Motor Zero-GPU Viabilizaram 0.00% de CPU em Repouso",
        "url": f"{SITE_URL}/post-mortem-pic-eoi-zero-gpu.html",
        "body": (
            "Auditoria aprofundada sobre a eliminação de congelamentos de interrupção (IRQs 1 e 12) via EOI "
            "incondicional no PIC mestre e escravo, combinada com a arquitetura Zero-GPU do JHCS (AVX2/FMA sem "
            "alocações no laço quente), atingindo o Rank 1 de eficiência energética com 0.00% de CPU em repouso absoluto."
        )
    }
]

def get_announced_commits(limit=15):
    """Extrai commits relevantes com tags semânticas ou flag [announce]."""
    cmd = [
        "git", "log", f"-n{limit}",
        "--grep=\\[announce\\]\\|^feat\\|^perf\\|^fix\\|^release",
        "--pretty=format:%H|%ad|%s|%b---END---",
        "--date=rfc2822"
    ]
    try:
        raw = subprocess.check_output(cmd, text=True, encoding="utf-8")
    except Exception as e:
        print(f"[WARN] Falha ao executar git log: {e}")
        return []

    entries = []
    for block in raw.split("---END---\n"):
        if not block.strip():
            continue
        parts = block.strip().split("|", 3)
        if len(parts) >= 3:
            h = parts[0]
            d = parts[1]
            s = parts[2]
            b = parts[3].strip() if len(parts) > 3 and parts[3].strip() else s
            entries.append({
                "hash": h,
                "date": d,
                "title": s,
                "body": b
            })
    return entries

def build_rss():
    print(f"[+] Construindo RSS Feed em {FEED_PATH} e {RSS_PATH}...")
    ET.register_namespace('atom', 'http://www.w3.org/2005/Atom')
    ET.register_namespace('media', 'http://search.yahoo.com/mrss/')

    rss = ET.Element("rss", version="2.0")
    channel = ET.SubElement(rss, "channel")
    ET.SubElement(channel, "title").text = "Jinn OS — Devlog & Microkernel Releases"
    ET.SubElement(channel, "link").text = SITE_URL
    ET.SubElement(channel, "description").text = (
        "Atualizações técnicas, benchmarks de latência SIMD/AVX2, post-mortems e releases "
        "do microkernel bare-metal x86_64 em Rust (jinn-os.dev.br)."
    )
    ET.SubElement(channel, "language").text = "pt-br"

    # Suporte WebSub (antigo PubSubHubbub) para indexação em tempo real
    hub_link = ET.SubElement(channel, "{http://www.w3.org/2005/Atom}link")
    hub_link.set("href", "https://pubsubhubbub.appspot.com/")
    hub_link.set("rel", "hub")

    atom_link = ET.SubElement(channel, "{http://www.w3.org/2005/Atom}link")
    atom_link.set("href", f"{SITE_URL}/rss.xml")
    atom_link.set("rel", "self")
    atom_link.set("type", "application/rss+xml")

    default_image_url = f"{SITE_URL}/assets/wallpaper.jpg"

    # 1. Artigos Destacados / Post-Mortems Técnicos
    for feat in FEATURED_POSTS:
        item = ET.SubElement(channel, "item")
        ET.SubElement(item, "title").text = feat["title"]
        ET.SubElement(item, "link").text = feat["url"]
        guid_el = ET.SubElement(item, "guid")
        guid_el.text = feat["guid"]
        guid_el.set("isPermaLink", "false")
        ET.SubElement(item, "pubDate").text = feat["date"]
        ET.SubElement(item, "description").text = feat["body"]

        # Imagem destacada / Media Enclosure (W3C / Google News / Flipboard compatível)
        enc = ET.SubElement(item, "enclosure")
        enc.set("url", default_image_url)
        enc.set("length", "70707")
        enc.set("type", "image/jpeg")

    # 2. Commits Técnicos & Releases
    commits = get_announced_commits(15)
    for c in commits:
        item = ET.SubElement(channel, "item")
        ET.SubElement(item, "title").text = c["title"]
        commit_url = f"{REPO_URL}/commit/{c['hash']}"
        ET.SubElement(item, "link").text = commit_url
        guid_el = ET.SubElement(item, "guid")
        guid_el.text = c["hash"]
        guid_el.set("isPermaLink", "false")
        ET.SubElement(item, "pubDate").text = c["date"]
        ET.SubElement(item, "description").text = c["body"]

        # Imagem destacada padrão para agregadores
        enc = ET.SubElement(item, "enclosure")
        enc.set("url", default_image_url)
        enc.set("length", "70707")
        enc.set("type", "image/jpeg")

    tree = ET.ElementTree(rss)
    ET.indent(tree, space="  ")

    tree.write(FEED_PATH, encoding="utf-8", xml_declaration=True)
    tree.write(RSS_PATH, encoding="utf-8", xml_declaration=True)
    print(f"  [PASS] {len(FEATURED_POSTS) + len(commits)} entradas gravadas com sucesso em feed.xml e rss.xml!")

if __name__ == "__main__":
    build_rss()
