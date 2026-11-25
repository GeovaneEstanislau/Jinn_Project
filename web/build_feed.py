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

import html as html_lib
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

MONTHS_PT = {
    "01": "Jan", "02": "Fev", "03": "Mar", "04": "Abr",
    "05": "Mai", "06": "Jun", "07": "Jul", "08": "Ago",
    "09": "Set", "10": "Out", "11": "Nov", "12": "Dez",
}

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
    "HOMOLOGAÇÃO & SEGURANÇA": ("text-emerald-400", "bg-emerald-950", "border-emerald-800", "group-hover:text-emerald-300"),
    "ARQUITETURA & PERFORMANCE": ("text-cyan-400", "bg-cyan-950", "border-cyan-800", "group-hover:text-cyan-300"),
}

DEFAULT_COLOR = ("text-cyan-400", "bg-cyan-950", "border-cyan-800", "group-hover:text-cyan-300")


def classify_buckets(tag: str, all_tags: list[str], slug: str, title: str) -> list[str]:
    """Classifica um artigo nos filtros oficiais da central para navegação escalável."""
    haystack = " ".join([tag, *all_tags, slug, title]).lower()
    buckets = []
    if any(k in haystack for k in ("microkernel", "ipc", "spsc", "io_uring", "zero-copy", "fat32", "ext2", "vfs", "driver", "jpkg", "pacote", "armazenamento", "pic", "eoi")):
        buckets.append("microkernel")
    if any(k in haystack for k in ("rede", "dns", "arp", "e1000", "virtio", "browser", "navegador", "http", "tcp")):
        buckets.append("redes")
    if any(k in haystack for k in ("simd", "zero-gpu", "blur", "glass", "acrílico", "acrilico", "compositor", "jhcs", "aladdin", "gráfica", "grafica", "frosted")):
        buckets.append("simd")
    if any(k in haystack for k in ("homolog", "segurança", "seguranca", "auditoria", "zero-idle", "zero-trust", "efault", "sha-256", "sha256", "tls", "jaha", "criptografia", "post-mortem", "estabilidade")):
        buckets.append("homologacao")
    if not buckets:
        buckets.append("microkernel")
    return buckets


def format_date_pt(date_iso: str) -> str:
    """Converte YYYY-MM-DD para leitura curta em português (ex: 24 Nov 2026)."""
    parts = date_iso.split("-")
    if len(parts) == 3:
        y, m, d = parts
        return f"{int(d)} {MONTHS_PT.get(m, m)} {y}"
    return date_iso


