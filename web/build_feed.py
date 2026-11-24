#!/usr/bin/env python3
"""
web/build_feed.py

Gerador automático de:
  - web/artigos/index.html (Listagem visual moderna de todos os artigos)
  - web/feed.xml          (Feed RSS 2.0 / Atom)
  - web/rss.xml           (Feed RSS 2.0 oficial W3C / WebSub)
  - web/sitemap.xml       (Sitemap XML com Google News e SEO)

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
INDEX_PATH = ARTIGOS_DIR / "index.html"
FEED_PATH = WEB_DIR / "feed.xml"
RSS_PATH = WEB_DIR / "rss.xml"
SITEMAP_PATH = WEB_DIR / "sitemap.xml"

DEFAULT_IMAGE_URL = f"{SITE_URL}/assets/wallpaper.jpg"

CATEGORY_COLORS = {
    "POST-MORTEM": ("text-cyan-400", "bg-cyan-950", "border-cyan-800", "group-hover:text-cyan-300"),
    "REDES & DRIVERS": ("text-emerald-400", "bg-emerald-950", "border-emerald-800", "group-hover:text-emerald-300"),
    "REDES": ("text-emerald-400", "bg-emerald-950", "border-emerald-800", "group-hover:text-emerald-300"),
    "COMPUTAÇÃO GRÁFICA & UI": ("text-purple-400", "bg-purple-950", "border-purple-800", "group-hover:text-purple-300"),
    "COMPUTAÇÃO GRÁFICA": ("text-purple-400", "bg-purple-950", "border-purple-800", "group-hover:text-purple-300"),
    "ARMAZENAMENTO & KERNEL": ("text-blue-400", "bg-blue-950", "border-blue-800", "group-hover:text-blue-300"),
    "ARMAZENAMENTO": ("text-blue-400", "bg-blue-950", "border-blue-800", "group-hover:text-blue-300"),
    "SISTEMAS DE ARQUIVOS": ("text-blue-400", "bg-blue-950", "border-blue-800", "group-hover:text-blue-300"),
    "CRIPTOGRAFIA": ("text-emerald-400", "bg-emerald-950", "border-emerald-800", "group-hover:text-emerald-300"),
    "MICROKERNEL": ("text-amber-400", "bg-amber-950", "border-amber-800", "group-hover:text-amber-300"),
    "JINN BROWSER": ("text-rose-400", "bg-rose-950", "border-rose-800", "group-hover:text-rose-300"),
    "PACOTES & ATUALIZAÇÃO": ("text-indigo-400", "bg-indigo-950", "border-indigo-800", "group-hover:text-indigo-300"),
}

DEFAULT_COLOR = ("text-cyan-400", "bg-cyan-950", "border-cyan-800", "group-hover:text-cyan-300")

def extract_article_meta(html_path: Path):
    """Extrai metadados estruturados (title, description, canonical, date, tag) de um arquivo HTML."""
    try:
        content = html_path.read_text(encoding="utf-8", errors="ignore")
    except Exception as e:
        print(f"[WARN] Erro ao ler {html_path}: {e}")
        return None

    # Título
    title_match = re.search(r"<title>(.*?)</title>", content, re.IGNORECASE | re.DOTALL)
    title = title_match.group(1).strip() if title_match else html_path.stem.replace("-", " ").title()
    title = re.sub(r"\s*[—–-]\s*Jinn\s*OS.*$", "", title).strip()

    # Descrição
    desc_match = re.search(r'<meta\s+name=["\']description["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE)
    description = desc_match.group(1).strip() if desc_match else title

    # Canonical URL
    canon_match = re.search(r'<link\s+rel=["\']canonical["\']\s+href=["\'](.*?)["\']', content, re.IGNORECASE)
    url = canon_match.group(1).strip() if canon_match else f"{SITE_URL}/artigos/{html_path.name}"

    # Tag / Categoria
    tag_match = re.search(r'<meta\s+name=["\']article:tag["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE)
    if tag_match:
        tag = tag_match.group(1).split(",")[0].strip().upper()
    else:
        # Dedução simples por slug se não houver meta
        slug = html_path.stem
        if "pic" in slug or "post-mortem" in slug:
            tag = "POST-MORTEM"
        elif "tls" in slug:
            tag = "CRIPTOGRAFIA"
        elif "browser" in slug or "navegador" in slug:
            tag = "JINN BROWSER"
        elif "simd" in slug or "compositor" in slug or "glass" in slug:
            tag = "COMPUTAÇÃO GRÁFICA & UI"
        elif "fat32" in slug or "vfs" in slug or "resiliencia" in slug:
            tag = "ARMAZENAMENTO & KERNEL"
        elif "driver" in slug or "jaha" in slug:
            tag = "MICROKERNEL"
        elif "arp" in slug or "dns" in slug or "e1000" in slug:
            tag = "REDES & DRIVERS"
        elif "jpkg" in slug or "pacote" in slug:
            tag = "PACOTES & ATUALIZAÇÃO"
        else:
            tag = "ARTIGO TÉCNICO"

    # Data de modificação / publicação
    pub_date_rfc2822 = "Tue, 24 Nov 2026 18:00:00 -0300"
    pub_date_iso = "2026-11-24"

    # 1. Verifica meta article:published_time primeiro
    pub_match = re.search(r'<meta\s+name=["\']article:published_time["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE)
    if pub_match:
        raw_time = pub_match.group(1).strip()
        try:
            dt = datetime.fromisoformat(raw_time)
            pub_date_rfc2822 = dt.strftime("%a, %d %b %Y %H:%M:%S %z")
            pub_date_iso = dt.strftime("%Y-%m-%d")
        except Exception:
            pub_date_iso = raw_time[:10]
    else:
        # 2. Prioriza git log se disponível
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
        "tag": tag,
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

    # Ordena com os mais recentes primeiro
    articles.sort(key=lambda a: (a["date_iso"], a["slug"]), reverse=True)
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

def build_artigos_index(articles):
    """Gera web/artigos/index.html contendo cards visuais modernos para todos os artigos."""
    print(f"[+] Gerando Índice Visual em {INDEX_PATH} ({len(articles)} artigos)...")

    cards_html = []
    for a in articles:
        colors = CATEGORY_COLORS.get(a["tag"], DEFAULT_COLOR)
        tag_color, tag_bg, tag_border, hover_title = colors

        card = f"""      <!-- Card: {a['slug']} -->
      <a href="/artigos/{a['filename']}" class="group block p-6 rounded-xl bg-slate-900/60 border border-slate-800 hover:border-cyan-500/50 hover:bg-slate-900 transition-all">
        <div class="flex items-center gap-2 text-xs font-mono {tag_color} mb-2">
          <span class="{tag_bg} border {tag_border} px-2 py-0.5 rounded">{a['tag']}</span>
          <span>Novembro 2026</span>
        </div>
        <h2 class="text-xl font-bold text-white {hover_title} transition-colors leading-snug">
          {a['title']}
        </h2>
        <p class="text-slate-400 text-sm mt-3 line-clamp-3 leading-relaxed">
          {a['description']}
        </p>
        <div class="mt-4 flex items-center gap-1 text-xs font-semibold text-cyan-400">
          <span>Ler artigo completo</span>
          <i data-lucide="arrow-right" class="w-3.5 h-3.5 group-hover:translate-x-1 transition-transform"></i>
        </div>
      </a>"""
        cards_html.append(card)

    cards_block = "\n\n".join(cards_html)

    html_content = f"""<!DOCTYPE html>
