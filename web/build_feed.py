#!/usr/bin/env python3
"""
web/build_feed.py

Gerador automático de:
  - web/feed.xml   (Feed RSS 2.0 / Atom)
  - web/rss.xml    (Feed RSS 2.0 oficial W3C / WebSub)
  - web/sitemap.xml (Sitemap XML com Google News e SEO)

Escaneia dinamicamente todos os artigos presentes em 'web/artigos/*.html',
permitindo adicionar centenas ou milhares de novos artigos sem manutenção manual!
"""

import os
import re
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path
from datetime import datetime, timezone

SITE_URL = "https://jinn-os.dev.br"
REPO_URL = "https://github.com/GeovaneEstanislau/Jinn_Project"
WEB_DIR = Path(__file__).resolve().parent
ARTIGOS_DIR = WEB_DIR / "artigos"
FEED_PATH = WEB_DIR / "feed.xml"
RSS_PATH = WEB_DIR / "rss.xml"
SITEMAP_PATH = WEB_DIR / "sitemap.xml"

DEFAULT_IMAGE_URL = f"{SITE_URL}/assets/wallpaper.jpg"

def extract_article_meta(html_path: Path):
    """Extrai metadados estruturados (title, description, canonical, date) de um arquivo HTML."""
    try:
        content = html_path.read_text(encoding="utf-8", errors="ignore")
    except Exception as e:
        print(f"[WARN] Erro ao ler {html_path}: {e}")
        return None

    # Título
    title_match = re.search(r"<title>(.*?)</title>", content, re.IGNORECASE | re.DOTALL)
    title = title_match.group(1).strip() if title_match else html_path.stem.replace("-", " ").title()
    # Remove sufixo padrão de marca se presente
    title = re.sub(r"\s*[—–-]\s*Jinn\s*OS.*$", "", title).strip()

    # Descrição
    desc_match = re.search(r'<meta\s+name=["\']description["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE)
    description = desc_match.group(1).strip() if desc_match else title

    # Canonical URL
    canon_match = re.search(r'<link\s+rel=["\']canonical["\']\s+href=["\'](.*?)["\']', content, re.IGNORECASE)
    url = canon_match.group(1).strip() if canon_match else f"{SITE_URL}/artigos/{html_path.name}"

    # Data de modificação / publicação
    # Prioriza git log para determinar a data exata do arquivo
    pub_date_rfc2822 = "Mon, 23 Nov 2026 12:00:00 -0300"
    pub_date_iso = "2026-11-23"
    try:
        git_date = subprocess.check_output(
            ["git", "log", "-1", "--format=%ad", "--date=rfc2822", str(html_path)],
            text=True, encoding="utf-8"
        ).strip()
        if git_date:
            pub_date_rfc2822 = git_date
            dt = datetime.strptime(git_date[:25].strip(), "%a, %d %b %Y %H:%M:%S")
            pub_date_iso = dt.strftime("%Y-%m-%d")
    except Exception:
        stat = html_path.stat()
        mtime = datetime.fromtimestamp(stat.st_mtime, timezone.utc)
        pub_date_rfc2822 = mtime.strftime("%a, %d %b %Y %H:%M:%S +0000")
        pub_date_iso = mtime.strftime("%Y-%m-%d")

    guid = f"jinn-os-artigo-{html_path.stem}"

    return {
        "slug": html_path.stem,
        "filename": html_path.name,
        "title": title,
        "description": description,
        "url": url,
        "guid": guid,
        "date_rfc2822": pub_date_rfc2822,
        "date_iso": pub_date_iso,
    }

def get_all_articles():
    """Descobre e ordena todos os artigos técnicos em web/artigos/."""
    articles = []
    if not ARTIGOS_DIR.exists():
        return articles

    for f in sorted(ARTIGOS_DIR.glob("*.html")):
        if f.name == "index.html":
            continue
        meta = extract_article_meta(f)
        if meta:
            articles.append(meta)

    # Ordena com os mais recentes ou específicos primeiro
    return articles

def get_announced_commits(limit=10):
    """Extrai commits técnicos recentes com mensagens semânticas relevantes."""
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

def build_rss_feeds(articles, commits):
    """Gera feed.xml e rss.xml com total compatibilidade W3C e WebSub."""
    print(f"[+] Gerando RSS 2.0 e WebSub Feeds em {FEED_PATH} e {RSS_PATH}...")
    ET.register_namespace('atom', 'http://www.w3.org/2005/Atom')
    ET.register_namespace('media', 'http://search.yahoo.com/mrss/')

    rss = ET.Element("rss", version="2.0")
    channel = ET.SubElement(rss, "channel")
    ET.SubElement(channel, "title").text = "Jinn OS — Devlog & Microkernel Releases"
    ET.SubElement(channel, "link").text = SITE_URL
    ET.SubElement(channel, "description").text = (
        "Artigos de engenharia, arquitetura de microkernel, benchmarks de computação vetorial Zero-GPU, "
        "relatórios de isolamento de falhas e auditorias do sistema operacional Jinn OS."
    )
    ET.SubElement(channel, "language").text = "pt-br"

    # WebSub Hub oficial Google / PubSubHubbub
    hub_link = ET.SubElement(channel, "{http://www.w3.org/2005/Atom}link")
    hub_link.set("href", "https://pubsubhubbub.appspot.com/")
    hub_link.set("rel", "hub")

    atom_link = ET.SubElement(channel, "{http://www.w3.org/2005/Atom}link")
    atom_link.set("href", f"{SITE_URL}/rss.xml")
    atom_link.set("rel", "self")
    atom_link.set("type", "application/rss+xml")

    # 1. Artigos Técnicos em web/artigos/
    for art in articles:
        item = ET.SubElement(channel, "item")
        ET.SubElement(item, "title").text = art["title"]
        ET.SubElement(item, "link").text = art["url"]
        guid_el = ET.SubElement(item, "guid")
        guid_el.text = art["guid"]
        guid_el.set("isPermaLink", "false")
        ET.SubElement(item, "pubDate").text = art["date_rfc2822"]
        ET.SubElement(item, "description").text = art["description"]

        enc = ET.SubElement(item, "enclosure")
        enc.set("url", DEFAULT_IMAGE_URL)
        enc.set("length", "70707")
        enc.set("type", "image/jpeg")

    # 2. Commits Técnicos Recentes
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

        enc = ET.SubElement(item, "enclosure")
        enc.set("url", DEFAULT_IMAGE_URL)
        enc.set("length", "70707")
        enc.set("type", "image/jpeg")

    tree = ET.ElementTree(rss)
    ET.indent(tree, space="  ")
    tree.write(FEED_PATH, encoding="utf-8", xml_declaration=True)
    tree.write(RSS_PATH, encoding="utf-8", xml_declaration=True)
    print(f"  [PASS] {len(articles)} artigos e {len(commits)} commits gravados em feed.xml e rss.xml!")

def build_sitemap(articles):
    """Gera sitemap.xml com suporte a Google News e indexação completa de artigos."""
    print(f"[+] Gerando Sitemap SEO / Google News em {SITEMAP_PATH}...")
    ET.register_namespace('', 'http://www.sitemaps.org/schemas/sitemap/0.9')
    ET.register_namespace('news', 'http://www.google.com/schemas/sitemap-news/0.9')

    urlset = ET.Element("{http://www.sitemaps.org/schemas/sitemap/0.9}urlset")

    # Página Inicial
    url_home = ET.SubElement(urlset, "{http://www.sitemaps.org/schemas/sitemap/0.9}url")
    ET.SubElement(url_home, "{http://www.sitemaps.org/schemas/sitemap/0.9}loc").text = f"{SITE_URL}/"
    ET.SubElement(url_home, "{http://www.sitemaps.org/schemas/sitemap/0.9}lastmod").text = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    ET.SubElement(url_home, "{http://www.sitemaps.org/schemas/sitemap/0.9}changefreq").text = "daily"
    ET.SubElement(url_home, "{http://www.sitemaps.org/schemas/sitemap/0.9}priority").text = "1.0"

    # Índice de Artigos
    url_artigos = ET.SubElement(urlset, "{http://www.sitemaps.org/schemas/sitemap/0.9}url")
    ET.SubElement(url_artigos, "{http://www.sitemaps.org/schemas/sitemap/0.9}loc").text = f"{SITE_URL}/artigos/"
    ET.SubElement(url_artigos, "{http://www.sitemaps.org/schemas/sitemap/0.9}lastmod").text = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    ET.SubElement(url_artigos, "{http://www.sitemaps.org/schemas/sitemap/0.9}changefreq").text = "daily"
    ET.SubElement(url_artigos, "{http://www.sitemaps.org/schemas/sitemap/0.9}priority").text = "0.9"

    # Cada Artigo Técnico
    for art in articles:
        url_art = ET.SubElement(urlset, "{http://www.sitemaps.org/schemas/sitemap/0.9}url")
        ET.SubElement(url_art, "{http://www.sitemaps.org/schemas/sitemap/0.9}loc").text = art["url"]
        ET.SubElement(url_art, "{http://www.sitemaps.org/schemas/sitemap/0.9}lastmod").text = art["date_iso"]
        ET.SubElement(url_art, "{http://www.sitemaps.org/schemas/sitemap/0.9}changefreq").text = "weekly"
        ET.SubElement(url_art, "{http://www.sitemaps.org/schemas/sitemap/0.9}priority").text = "0.8"

        # Tag Google News
        news = ET.SubElement(url_art, "{http://www.google.com/schemas/sitemap-news/0.9}news")
        pub = ET.SubElement(news, "{http://www.google.com/schemas/sitemap-news/0.9}publication")
        ET.SubElement(pub, "{http://www.google.com/schemas/sitemap-news/0.9}name").text = "Jinn OS Devlog"
        ET.SubElement(pub, "{http://www.google.com/schemas/sitemap-news/0.9}language").text = "pt-BR"
        ET.SubElement(news, "{http://www.google.com/schemas/sitemap-news/0.9}publication_date").text = art["date_iso"]
        ET.SubElement(news, "{http://www.google.com/schemas/sitemap-news/0.9}title").text = art["title"]

    tree = ET.ElementTree(urlset)
    ET.indent(tree, space="  ")
    tree.write(SITEMAP_PATH, encoding="utf-8", xml_declaration=True)
    print(f"  [PASS] {len(articles) + 2} URLs catalogadas com sucesso em sitemap.xml!")

def main():
    print("==================================================================")
    print("  JINN OS — GERADOR DINÂMICO DE FEEDS & SITEMAP MULTI-ARTIGOS")
    print("==================================================================")
    articles = get_all_articles()
    print(f"[+] Artigos identificados em {ARTIGOS_DIR}: {len(articles)}")
    for a in articles:
        print(f"  - [{a['date_iso']}] {a['title']} -> {a['url']}")

    commits = get_announced_commits(10)
    build_rss_feeds(articles, commits)
    build_sitemap(articles)
    print("[+] Concluído com sucesso!")

if __name__ == "__main__":
    main()