def extract_article_meta(html_path: Path):
    """Extrai metadados estruturados (title, description, canonical, date, tag, reading_min, categories) de um arquivo HTML."""
    try:
        content = html_path.read_text(encoding="utf-8", errors="ignore")
    except Exception as e:
        print(f"[WARN] Erro ao ler {html_path}: {e}")
        return None

    # Título
    title_match = re.search(r"<title>(.*?)</title>", content, re.IGNORECASE | re.DOTALL)
    title = html_lib.unescape(title_match.group(1).strip()) if title_match else html_path.stem.replace("-", " ").title()
    title = re.sub(r"\s*[—–-]\s*Jinn\s*OS.*$", "", title).strip()

    # Descrição
    desc_match = re.search(r'<meta\s+name=["\']description["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE)
    description = html_lib.unescape(desc_match.group(1).strip()) if desc_match else title

    # Canonical URL
    canon_match = re.search(r'<link\s+rel=["\']canonical["\']\s+href=["\'](.*?)["\']', content, re.IGNORECASE)
    url = canon_match.group(1).strip() if canon_match else f"{SITE_URL}/artigos/{html_path.name}"

    # Tags / Categorias
    tag_match = re.search(r'<meta\s+name=["\']article:tag["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE)
    if tag_match:
        all_tags = [t.strip().upper() for t in html_lib.unescape(tag_match.group(1)).split(",") if t.strip()]
        tag = all_tags[0] if all_tags else "ARTIGO TÉCNICO"
    else:
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
        all_tags = [tag]

    # Tempo estimado de leitura (~200 palavras/min)
    article_body = re.search(r"<article[^>]*>(.*?)</article>", content, re.IGNORECASE | re.DOTALL)
    body_src = article_body.group(1) if article_body else content
    plain_text = re.sub(r"<script[^>]*>.*?</script>", " ", body_src, flags=re.IGNORECASE | re.DOTALL)
    plain_text = re.sub(r"<style[^>]*>.*?</style>", " ", plain_text, flags=re.IGNORECASE | re.DOTALL)
    plain_text = re.sub(r"<[^>]+>", " ", plain_text)
    words = len(re.findall(r"\w+", plain_text))
    read_minutes = max(2, round(words / 180))

    # Data de modificação / publicação
    pub_date_rfc2822 = "Tue, 24 Nov 2026 18:00:00 -0300"
    pub_date_iso = "2026-11-24"
    sort_ts = "2026-11-24T18:00:00"

    pub_match = re.search(r'<meta\s+name=["\']article:published_time["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE)
    if pub_match:
        raw_time = pub_match.group(1).strip()
        try:
            dt = datetime.fromisoformat(raw_time.replace("Z", "+00:00"))
            pub_date_rfc2822 = dt.strftime("%a, %d %b %Y %H:%M:%S %z")
            pub_date_iso = dt.strftime("%Y-%m-%d")
            sort_ts = dt.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S")
        except Exception:
            pub_date_iso = raw_time[:10]
            sort_ts = raw_time[:19]
    else:
        try:
            git_date = subprocess.check_output(
                ["git", "log", "-1", "--format=%ad", "--date=rfc2822", str(html_path)],
                text=True, encoding="utf-8"
            ).strip()
            if git_date:
                pub_date_rfc2822 = git_date
                dt = datetime.strptime(git_date[:25].strip(), "%a, %d %b %Y %H:%M:%S")
                pub_date_iso = dt.strftime("%Y-%m-%d")
                sort_ts = dt.strftime("%Y-%m-%dT%H:%M:%S")
        except Exception:
            stat = html_path.stat()
            mtime = datetime.fromtimestamp(stat.st_mtime, timezone.utc)
            pub_date_rfc2822 = mtime.strftime("%a, %d %b %Y %H:%M:%S +0000")
            pub_date_iso = mtime.strftime("%Y-%m-%d")
            sort_ts = mtime.strftime("%Y-%m-%dT%H:%M:%S")

    guid = f"jinn-os-artigo-{html_path.stem}"
    buckets = classify_buckets(tag, all_tags, html_path.stem, title)

    return {
        "slug": html_path.stem,
        "filename": html_path.name,
        "title": title,
        "description": description,
        "tag": tag,
        "all_tags": all_tags,
        "buckets": buckets,
        "read_minutes": read_minutes,
        "date_human": format_date_pt(pub_date_iso),
        "sort_ts": sort_ts,
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

    # Ordena com os mais recentes primeiro (timestamp + slug)
    articles.sort(key=lambda a: (a["sort_ts"], a["slug"]), reverse=True)
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
    """Gera web/artigos/index.html escalável com scroll fluido, barra de progresso, busca instantânea e filtros."""
    print(f"[+] Gerando Índice Visual em {INDEX_PATH} ({len(articles)} artigos)...")

    cards_html = []
    for a in articles:
        colors = CATEGORY_COLORS.get(a["tag"], DEFAULT_COLOR)
        tag_color, tag_bg, tag_border, hover_title = colors
        tags_text = " ".join(a["all_tags"])
        search_blob = html_lib.escape(f"{a['title']} {a['description']} {tags_text}".lower(), quote=True)
        categories_attr = html_lib.escape(" ".join(a["buckets"]), quote=True)
        title_esc = html_lib.escape(a["title"])
        desc_esc = html_lib.escape(a["description"])
        tag_esc = html_lib.escape(a["tag"])
        extra_tags = "".join(
            f'<span class="px-2 py-0.5 rounded bg-slate-800/80 border border-slate-700/70 text-[10px] font-mono text-slate-300">{html_lib.escape(t)}</span>'
            for t in a["all_tags"][1:4]
        )

        card = f"""      <!-- Card: {a['slug']} -->
      <a href="/artigos/{a['filename']}" data-article-card data-categories="{categories_attr}" data-search="{search_blob}" class="group flex flex-col justify-between p-6 rounded-xl bg-slate-900/65 border border-slate-800 hover:border-cyan-500/60 hover:bg-slate-900/95 transition-all duration-200 shadow-sm hover:shadow-cyan-950/30">
        <div>
          <div class="flex flex-wrap items-center justify-between gap-2 text-xs font-mono {tag_color} mb-3">
            <span class="{tag_bg} border {tag_border} px-2.5 py-0.5 rounded-full font-semibold">{tag_esc}</span>
            <div class="flex items-center gap-2 text-slate-400">
              <time datetime="{a['date_iso']}">{a['date_human']}</time>
              <span>•</span>
              <span class="inline-flex items-center gap-1 text-slate-300"><i data-lucide="clock" class="w-3 h-3 text-cyan-400"></i>{a['read_minutes']} min</span>
            </div>
          </div>
          <h2 class="text-lg sm:text-xl font-bold text-white {hover_title} transition-colors leading-snug">
            {title_esc}
          </h2>
          <p class="text-slate-400 text-sm mt-3 line-clamp-3 leading-relaxed">
            {desc_esc}
          </p>
        </div>
        <div class="mt-4 pt-3 border-t border-slate-800/70 flex flex-wrap items-center justify-between gap-2">
          <div class="flex flex-wrap items-center gap-1.5">{extra_tags}</div>
          <div class="flex items-center gap-1 text-xs font-semibold text-cyan-400 ml-auto">
            <span>Ler artigo</span>
            <i data-lucide="arrow-right" class="w-3.5 h-3.5 group-hover:translate-x-1 transition-transform"></i>
          </div>
        </div>
      </a>"""
        cards_html.append(card)

    cards_block = "\n\n".join(cards_html)
    total_count = len(articles)

    html_content = f"""<!DOCTYPE html>
<html lang="pt-BR" class="scroll-smooth">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Artigos Técnicos &amp; Devlogs — Jinn OS</title>
  <meta name="description" content="Arquivo central de artigos técnicos, post-mortems, arquitetura de sistemas e auditorias de engenharia do microkernel Jinn OS e ecossistema Aladdin.">
  <link rel="canonical" href="https://jinn-os.dev.br/artigos/">
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
  <style>
    html {{
      scroll-behavior: smooth;
      overflow-y: auto;
      height: auto;
    }}
    body {{
      overflow-x: hidden;
      overflow-y: auto;
      min-height: 100vh;
      height: auto;
      user-select: text;
    }}
  </style>
</head>
<body id="topo" class="bg-slate-950 text-slate-100 min-h-screen font-sans selection:bg-cyan-500 selection:text-black flex flex-col">

  <!-- Barra de Progresso de Leitura/Rolagem no Topo -->
  <div class="fixed top-0 left-0 right-0 h-1 bg-slate-900/90 z-[60]">
    <div id="scroll-progress" class="h-full w-0 bg-gradient-to-r from-cyan-400 via-sky-400 to-emerald-400 transition-[width] duration-75"></div>
  </div>

  <!-- Cabeçalho Fixo Compacto (Sticky Header) com Busca e Filtros -->
  <header class="border-b border-slate-800/80 bg-slate-950/90 backdrop-blur-md sticky top-0 z-50">
    <div class="max-w-6xl mx-auto px-4 sm:px-6 h-14 flex items-center justify-between gap-3">
      <a href="/" class="flex items-center gap-2 hover:text-cyan-400 transition-colors shrink-0">
        <i data-lucide="arrow-left" class="w-4 h-4 text-cyan-400"></i>
        <span class="font-bold tracking-wide">Jinn OS</span>
        <span class="text-xs font-mono bg-cyan-950 text-cyan-300 px-2 py-0.5 rounded border border-cyan-800">Artigos</span>
      </a>
      <div class="flex items-center gap-3 sm:gap-4 text-xs font-mono overflow-x-auto">
        <span id="articles-counter" class="hidden sm:inline-block text-slate-400">{total_count} publicações</span>
        <a href="http://validator.w3.org/feed/check.cgi?url=https%3A//jinn-os.dev.br/rss.xml" class="hidden md:inline-block shrink-0"><img src="/valid-rss-rogers.png" alt="[Valid RSS]" title="Validate my RSS feed" style="height: 15px;" /></a>
        <a href="/feed.xml" target="_blank" class="flex items-center gap-1.5 text-amber-400 hover:text-amber-300 transition-colors shrink-0">
          <i data-lucide="rss" class="w-3.5 h-3.5"></i>
          <span>RSS</span>
        </a>
        <span class="text-slate-700">|</span>
        <a href="https://github.com/GeovaneEstanislau/Jinn_Project" target="_blank" rel="noopener noreferrer" class="flex items-center gap-1 text-slate-400 hover:text-white transition-colors shrink-0">
          <i data-lucide="github" class="w-3.5 h-3.5"></i>
          <span>GitHub</span>
        </a>
      </div>
    </div>

    <!-- Barra de Busca Instantânea + Filtros por Categoria -->
    <div class="border-t border-slate-800/60 bg-slate-900/75">
      <div class="max-w-6xl mx-auto px-4 sm:px-6 py-2.5 flex flex-col lg:flex-row lg:items-center gap-2.5 justify-between">
        <div class="relative flex-1 max-w-md">
          <i data-lucide="search" class="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none"></i>
          <input id="article-search" type="search" placeholder="Buscar por título, syscall, subsistema ou tag..."
            class="w-full pl-9 pr-3 py-1.5 text-xs sm:text-sm rounded-lg bg-slate-950/90 border border-slate-700/80 text-slate-100 placeholder-slate-500 focus:outline-none focus:border-cyan-400 transition-colors" />
        </div>
        <nav aria-label="Filtros por categoria" class="flex flex-wrap items-center gap-1.5 text-xs font-mono">
          <button type="button" data-filter="all" class="filter-pill px-3 py-1 rounded-full border border-cyan-500/70 bg-cyan-950/90 text-cyan-300 font-semibold transition-colors">Todos</button>
          <button type="button" data-filter="microkernel" class="filter-pill px-3 py-1 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-slate-500 transition-colors">Microkernel &amp; IPC</button>
          <button type="button" data-filter="redes" class="filter-pill px-3 py-1 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-slate-500 transition-colors">Redes &amp; DNS</button>
          <button type="button" data-filter="simd" class="filter-pill px-3 py-1 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-slate-500 transition-colors">SIMD &amp; Zero-GPU</button>
          <button type="button" data-filter="homologacao" class="filter-pill px-3 py-1 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-slate-500 transition-colors">Homologação &amp; Segurança</button>
        </nav>
      </div>
    </div>
  </header>

  <!-- Conteúdo Principal com Rolagem Fluida Nativa -->
  <main class="max-w-6xl mx-auto px-4 sm:px-6 py-8 sm:py-10 flex-1 w-full">
    <div class="mb-8 flex flex-col sm:flex-row sm:items-end justify-between gap-4">
      <div>
        <h1 class="text-2xl sm:text-4xl font-extrabold text-white tracking-tight">
          Acervo Técnico de Engenharia &amp; Devlogs
        </h1>
        <p class="text-slate-400 mt-2 max-w-2xl leading-relaxed text-sm">
          Relatórios de homologação, benchmarks bare-metal, post-mortems e arquitetura do <strong>Jinn Microkernel</strong>, <strong>JHCS</strong> e compositor <strong>Aladdin</strong>.
        </p>
      </div>
      <div class="text-xs font-mono text-slate-400 shrink-0">
        Mostrando <span id="visible-count" class="text-cyan-400 font-bold">{total_count}</span> de {total_count} artigos
      </div>
    </div>

    <!-- Container Escalável de Artigos -->
    <div id="articles-grid" class="grid grid-cols-1 md:grid-cols-2 gap-5 sm:gap-6">
{cards_block}
    </div>

    <!-- Estado Vazio de Busca -->
    <div id="empty-state" class="hidden text-center py-16 px-4 rounded-xl border border-slate-800 bg-slate-900/40 mt-6">
      <p class="text-slate-300 font-semibold text-base">Nenhum artigo encontrado para este filtro.</p>
      <p class="text-slate-500 text-sm mt-1">Tente limpar a busca ou voltar para a categoria Todos.</p>
      <button type="button" id="reset-filters-btn" class="mt-4 px-4 py-1.5 rounded-lg bg-cyan-950 border border-cyan-700 text-cyan-300 text-xs font-mono hover:bg-cyan-900 transition-colors">
        Limpar filtros
      </button>
    </div>
  </main>

  <!-- Rodapé -->
  <footer class="border-t border-slate-800 mt-12 py-8 text-xs text-slate-400 font-mono">
    <div class="max-w-6xl mx-auto px-4 sm:px-6 flex flex-col sm:flex-row items-center justify-between gap-4">
      <div class="flex items-center gap-3">
        <span>Jinn OS Sovereign Systems • 2026</span>
        <a href="http://validator.w3.org/feed/check.cgi?url=https%3A//jinn-os.dev.br/rss.xml"><img src="/valid-rss-rogers.png" alt="[Valid RSS]" title="Validate my RSS feed" style="height: 15px;" /></a>
      </div>
      <div class="flex items-center gap-4">
        <a href="/rss.xml" class="text-emerald-400 hover:underline">RSS 2.0 (WebSub)</a>
        <a href="/feed.xml" class="text-amber-400 hover:underline flex items-center gap-1">
          <i data-lucide="rss" class="w-3.5 h-3.5"></i>
          <span>Assinar Feed</span>
        </a>
        <a href="/" class="text-cyan-400 hover:underline">Área de Trabalho Virtual</a>
      </div>
    </div>
  </footer>

  <!-- Botão Flutuante Voltar ao Topo ↑ -->
  <button type="button" id="back-to-top" aria-label="Voltar ao Topo"
    class="fixed bottom-6 right-6 z-50 px-4 py-2.5 rounded-full bg-slate-900/95 border border-cyan-500/60 text-cyan-300 text-xs font-mono font-semibold shadow-lg shadow-black/50 backdrop-blur-md hover:bg-cyan-950 hover:text-white transition-all duration-200 opacity-0 pointer-events-none translate-y-2">
    Voltar ao Topo ↑
  </button>

  <script>
    lucide.createIcons();

    (function () {{
      const progressBar = document.getElementById('scroll-progress');
      const backToTop = document.getElementById('back-to-top');
      const searchInput = document.getElementById('article-search');
      const pills = Array.from(document.querySelectorAll('.filter-pill'));
      const cards = Array.from(document.querySelectorAll('[data-article-card]'));
      const visibleCount = document.getElementById('visible-count');
      const articlesCounter = document.getElementById('articles-counter');
      const emptyState = document.getElementById('empty-state');
      const resetBtn = document.getElementById('reset-filters-btn');

      let activeFilter = 'all';

      function updateScrollUI() {{
        const doc = document.documentElement;
        const scrollTop = window.scrollY || doc.scrollTop || 0;
        const scrollHeight = Math.max(doc.scrollHeight - window.innerHeight, 1);
        const pct = Math.min(100, Math.max(0, (scrollTop / scrollHeight) * 100));
        if (progressBar) progressBar.style.width = pct.toFixed(1) + '%';
        if (backToTop) {{
          if (scrollTop > 260) {{
            backToTop.classList.remove('opacity-0', 'pointer-events-none', 'translate-y-2');
          }} else {{
            backToTop.classList.add('opacity-0', 'pointer-events-none', 'translate-y-2');
          }}
        }}
      }}

      window.addEventListener('scroll', updateScrollUI, {{ passive: true }});
      updateScrollUI();

      if (backToTop) {{
        backToTop.addEventListener('click', function () {{
          window.scrollTo({{ top: 0, behavior: 'smooth' }});
        }});
      }}

      function applyFilters() {{
        const q = (searchInput ? searchInput.value : '').trim().toLowerCase();
        let shown = 0;
        cards.forEach(function (card) {{
          const cats = (card.getAttribute('data-categories') || '').split(' ');
          const text = card.getAttribute('data-search') || '';
          const matchCat = activeFilter === 'all' || cats.includes(activeFilter);
          const matchText = !q || text.includes(q);
          if (matchCat && matchText) {{
            card.classList.remove('hidden');
            shown++;
          }} else {{
            card.classList.add('hidden');
          }}
        }});
        if (visibleCount) visibleCount.textContent = String(shown);
        if (articlesCounter) articlesCounter.textContent = shown + ' de ' + cards.length + ' publicações';
        if (emptyState) emptyState.classList.toggle('hidden', shown > 0);
      }}

      pills.forEach(function (btn) {{
        btn.addEventListener('click', function () {{
          activeFilter = btn.getAttribute('data-filter') || 'all';
          pills.forEach(function (p) {{
            const selected = p === btn;
            p.classList.toggle('border-cyan-500/70', selected);
            p.classList.toggle('bg-cyan-950/90', selected);
            p.classList.toggle('text-cyan-300', selected);
            p.classList.toggle('font-semibold', selected);
            p.classList.toggle('border-slate-700', !selected);
            p.classList.toggle('bg-slate-900', !selected);
            p.classList.toggle('text-slate-300', !selected);
          }});
          applyFilters();
        }});
      }});

      if (searchInput) searchInput.addEventListener('input', applyFilters);
      if (resetBtn) {{
        resetBtn.addEventListener('click', function () {{
          if (searchInput) searchInput.value = '';
          const allBtn = pills.find(function (p) {{ return p.getAttribute('data-filter') === 'all'; }});
          if (allBtn) allBtn.click();
        }});
      }}
    }})();
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