<html lang="pt-BR">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Artigos Técnicos &amp; Devlogs — Jinn OS</title>
  <meta name="description" content="Arquivo central de artigos técnicos, post-mortems, arquitetura de sistemas e auditorias de engenharia do microkernel Jinn OS e ecossistema Aladdin.">
  <link rel="canonical" href="https://jinn-os.dev.br/artigos/">
  <link rel="stylesheet" href="/style.css">
  <link rel="alternate" type="application/rss+xml" title="Jinn OS — Devlog &amp; Releases" href="https://jinn-os.dev.br/feed.xml">
  <link rel="alternate" type="application/rss+xml" title="Jinn OS — RSS Feed" href="https://jinn-os.dev.br/rss.xml">

  <!-- Open Graph -->
  <meta property="og:type" content="website">
  <meta property="og:url" content="https://jinn-os.dev.br/artigos/">
  <meta property="og:title" content="Artigos Técnicos &amp; Devlogs — Jinn OS">
  <meta property="og:description" content="Engenharia de sistemas operacionais em Rust bare-metal: post-mortems, microkernels, criptografia e computação heterogênea.">
  <meta property="og:image" content="https://jinn-os.dev.br/assets/wallpaper.jpg">

  <!-- Google tag (gtag.js) - Google Ads -->
  <script async src="https://www.googletagmanager.com/gtm.js?id=AW-884426711"></script>
  <script>
    window.dataLayer = window.dataLayer || [];
    function gtag() {{ dataLayer.push(arguments); }}
    gtag('js', new Date());
    gtag('config', 'AW-884426711');
  </script>

  <!-- Tailwind CSS & Lucide Icons -->
  <script src="https://cdn.tailwindcss.com"></script>
  <script src="https://unpkg.com/lucide@latest"></script>
</head>
<body class="bg-slate-950 text-slate-100 min-h-screen font-sans selection:bg-cyan-500 selection:text-black flex flex-col justify-between">

  <!-- Cabeçalho de Navegação -->
  <header class="border-b border-slate-800/80 bg-slate-900/60 backdrop-blur sticky top-0 z-50">
    <div class="max-w-5xl mx-auto px-6 h-16 flex items-center justify-between">
      <a href="/" class="flex items-center gap-2 hover:text-cyan-400 transition-colors">
        <i data-lucide="arrow-left" class="w-4 h-4 text-cyan-400"></i>
        <span class="font-bold tracking-wide">Jinn OS</span>
        <span class="text-xs font-mono bg-cyan-950 text-cyan-300 px-2 py-0.5 rounded border border-cyan-800">Artigos</span>
      </a>
      <div class="flex items-center gap-4 text-xs font-mono">
        <a href="http://validator.w3.org/feed/check.cgi?url=https%3A//jinn-os.dev.br/rss.xml"><img src="/valid-rss-rogers.png" alt="[Valid RSS]" title="Validate my RSS feed" style="height: 15px;" /></a>
        <a href="/feed.xml" target="_blank" class="flex items-center gap-1.5 text-amber-400 hover:text-amber-300 transition-colors">
          <i data-lucide="rss" class="w-3.5 h-3.5"></i>
          <span>RSS Feed</span>
        </a>
        <span class="text-slate-600">|</span>
        <a href="https://github.com/GeovaneEstanislau/Jinn_Project" target="_blank" rel="noopener noreferrer" class="flex items-center gap-1 text-slate-400 hover:text-white transition-colors">
          <i data-lucide="github" class="w-3.5 h-3.5"></i>
          <span>GitHub</span>
        </a>
      </div>
    </div>
  </header>

  <!-- Conteúdo Principal -->
  <main class="max-w-5xl mx-auto px-6 py-12 flex-1 w-full">
    <div class="mb-10 text-center sm:text-left">
      <h1 class="text-3xl sm:text-4xl font-extrabold text-white tracking-tight">
        Acervo Técnico de Engenharia &amp; Devlogs
      </h1>
      <p class="text-slate-400 mt-2 max-w-2xl leading-relaxed text-sm">
        Relatórios de auditoria, post-mortems de depuração bare-metal e decisões de arquitetura do <strong>Jinn Microkernel</strong> e do ecossistema gráfico <strong>Aladdin</strong>.
      </p>
    </div>

    <!-- Grade de Artigos Dinâmica -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
{cards_block}
    </div>
  </main>

  <!-- Rodapé -->
  <footer class="border-t border-slate-800 mt-16 py-8 text-xs text-slate-400 font-mono">
    <div class="max-w-5xl mx-auto px-6 flex flex-col sm:flex-row items-center justify-between gap-4">
      <div class="flex items-center gap-3">
        <span>Jinn OS Sovereign Systems • 2026</span>
        <a href="http://validator.w3.org/feed/check.cgi?url=https%3A//jinn-os.dev.br/rss.xml"><img src="/valid-rss-rogers.png" alt="[Valid RSS]" title="Validate my RSS feed" style="height: 15px;" /></a>
      </div>
      <div class="flex items-center gap-4">
        <a href="/feed.xml" class="text-amber-400 hover:underline flex items-center gap-1">
          <i data-lucide="rss" class="w-3.5 h-3.5"></i>
          <span>Assinar Feed</span>
        </a>
        <a href="/" class="text-cyan-400 hover:underline">Área de Trabalho Virtual</a>
      </div>
    </div>
  </footer>

  <script>
    lucide.createIcons();
  </script>
</body>
</html>
"""
    INDEX_PATH.write_text(html_content, encoding="utf-8")
    print(f"  [PASS] {INDEX_PATH} gerado com sucesso!")

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
        print(f"  - [{a['date_iso']}] [{a['tag']}] {a['title']} -> {a['url']}")

    commits = get_announced_commits(10)
    build_artigos_index(articles)
    build_rss_feeds(articles, commits)
    build_sitemap(articles)
    print("[+] Concluído com sucesso!")

if __name__ == "__main__":
    main()
