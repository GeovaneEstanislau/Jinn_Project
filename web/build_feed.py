#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR Jinn-Commercial-1.0
"""
web/build_feed.py

Gerador automático e enriquecedor semântico do Jinn Engineering Observatory:
  - Injeta metadados Schema.org TechArticle (JSON-LD) e bloco de Ecossistemas &
    Colaboração Aberta em cada artigo em 'web/artigos/*.html' (de forma idempotente)
  - web/artigos/index.html (Central visual com busca e filtro por ecossistema)
  - web/feed.xml & web/rss.xml (RSS 2.0 + WebSub com ganhos mensuráveis e ecossistemas)
  - web/sitemap.xml (Google News Sitemap & SEO)
"""

import html as html_lib
import json
import os
import re
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path
from datetime import datetime, timezone

SITE_URL = "https://jinn-os.dev.br"
REPO_URL = "https://github.com/GeovaneEstanislau/Jinn_Project"
JHCS_REPO_URL = "https://github.com/GeovaneEstanislau/jhcs-rs"
WEB_DIR = Path(__file__).resolve().parent
ARTIGOS_DIR = WEB_DIR / "artigos"
INDEX_PATH = ARTIGOS_DIR / "index.html"
FEED_PATH = WEB_DIR / "feed.xml"
RSS_PATH = WEB_DIR / "rss.xml"
SITEMAP_PATH = WEB_DIR / "sitemap.xml"

DEFAULT_IMAGE_URL = f"{SITE_URL}/assets/wallpaper.jpg"
AUTHOR_NAME = "Geovane da Conceição Estanislau"
PUBLISHER_NAME = "Jinn Engineering Observatory"

TECHARTICLE_START = "<!-- Jinn Engineering Observatory: Schema.org TechArticle (Auto-Generated) -->"
TECHARTICLE_END = "<!-- /Jinn Engineering Observatory: Schema.org TechArticle -->"
ECO_BLOCK_START = "<!-- Jinn Ecosystem & Open Collaboration Block (Auto-Generated) -->"
ECO_BLOCK_END = "<!-- /Jinn Ecosystem & Open Collaboration Block -->"

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

# Taxonomia canônica de Ecossistemas-Alvo conectando engenharia Jinn a problemas reais
ECOSYSTEM_DOMAINS = {
    "compute-simd": {
        "id": "compute-simd",
        "short_label": "Compute & SIMD",
        "title": "Heterogeneous Compute, SIMD & Zero-GPU Acceleration",
        "badge_class": "bg-purple-950/80 border-purple-700/70 text-purple-300 hover:border-purple-400",
        "ecosystems": ["Intel", "AMD", "Arm", "Tenstorrent", "SiFive", "HPC Research Labs"],
        "about": [
            "workload efficiency",
            "heterogeneous compute scheduling",
            "SIMD vectorization AVX2 FMA",
            "zero-GPU 2D compositing",
        ],
        "keywords": [
            "JHCS", "SIMD", "AVX2", "FMA", "Zero-GPU", "Heterogeneous Compute",
            "Aladdin Compositor", "Vector Acceleration",
        ],
        "match_terms": (
            "simd", "avx", "avx2", "fma", "neon", "jhcs", "zero-gpu", "acrílico", "acrilico",
            "glassmorphism", "frosted", "blur", "compositor", "aladdin", "gráfica", "grafica",
        ),
    },
    "zero-idle-efficiency": {
        "id": "zero-idle-efficiency",
        "short_label": "Zero-Idle & SMP",
        "title": "Energy-Proportional Runtime, SMP Accounting & Low-Overhead Scheduling",
        "badge_class": "bg-emerald-950/80 border-emerald-700/70 text-emerald-300 hover:border-emerald-400",
        "ecosystems": ["Arm", "Qualcomm", "LF Edge", "Industrial Automation", "Green Public Infrastructure"],
        "about": [
            "deterministic scheduling",
            "power reduction",
            "energy-proportional computing",
            "SMP per-CPU accounting",
            "lazy TTL cache evaluation",
        ],
        "keywords": [
            "Zero-Idle", "HLT", "SMP", "APIC Timer Gating", "Per-CPU Accounting",
            "Lazy TTL", "Preemptive Scheduler", "Energy-Proportional Runtime",
        ],
        "match_terms": (
            "zero-idle", "hlt", "smp", "per-cpu", "percpu", "apic", "tickless", "escalonador",
            "scheduler", "lazy", "prefetch", "prefet", "pic", "eoi", "0.00%", "0,00%", "ociosa",
        ),
    },
    "microkernel-security": {
        "id": "microkernel-security",
        "short_label": "Microkernel & Segurança",
        "title": "Zero-Trust Isolation, W^X Enforcement & Supply-Chain Integrity",
        "badge_class": "bg-amber-950/80 border-amber-700/70 text-amber-300 hover:border-amber-400",
        "ecosystems": ["Red Hat", "Canonical", "Google", "Siemens", "Sovereign & Civic Infrastructure"],
        "about": [
            "process isolation",
            "W^X memory enforcement",
            "driver fault containment in Ring 3",
            "supply-chain integrity",
            "bare-metal TLS 1.3 security",
        ],
        "keywords": [
            "Microkernel", "Ring 3 Drivers", "W^X", "Zero-Trust Syscalls", "JAHA 1.2",
            "JinnWall SPI", "Supply-Chain Integrity", "TLS 1.3 no_std",
        ],
        "match_terms": (
            "microkernel", "isolamento", "supervisor", "ring 3", "ring3", "w^x", "efault",
            "zero-trust", "segurança", "seguranca", "auditoria", "homolog", "jaha",
            "criptografia", "poly1305", "sha-256", "sha256", "jpkg", "pacote", "fat32",
            "ext2", "vfs", "jinnwall", "auto-recupera",
        ),
    },
    "fastpath-ipc-net": {
        "id": "fastpath-ipc-net",
        "short_label": "IPC & Redes Fast-Path",
        "title": "Zero-Copy Ring Buffers & L2/L3 Fast-Path Networking",
        "badge_class": "bg-cyan-950/80 border-cyan-700/70 text-cyan-300 hover:border-cyan-400",
        "ecosystems": ["Cloudflare", "Akamai", "Huawei", "RISC-V International", "Local Edge Gateways"],
        "about": [
            "zero-copy IPC ring buffers",
            "L2/L3 fast-path networking",
            "workload efficiency",
            "three-tier DNS resolution",
            "sovereign bare-metal TCP/IP",
        ],
        "keywords": [
            "SPSC Ring Buffer", "Zero-Copy IPC", "io_uring", "HotRouteTable",
            "3-Tier DNS", "e1000", "VirtIO-Net", "Jinn Browser", "TLS 1.3",
        ],
        "match_terms": (
            "spsc", "shared-rings", "io_uring", "zero-copy", "ipc", "rede", "redes", "dns",
            "arp", "e1000", "virtio", "hotroute", "fast-path", "tcp", "http", "tls",
            "browser", "navegador", "pcap",
        ),
    },
}

# 6 Bancadas Canônicas do Jinn Engineering Observatory
OBSERVATORY_LABS = {
    "performance-lab": {
        "id": "performance-lab",
        "title": "Performance Lab",
        "subtitle": "Fast-Path IPC, HotRouteTable L2/L3, LRU Cache & Latência",
        "badge_class": "bg-cyan-950/80 border-cyan-700/70 text-cyan-300 hover:border-cyan-400",
        "match_terms": (
            "fast-path", "hotroute", "rotas quentes", "shared-rings", "spsc", "io_uring",
            "prefetch", "latency", "latencia", "throughput", "benchmark", "homologacao-desempenho",
        ),
    },
    "efficiency-lab": {
        "id": "efficiency-lab",
        "title": "Efficiency Lab",
        "subtitle": "Zero-Idle HLT, SMP Per-CPU Accounting, Lazy TTL & 64M RAM",
        "badge_class": "bg-emerald-950/80 border-emerald-700/70 text-emerald-300 hover:border-emerald-400",
        "match_terms": (
            "zero-idle", "hlt", "smp", "percpu", "per-cpu", "apic", "lazy", "ttl",
            "ociosa", "0.00%", "consumo", "energia", "64m", "memory", "jaha",
        ),
    },
    "compute-lab": {
        "id": "compute-lab",
        "title": "Compute Lab",
        "subtitle": "JHCS-RS DAG Broker, AVX2/FMA SIMD, Breakeven & wgpu",
        "badge_class": "bg-purple-950/80 border-purple-700/70 text-purple-300 hover:border-purple-400",
        "match_terms": (
            "jhcs", "simd", "avx2", "fma", "neon", "wgpu", "breakeven", "zero-gpu",
            "glassmorphism", "blur", "compositor", "compute", "gpu",
        ),
    },
    "security-lab": {
        "id": "security-lab",
        "title": "Security Lab",
        "subtitle": "Ring 3 Driver Isolation, W^X PML4, TLS 1.3 no_std & JinnWall",
        "badge_class": "bg-amber-950/80 border-amber-700/70 text-amber-300 hover:border-amber-400",
        "match_terms": (
            "security", "seguranca", "segurança", "ring 3", "ring3", "w^x", "tls",
            "poly1305", "x.509", "x509", "jinnwall", "efault", "isolamento", "criptografia",
        ),
    },
    "architecture-rfcs": {
        "id": "architecture-rfcs",
        "title": "Architecture & RFCs",
        "subtitle": "Especificações de Microkernel, VFS FAT32/Ext2, .jpkg & Profiles",
        "badge_class": "bg-indigo-950/80 border-indigo-700/70 text-indigo-300 hover:border-indigo-400",
        "match_terms": (
            "arquitetura", "architecture", "rfc", "vfs", "fat32", "ext2", "jpkg",
            "persistencia", "aladdin", "navegador", "browser", "dns", "simbad", "profile",
        ),
    },
    "post-mortems": {
        "id": "post-mortems",
        "title": "Post-Mortems & Failure Logs",
        "subtitle": "Análises de Causa Raiz, PCAP Traces & Hardware/IRQ Incidents",
        "badge_class": "bg-rose-950/80 border-rose-700/70 text-rose-300 hover:border-rose-400",
        "match_terms": (
            "post-mortem", "failure", "falha", "incidente", "bug", "pic", "eoi",
            "irq", "carry", "fallback", "telemetria", "pcap", "resiliencia",
        ),
    },
}


def classify_labs(tag: str, all_tags: list[str], slug: str, title: str, body_text: str = "") -> list[str]:
    """Classifica um artigo em 1..2 das 6 bancadas do Jinn Engineering Observatory."""
    primary_haystack = " ".join([tag, *all_tags, slug, title]).lower()
    body_sample = body_text[:2500].lower()

    scored: list[tuple[int, int, str]] = []
    for idx, (lab_id, cfg) in enumerate(OBSERVATORY_LABS.items()):
        p_hits = sum(1 for kw in cfg["match_terms"] if kw in primary_haystack)
        b_hits = min(3, sum(1 for kw in cfg["match_terms"] if kw in body_sample))
        bonus = 6 if (lab_id == "post-mortems" and ("post-mortem" in slug or "POST-MORTEM" in tag)) else 0
        score = p_hits * 3 + b_hits + bonus
        scored.append((score, -idx, lab_id))

    scored.sort(reverse=True)
    selected = [lab_id for score, _, lab_id in scored if score >= 3][:2]
    if not selected and scored:
        selected = [scored[0][2]]
    return selected

CORPORATE_ORGS = {
    "Intel", "AMD", "Arm", "Tenstorrent", "SiFive", "Qualcomm", "LF Edge",
    "Red Hat", "Canonical", "Google", "Siemens", "Cloudflare", "Akamai",
    "Huawei", "RISC-V International",
}


def classify_ecosystems(tag: str, all_tags: list[str], slug: str, title: str, body_text: str = "") -> list[str]:
    """Mapeia os 1..3 domínios técnicos mais pertinentes ao artigo/commit, evitando marcação em massa."""
    primary_haystack = " ".join([tag, *all_tags, slug, title]).lower()
    body_sample = body_text[:2500].lower()

    scored: list[tuple[int, int, str]] = []
    for idx, (dom_id, cfg) in enumerate(ECOSYSTEM_DOMAINS.items()):
        p_hits = sum(1 for kw in cfg["match_terms"] if kw in primary_haystack)
        b_hits = min(3, sum(1 for kw in cfg["match_terms"] if kw in body_sample))
        score = p_hits * 3 + b_hits
        scored.append((score, -idx, dom_id))

    scored.sort(reverse=True)
    selected = [dom_id for score, _, dom_id in scored if score >= 3][:3]
    if not selected and scored:
        selected = [scored[0][2]]
    return selected


def classify_buckets(tag: str, all_tags: list[str], slug: str, title: str) -> list[str]:
    """Classifica um artigo nos filtros legados e nos 4 domínios de ecossistema."""
    eco_ids = classify_ecosystems(tag, all_tags, slug, title)
    return eco_ids


def strip_auto_blocks(content: str) -> str:
    """Remove blocos autogerados antes de re-extrair métricas ou re-injetar marcação idempotente."""
    content = re.sub(
        rf"\n?\s*{re.escape(TECHARTICLE_START)}.*?{re.escape(TECHARTICLE_END)}\s*",
        "\n",
        content,
        flags=re.DOTALL,
    )
    content = re.sub(
        rf"\n?\s*{re.escape(ECO_BLOCK_START)}.*?{re.escape(ECO_BLOCK_END)}\s*",
        "\n",
        content,
        flags=re.DOTALL,
    )
    return content


def extract_measurable_gain(description: str, plain_text: str) -> str:
    """Extrai métricas técnicas verificáveis presentes no texto/descrição para feeds e release notes."""
    text = f"{description} {plain_text[:2500]}"
    candidates = re.findall(
        r"(?:\d+(?:[.,]\d+)?\s*(?:%|bytes|B\b|KiB|MB|GB|ms|µs|us|ns|Hz|FPS|pkts|pacotes|ciclos|vCPUs?|cores|HLT|discrepancy)|\b0[.,]00%\b|\b0 bytes\b|\b0 pacotes\b)",
        text,
        flags=re.IGNORECASE,
    )
    unique = []
    seen = set()
    for c in candidates:
        norm = " ".join(c.split())
        if norm.lower() not in seen:
            seen.add(norm.lower())
            unique.append(norm)
        if len(unique) >= 4:
            break
    if unique:
        return "Indicadores citados no relatório: " + ", ".join(unique)
    return "Evidência empírica reproduzível via QEMU/PCAP e código-fonte auditável"


def format_date_pt(date_iso: str) -> str:
    """Converte YYYY-MM-DD para leitura curta em português (ex: 24 Nov 2026)."""
    parts = date_iso.split("-")
    if len(parts) == 3:
        y, m, d = parts
        return f"{int(d)} {MONTHS_PT.get(m, m)} {y}"
    return date_iso


def build_techarticle_jsonld(meta: dict) -> str:
    """Constrói bloco JSON-LD Schema.org TechArticle válido e determinístico."""
    about_terms = []
    seen_about = set()
    mentions_list = []
    seen_mentions = set()
    keywords = ["Jinn OS", "Microkernel", "Rust no_std", "x86_64", *meta["all_tags"]]
    seen_kw = set(k.lower() for k in keywords)

    for eco_id in meta["ecosystems"]:
        cfg = ECOSYSTEM_DOMAINS[eco_id]
        for topic in cfg["about"]:
            if topic.lower() not in seen_about:
                seen_about.add(topic.lower())
                about_terms.append({"@type": "Thing", "name": topic})
        for org in cfg["ecosystems"]:
            if org.lower() not in seen_mentions:
                seen_mentions.add(org.lower())
                schema_type = "Organization" if org in CORPORATE_ORGS else "Thing"
                mentions_list.append({"@type": schema_type, "name": org})
        for kw in cfg["keywords"]:
            if kw.lower() not in seen_kw:
                seen_kw.add(kw.lower())
                keywords.append(kw)

    lab_titles = [OBSERVATORY_LABS[l]["title"] for l in meta.get("labs", []) if l in OBSERVATORY_LABS]

    img_url = meta.get("image", DEFAULT_IMAGE_URL)
    payload = {
        "@context": "https://schema.org",
        "@type": ["TechArticle", "NewsArticle"],
        "headline": meta["title"],
        "description": meta["description"],
        "image": [img_url],
        "datePublished": meta["pub_iso_full"],
        "dateModified": meta["pub_iso_full"],
        "inLanguage": "pt-BR",
        "articleSection": lab_titles or ["Architecture & RFCs"],
        "author": {
            "@type": "Person",
            "name": AUTHOR_NAME,
            "url": "https://github.com/GeovaneEstanislau",
        },
        "publisher": {
            "@type": "Organization",
            "name": PUBLISHER_NAME,
            "url": f"{SITE_URL}/",
            "logo": {
                "@type": "ImageObject",
                "url": DEFAULT_IMAGE_URL,
            },
        },
        "mainEntityOfPage": {
            "@type": "WebPage",
            "@id": meta["url"],
        },
        "about": about_terms,
        "mentions": mentions_list,
        "keywords": ", ".join(keywords),
    }

    raw_json = json.dumps(payload, ensure_ascii=False, indent=2).replace("</", "<\\/")
    return (
        f"  {TECHARTICLE_START}\n"
        f'  <script type="application/ld+json" id="jinn-techarticle-jsonld">\n'
        f"{raw_json}\n"
        f"  </script>\n"
        f"  {TECHARTICLE_END}"
    )


def build_collaboration_section_html(meta: dict) -> str:
    """Gera bloco visual 'Applicable Ecosystems & Open Collaboration' para o rodapé técnico do artigo."""
    lab_badges = []
    for lab_id in meta.get("labs", []):
        lcfg = OBSERVATORY_LABS.get(lab_id)
        if lcfg:
            lab_badges.append(
                f'<a href="/artigos/?lab={lab_id}" '
                f'class="inline-flex items-center gap-1.5 px-3 py-1 rounded-full border text-xs font-mono transition-colors {lcfg["badge_class"]}">'
                f'<span>{html_lib.escape(lcfg["title"])}</span></a>'
            )

    domain_badges = []
    ref_ecosystems = []
    seen_eco = set()

    for eco_id in meta["ecosystems"]:
        cfg = ECOSYSTEM_DOMAINS[eco_id]
        domain_badges.append(
            f'<a href="/artigos/?eco={eco_id}" '
            f'class="inline-flex items-center gap-1.5 px-3 py-1 rounded-full border text-xs font-mono transition-colors {cfg["badge_class"]}">'
            f'<span>{html_lib.escape(cfg["short_label"])}</span>'
            f'<span class="opacity-70">&bull; {html_lib.escape(cfg["title"])}</span>'
            f"</a>"
        )
        for eco_name in cfg["ecosystems"]:
            if eco_name not in seen_eco:
                seen_eco.add(eco_name)
                ref_ecosystems.append(
                    f'<span class="px-2.5 py-0.5 rounded bg-slate-900 border border-slate-700/80 text-[11px] font-mono text-slate-300">'
                    f'{html_lib.escape(eco_name)}</span>'
                )

    labs_html = "\n          ".join(lab_badges)
    badges_html = "\n          ".join(domain_badges)
    ecos_html = "\n          ".join(ref_ecosystems)

    return f"""      {ECO_BLOCK_START}
      <section aria-labelledby="ecosystems-collaboration-heading" class="not-prose my-10 p-6 rounded-xl bg-slate-900/75 border border-slate-800 space-y-5">
        <div class="space-y-2">
          <p class="text-[11px] font-mono uppercase tracking-wider text-cyan-400">Jinn Engineering Observatory • Open Collaboration</p>
          <h3 id="ecosystems-collaboration-heading" class="text-lg sm:text-xl font-bold text-white">
            Applicable Ecosystems &amp; Open Collaboration
          </h3>
          <p class="text-xs sm:text-sm text-slate-300 leading-relaxed">
            Conexões arquiteturais deste estudo com domínios industriais, pesquisa científica e infraestrutura auditável:
          </p>
        </div>

        <div class="flex flex-wrap gap-2">
          {labs_html}
          {badges_html}
        </div>

        <div class="pt-1">
          <div class="text-[11px] font-mono text-slate-400 mb-2">Ecossistemas de referência pertinentes (Nível B contextual):</div>
          <div class="flex flex-wrap gap-1.5">
            {ecos_html}
          </div>
        </div>

        <div class="border-t border-slate-800 pt-4 space-y-3">
          <h4 class="text-sm font-bold text-emerald-300 font-mono">
            Chamada Aberta: Indústria Local, Academia &amp; Setor Público Eticamente Responsável
          </h4>
          <p class="text-xs sm:text-sm text-slate-300 leading-relaxed">
            Convidamos <strong>empresas regionais de automação industrial e integradores</strong>, <strong>universidades e grupos de pesquisa em sistemas operacionais</strong> e <strong>projetos públicos/governamentais eticamente responsáveis</strong> (com transparência e código auditável) a reproduzirem estes experimentos em bancada física, propor <em>workloads</em> comparativos ou avaliar pilotos com o <strong>Jinn OS</strong> e o <strong>JHCS</strong>.
          </p>
          <div class="flex flex-wrap gap-2.5 text-xs font-mono pt-1">
            <a href="{REPO_URL}" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-cyan-400 text-slate-200 hover:text-cyan-300 transition-colors">
              GitHub: Jinn_Project
            </a>
            <a href="{JHCS_REPO_URL}" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-purple-400 text-slate-200 hover:text-purple-300 transition-colors">
              GitHub: jhcs-rs
            </a>
            <a href="{REPO_URL}/blob/main/docs/ENGINEERING_STATUS_AND_LIMITATIONS.md" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-amber-400 text-slate-200 hover:text-amber-300 transition-colors">
              Fronteira Honesta &amp; Evidências
            </a>
            <a href="{REPO_URL}/blob/main/docs/strategy/OPPORTUNITY_RADAR_50.md" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-emerald-400 text-slate-200 hover:text-emerald-300 transition-colors">
              Radar &amp; 6ª Trilha Aberta
            </a>
            <a href="/#win-contact" class="px-3 py-1.5 rounded-lg bg-cyan-950/90 border border-cyan-600/70 hover:bg-cyan-900 text-cyan-200 transition-colors">
              Contato Técnico &amp; Pilotos
            </a>
          </div>
        </div>
      </section>
      {ECO_BLOCK_END}"""


def enrich_article_html(html_path: Path, clean_content: str, meta: dict) -> None:
    """Injeta JSON-LD TechArticle no <head> e bloco de Ecossistemas & Colaboração no corpo."""
    jsonld_block = build_techarticle_jsonld(meta)
    collab_block = build_collaboration_section_html(meta)

    updated = clean_content

    if "</head>" in updated:
        updated = re.sub(r"</head>", f"{jsonld_block}\n</head>", updated, count=1, flags=re.IGNORECASE)

    rrm_marker = "<!-- Google News Reader Revenue Manager (CTA Inline Final) -->"
    if rrm_marker in updated:
        updated = updated.replace(rrm_marker, f"{collab_block}\n\n      {rrm_marker}", 1)
    elif '<div class="my-8 not-prose">' in updated and "rrm-inline-cta" in updated:
        updated = re.sub(
            r'(<div\s+class=["\'][^"\']*not-prose[^"\']*["\']>\s*<div\s+rrm-inline-cta)',
            f"{collab_block}\n\n      \\1",
            updated,
            count=1,
        )
    elif "</article>" in updated:
        updated = updated.replace("</article>", f"{collab_block}\n    </article>", 1)

    if updated != html_path.read_text(encoding="utf-8", errors="ignore"):
        html_path.write_text(updated, encoding="utf-8")


def extract_article_meta(html_path: Path, enrich: bool = True):
    """Extrai metadados estruturados de um artigo HTML e opcionalmente enriquece seu HTML."""
    try:
        raw_content = html_path.read_text(encoding="utf-8", errors="ignore")
    except Exception as e:
        print(f"[WARN] Erro ao ler {html_path}: {e}")
        return None

    content = strip_auto_blocks(raw_content)

    # Título
    title_match = re.search(r"<title>(.*?)</title>", content, re.IGNORECASE | re.DOTALL)
    title = html_lib.unescape(title_match.group(1).strip()) if title_match else html_path.stem.replace("-", " ").title()
    title = re.sub(r"\s*[—–-]\s*Jinn\s*OS.*$", "", title).strip()

    # Descrição
    desc_match = re.search(r'<meta\s+name=["\']description["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE | re.DOTALL)
    description = html_lib.unescape(" ".join(desc_match.group(1).split())) if desc_match else title

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

    # Tempo estimado de leitura (~180 palavras/min) e texto limpo
    article_body = re.search(r"<article[^>]*>(.*?)</article>", content, re.IGNORECASE | re.DOTALL)
    body_src = article_body.group(1) if article_body else content
    plain_text = re.sub(r"<script[^>]*>.*?</script>", " ", body_src, flags=re.IGNORECASE | re.DOTALL)
    plain_text = re.sub(r"<style[^>]*>.*?</style>", " ", plain_text, flags=re.IGNORECASE | re.DOTALL)
    plain_text = html_lib.unescape(re.sub(r"<[^>]+>", " ", plain_text))
    words = len(re.findall(r"\w+", plain_text))
    read_minutes = max(2, round(words / 180))

    # Data de modificação / publicação
    pub_date_rfc2822 = "Tue, 24 Nov 2026 18:00:00 -0300"
    pub_date_iso = "2026-11-24"
    pub_iso_full = "2026-11-24T18:00:00-03:00"
    sort_ts = "2026-11-24T18:00:00"

    pub_match = re.search(r'<meta\s+name=["\']article:published_time["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE)
    if pub_match:
        raw_time = pub_match.group(1).strip()
        pub_iso_full = raw_time
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
                pub_iso_full = dt.strftime("%Y-%m-%dT%H:%M:%SZ")
                sort_ts = dt.strftime("%Y-%m-%dT%H:%M:%S")
        except Exception:
            stat = html_path.stat()
            mtime = datetime.fromtimestamp(stat.st_mtime, timezone.utc)
            pub_date_rfc2822 = mtime.strftime("%a, %d %b %Y %H:%M:%S +0000")
            pub_date_iso = mtime.strftime("%Y-%m-%d")
            pub_iso_full = mtime.strftime("%Y-%m-%dT%H:%M:%SZ")
            sort_ts = mtime.strftime("%Y-%m-%dT%H:%M:%S")

    img_match = re.search(r'<meta\s+property=["\']og:image["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE)
    image_url = img_match.group(1).strip() if img_match else DEFAULT_IMAGE_URL
    image_length = 70707
    image_mime = "image/png" if image_url.lower().endswith(".png") else "image/jpeg"
    if image_url.startswith(f"{SITE_URL}/assets/"):
        asset_path = WEB_DIR / "assets" / image_url.split("/assets/", 1)[1]
        if asset_path.exists():
            image_length = asset_path.stat().st_size

    ecosystems = classify_ecosystems(tag, all_tags, html_path.stem, title, plain_text)
    labs = classify_labs(tag, all_tags, html_path.stem, title, plain_text)
    measurable_gain = extract_measurable_gain(description, plain_text)
    guid = f"jinn-os-artigo-{html_path.stem}"

    meta = {
        "slug": html_path.stem,
        "filename": html_path.name,
        "title": title,
        "description": description,
        "tag": tag,
        "all_tags": all_tags,
        "ecosystems": ecosystems,
        "labs": labs,
        "buckets": ecosystems,
        "measurable_gain": measurable_gain,
        "read_minutes": read_minutes,
        "date_human": format_date_pt(pub_date_iso),
        "sort_ts": sort_ts,
        "pub_iso_full": pub_iso_full,
        "url": url,
        "guid": guid,
        "date_rfc2822": pub_date_rfc2822,
        "date_iso": pub_date_iso,
        "image": image_url,
        "image_length": str(image_length),
        "image_mime": image_mime,
    }

    if enrich:
        enrich_article_html(html_path, content, meta)

    return meta


def get_all_articles(enrich: bool = True):
    """Descobre, enriquece e ordena todos os artigos técnicos em web/artigos/."""
    articles = []
    if not ARTIGOS_DIR.exists():
        return articles

    for f in sorted(ARTIGOS_DIR.glob("*.html")):
        if f.name == "index.html":
            continue
        meta = extract_article_meta(f, enrich=enrich)
        if meta:
            articles.append(meta)

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
    """Gera web/artigos/index.html com filtros por ecossistema, badges clicáveis e bloco de colaboração aberta."""
    print(f"[+] Gerando Índice Visual em {INDEX_PATH} ({len(articles)} artigos)...")

    cards_html = []
    for a in articles:
        colors = CATEGORY_COLORS.get(a["tag"], DEFAULT_COLOR)
        tag_color, tag_bg, tag_border, hover_title = colors
        eco_terms = " ".join(
            [ECOSYSTEM_DOMAINS[e]["title"] + " " + " ".join(ECOSYSTEM_DOMAINS[e]["ecosystems"]) for e in a["ecosystems"]]
        )
        lab_terms = " ".join(
            [OBSERVATORY_LABS[l]["title"] + " " + OBSERVATORY_LABS[l]["subtitle"] for l in a["labs"]]
        )
        tags_text = " ".join(a["all_tags"])
        search_blob = html_lib.escape(
            f"{a['title']} {a['description']} {tags_text} {lab_terms} {eco_terms}".lower(),
            quote=True,
        )
        categories_attr = html_lib.escape(" ".join(a["ecosystems"]), quote=True)
        labs_attr = html_lib.escape(" ".join(a["labs"]), quote=True)
        title_esc = html_lib.escape(a["title"])
        desc_esc = html_lib.escape(a["description"])
        tag_esc = html_lib.escape(a["tag"])
        gain_esc = html_lib.escape(a["measurable_gain"])

        lab_buttons = []
        for lab_id in a["labs"]:
            lcfg = OBSERVATORY_LABS[lab_id]
            lab_buttons.append(
                f'<button type="button" data-lab-badge="{lab_id}" '
                f'title="Filtrar por {html_lib.escape(lcfg["title"], quote=True)}" '
                f'class="px-2 py-0.5 rounded-full border text-[10px] font-mono transition-colors {lcfg["badge_class"]}">'
                f'{html_lib.escape(lcfg["title"])}</button>'
            )
        eco_buttons = []
        for eco_id in a["ecosystems"]:
            cfg = ECOSYSTEM_DOMAINS[eco_id]
            eco_buttons.append(
                f'<button type="button" data-eco-badge="{eco_id}" '
                f'title="Filtrar por {html_lib.escape(cfg["title"], quote=True)}" '
                f'class="px-2 py-0.5 rounded-full border text-[10px] font-mono transition-colors {cfg["badge_class"]}">'
                f'{html_lib.escape(cfg["short_label"])}</button>'
            )
        eco_badges_html = "".join(lab_buttons + eco_buttons[:1])

        ref_ecos = []
        seen_ref = set()
        for eco_id in a["ecosystems"]:
            for name in ECOSYSTEM_DOMAINS[eco_id]["ecosystems"]:
                if name not in seen_ref:
                    seen_ref.add(name)
                    ref_ecos.append(name)
        ref_line = html_lib.escape(", ".join(ref_ecos[:6]))

        card = f"""      <!-- Card: {a['slug']} -->
      <article data-article-card data-labs="{labs_attr}" data-categories="{categories_attr}" data-search="{search_blob}" class="group flex flex-col justify-between p-6 rounded-xl bg-slate-900/65 border border-slate-800 hover:border-cyan-500/60 hover:bg-slate-900/95 transition-all duration-200 shadow-sm hover:shadow-cyan-950/30">
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
            <a href="/artigos/{a['filename']}" class="hover:underline decoration-cyan-500/60 underline-offset-4">{title_esc}</a>
          </h2>
          <p class="text-slate-400 text-sm mt-3 line-clamp-3 leading-relaxed">
            {desc_esc}
          </p>
          <p class="text-[11px] font-mono text-emerald-300/90 mt-2.5 truncate" title="{gain_esc}">
            {gain_esc}
          </p>
          <p class="text-[11px] font-mono text-slate-500 mt-1 truncate" title="Ecossistemas relacionados: {ref_line}">
            Ecossistemas: <span class="text-slate-300">{ref_line}</span>
          </p>
        </div>
        <div class="mt-4 pt-3 border-t border-slate-800/70 flex flex-wrap items-center justify-between gap-2">
          <div class="flex flex-wrap items-center gap-1.5">{eco_badges_html}</div>
          <a href="/artigos/{a['filename']}" class="flex items-center gap-1 text-xs font-semibold text-cyan-400 hover:text-cyan-300 ml-auto shrink-0">
            <span>Ler artigo</span>
            <i data-lucide="arrow-right" class="w-3.5 h-3.5 group-hover:translate-x-1 transition-transform"></i>
          </a>
        </div>
      </article>"""
        cards_html.append(card)

    cards_block = "\n\n".join(cards_html)
    total_count = len(articles)

    html_content = f"""<!DOCTYPE html>
<html lang="pt-BR" class="scroll-smooth">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Artigos Técnicos &amp; Devlogs — Jinn Engineering Observatory</title>
  <meta name="description" content="Arquivo central de artigos técnicos, TechArticle Schema.org, post-mortems, arquitetura de sistemas e auditorias de engenharia do microkernel Jinn OS, JHCS e Aladdin.">
  <link rel="canonical" href="https://jinn-os.dev.br/artigos/">
  <link rel="alternate" type="application/rss+xml" title="Jinn OS — Devlog &amp; Releases" href="https://jinn-os.dev.br/feed.xml">
  <link rel="alternate" type="application/rss+xml" title="Jinn OS — RSS Feed" href="https://jinn-os.dev.br/rss.xml">

  <!-- Open Graph -->
  <meta property="og:type" content="website">
  <meta property="og:url" content="https://jinn-os.dev.br/artigos/">
  <meta property="og:title" content="Artigos Técnicos &amp; Devlogs — Jinn Engineering Observatory">
  <meta property="og:description" content="Engenharia de sistemas operacionais em Rust bare-metal: post-mortems, microkernels, eficiência Zero-Idle, criptografia e computação heterogênea.">
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

  <!-- Cabeçalho Fixo Compacto (Sticky Header) com Busca e Filtros por Ecossistema -->
  <header class="border-b border-slate-800/80 bg-slate-950/90 backdrop-blur-md sticky top-0 z-50">
    <div class="max-w-6xl mx-auto px-4 sm:px-6 h-14 flex items-center justify-between gap-3">
      <a href="/" class="flex items-center gap-2 hover:text-cyan-400 transition-colors shrink-0">
        <i data-lucide="arrow-left" class="w-4 h-4 text-cyan-400"></i>
        <span class="font-bold tracking-wide">Jinn Observatory</span>
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
        <a href="{REPO_URL}" target="_blank" rel="noopener noreferrer" class="flex items-center gap-1 text-slate-400 hover:text-white transition-colors shrink-0">
          <i data-lucide="github" class="w-3.5 h-3.5"></i>
          <span>GitHub</span>
        </a>
      </div>
    </div>

    <!-- Barra de Busca Instantânea + 6 Bancadas do Observatory -->
    <div class="border-t border-slate-800/60 bg-slate-900/75">
      <div class="max-w-6xl mx-auto px-4 sm:px-6 py-2.5 flex flex-col gap-2">
        <div class="flex flex-col lg:flex-row lg:items-center gap-2.5 justify-between">
          <div class="relative flex-1 max-w-md">
            <i data-lucide="search" class="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none"></i>
            <input id="article-search" type="search" placeholder="Buscar por título, bancada, ecossistema, syscall ou métrica..."
              class="w-full pl-9 pr-3 py-1.5 text-xs sm:text-sm rounded-lg bg-slate-950/90 border border-slate-700/80 text-slate-100 placeholder-slate-500 focus:outline-none focus:border-cyan-400 transition-colors" />
          </div>
          <nav aria-label="6 Bancadas do Observatory" class="flex flex-wrap items-center gap-1.5 text-xs font-mono">
            <button type="button" data-lab-filter="all" class="lab-pill px-3 py-1 rounded-full border border-cyan-500/70 bg-cyan-950/90 text-cyan-300 font-semibold transition-colors">Todas as Bancadas</button>
            <button type="button" data-lab-filter="performance-lab" class="lab-pill px-3 py-1 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-cyan-400 transition-colors">Performance Lab</button>
            <button type="button" data-lab-filter="efficiency-lab" class="lab-pill px-3 py-1 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-emerald-400 transition-colors">Efficiency Lab</button>
            <button type="button" data-lab-filter="compute-lab" class="lab-pill px-3 py-1 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-purple-400 transition-colors">Compute Lab</button>
            <button type="button" data-lab-filter="security-lab" class="lab-pill px-3 py-1 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-amber-400 transition-colors">Security Lab</button>
            <button type="button" data-lab-filter="architecture-rfcs" class="lab-pill px-3 py-1 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-indigo-400 transition-colors">Architecture &amp; RFCs</button>
            <button type="button" data-lab-filter="post-mortems" class="lab-pill px-3 py-1 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-rose-400 transition-colors">Post-Mortems &amp; Failure Logs</button>
          </nav>
        </div>
        <nav aria-label="Filtros por ecossistema" class="flex flex-wrap items-center gap-1.5 text-[11px] font-mono">
          <span class="text-slate-500 mr-1">Ecossistemas:</span>
          <button type="button" data-filter="all" class="filter-pill px-2.5 py-0.5 rounded-full border border-cyan-500/70 bg-cyan-950/90 text-cyan-300 font-semibold transition-colors">Todos</button>
          <button type="button" data-filter="compute-simd" class="filter-pill px-2.5 py-0.5 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-purple-400 transition-colors">Compute &amp; SIMD</button>
          <button type="button" data-filter="zero-idle-efficiency" class="filter-pill px-2.5 py-0.5 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-emerald-400 transition-colors">Zero-Idle &amp; SMP</button>
          <button type="button" data-filter="microkernel-security" class="filter-pill px-2.5 py-0.5 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-amber-400 transition-colors">Microkernel &amp; Segurança</button>
          <button type="button" data-filter="fastpath-ipc-net" class="filter-pill px-2.5 py-0.5 rounded-full border border-slate-700 bg-slate-900 text-slate-300 hover:border-cyan-400 transition-colors">IPC &amp; Redes Fast-Path</button>
        </nav>
      </div>
    </div>
  </header>

  <!-- Conteúdo Principal com Rolagem Fluida Nativa -->
  <main class="max-w-6xl mx-auto px-4 sm:px-6 py-8 sm:py-10 flex-1 w-full">
    <div class="mb-8 flex flex-col sm:flex-row sm:items-end justify-between gap-4">
      <div>
        <h1 class="text-2xl sm:text-4xl font-extrabold text-white tracking-tight">
          Acervo Técnico de Engenharia &amp; Observatory
        </h1>
        <p class="text-slate-400 mt-2 max-w-2xl leading-relaxed text-sm">
          Relatórios reproduzíveis, <em>post-mortems</em> com PCAP, benchmarks bare-metal e arquitetura do <strong>Jinn Microkernel</strong>, <strong>JHCS</strong> e compositor <strong>Aladdin</strong>.
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

    <!-- Canal Aberto: Indústria Local, Academia & Setor Público Eticamente Responsável -->
    <section aria-labelledby="central-collaboration-heading" class="mt-14 p-6 sm:p-8 rounded-2xl bg-gradient-to-br from-slate-900/90 via-slate-900/70 to-cyan-950/30 border border-slate-800 space-y-5">
      <div class="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div class="space-y-2 max-w-3xl">
          <p class="text-xs font-mono uppercase tracking-wider text-cyan-400">Jinn Open Engineering Outreach • Research &amp; Collaboration</p>
          <h2 id="central-collaboration-heading" class="text-xl sm:text-2xl font-bold text-white">
            Applicable Ecosystems &amp; Open Collaboration
          </h2>
          <p class="text-sm text-slate-300 leading-relaxed">
            Cada publicação deste portal conecta engenharia verificável a desafios concretos de sistemas embarcados, edge computing, densidade de memória e aceleração heterogênea. Mantemos um canal aberto para diálogo técnico, validação em silício real e projetos-piloto com:
          </p>
        </div>
        <a href="/#win-contact" class="shrink-0 inline-flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl bg-cyan-500/15 border border-cyan-400/60 hover:bg-cyan-500/25 text-cyan-200 font-mono text-xs font-semibold transition-colors">
          <i data-lucide="mail" class="w-4 h-4"></i>
          <span>Propor Piloto ou Benchmark</span>
        </a>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-3 gap-4 text-xs sm:text-sm">
        <div class="p-4 rounded-xl bg-slate-950/75 border border-slate-800/90 space-y-1.5">
          <div class="font-mono text-emerald-300 font-semibold">Empresas Locais &amp; Automação</div>
          <p class="text-slate-400 leading-relaxed">Integradores, fabricantes de equipamentos, IHM/SCADA e gateways restritos buscando isolamento de falhas em Ring 3, baixo footprint de RAM e operação sem dependência de GPU.</p>
        </div>
        <div class="p-4 rounded-xl bg-slate-950/75 border border-slate-800/90 space-y-1.5">
          <div class="font-mono text-purple-300 font-semibold">Universidades &amp; Pesquisadores</div>
          <p class="text-slate-400 leading-relaxed">Grupos de sistemas operacionais, arquitetura, segurança e eficiência energética interessados em reproduzir testes em hardware físico real (potência elétrica vs QEMU <code>HLT</code>) ou co-orientar P&amp;D.</p>
        </div>
        <div class="p-4 rounded-xl bg-slate-950/75 border border-slate-800/90 space-y-1.5">
          <div class="font-mono text-amber-300 font-semibold">Setor Público Eticamente Responsável</div>
          <p class="text-slate-400 leading-relaxed">Instituições públicas, laboratórios didáticos e infraestrutura cívica que demandam software auditável, segurança defensiva e reaproveitamento responsável de hardware com transparência total.</p>
        </div>
      </div>

      <div class="flex flex-wrap items-center gap-3 text-xs font-mono pt-1">
        <a href="{REPO_URL}" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-cyan-400 text-slate-200 transition-colors">Repositório Jinn_Project</a>
        <a href="{JHCS_REPO_URL}" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-purple-400 text-slate-200 transition-colors">Workspace JHCS (jhcs-rs)</a>
        <a href="{REPO_URL}/tree/main/docs/outreach" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-emerald-400 text-slate-200 transition-colors">Documento-Mãe &amp; Radar de Oportunidades</a>
        <a href="{REPO_URL}/issues/new/choose" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-amber-400 text-slate-200 transition-colors">Abrir Issue Técnica</a>
      </div>
    </section>
  </main>

  <!-- Rodapé -->
  <footer class="border-t border-slate-800 mt-12 py-8 text-xs text-slate-400 font-mono">
    <div class="max-w-6xl mx-auto px-4 sm:px-6 flex flex-col sm:flex-row items-center justify-between gap-4">
      <div class="flex items-center gap-3">
        <span>{PUBLISHER_NAME} • {AUTHOR_NAME} • 2026</span>
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
      const labPills = Array.from(document.querySelectorAll('.lab-pill'));
      const pills = Array.from(document.querySelectorAll('.filter-pill'));
      const labBadges = Array.from(document.querySelectorAll('[data-lab-badge]'));
      const ecoBadges = Array.from(document.querySelectorAll('[data-eco-badge]'));
      const cards = Array.from(document.querySelectorAll('[data-article-card]'));
      const visibleCount = document.getElementById('visible-count');
      const articlesCounter = document.getElementById('articles-counter');
      const emptyState = document.getElementById('empty-state');
      const resetBtn = document.getElementById('reset-filters-btn');

      let activeLab = 'all';
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
          const labs = (card.getAttribute('data-labs') || '').split(' ');
          const cats = (card.getAttribute('data-categories') || '').split(' ');
          const text = card.getAttribute('data-search') || '';
          const matchLab = activeLab === 'all' || labs.includes(activeLab);
          const matchCat = activeFilter === 'all' || cats.includes(activeFilter);
          const matchText = !q || text.includes(q);
          if (matchLab && matchCat && matchText) {{
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

      function syncUrlParams() {{
        if (window.history && window.history.replaceState) {{
          const url = new URL(window.location.href);
          if (activeLab === 'all') url.searchParams.delete('lab'); else url.searchParams.set('lab', activeLab);
          if (activeFilter === 'all') url.searchParams.delete('eco'); else url.searchParams.set('eco', activeFilter);
          window.history.replaceState({{}}, '', url.toString());
        }}
      }}

      function selectLabFilter(labId, updateUrl) {{
        activeLab = labId || 'all';
        labPills.forEach(function (p) {{
          const selected = p.getAttribute('data-lab-filter') === activeLab;
          p.classList.toggle('border-cyan-500/70', selected);
          p.classList.toggle('bg-cyan-950/90', selected);
          p.classList.toggle('text-cyan-300', selected);
          p.classList.toggle('font-semibold', selected);
          p.classList.toggle('border-slate-700', !selected);
          p.classList.toggle('bg-slate-900', !selected);
          p.classList.toggle('text-slate-300', !selected);
        }});
        if (updateUrl) syncUrlParams();
        applyFilters();
      }}

      function selectEcoFilter(filterId, updateUrl) {{
        activeFilter = filterId || 'all';
        pills.forEach(function (p) {{
          const selected = p.getAttribute('data-filter') === activeFilter;
          p.classList.toggle('border-cyan-500/70', selected);
          p.classList.toggle('bg-cyan-950/90', selected);
          p.classList.toggle('text-cyan-300', selected);
          p.classList.toggle('font-semibold', selected);
          p.classList.toggle('border-slate-700', !selected);
          p.classList.toggle('bg-slate-900', !selected);
          p.classList.toggle('text-slate-300', !selected);
        }});
        if (updateUrl) syncUrlParams();
        applyFilters();
      }}

      labPills.forEach(function (btn) {{
        btn.addEventListener('click', function () {{
          selectLabFilter(btn.getAttribute('data-lab-filter') || 'all', true);
        }});
      }});

      pills.forEach(function (btn) {{
        btn.addEventListener('click', function () {{
          selectEcoFilter(btn.getAttribute('data-filter') || 'all', true);
        }});
      }});

      labBadges.forEach(function (badge) {{
        badge.addEventListener('click', function (ev) {{
          ev.preventDefault();
          ev.stopPropagation();
          selectLabFilter(badge.getAttribute('data-lab-badge') || 'all', true);
          window.scrollTo({{ top: 0, behavior: 'smooth' }});
        }});
      }});

      ecoBadges.forEach(function (badge) {{
        badge.addEventListener('click', function (ev) {{
          ev.preventDefault();
          ev.stopPropagation();
          selectEcoFilter(badge.getAttribute('data-eco-badge') || 'all', true);
          window.scrollTo({{ top: 0, behavior: 'smooth' }});
        }});
      }});

      if (searchInput) searchInput.addEventListener('input', applyFilters);
      if (resetBtn) {{
        resetBtn.addEventListener('click', function () {{
          if (searchInput) searchInput.value = '';
          selectLabFilter('all', true);
          selectEcoFilter('all', true);
        }});
      }}

      const params = new URLSearchParams(window.location.search);
      const initialLab = params.get('lab');
      const initialEco = params.get('eco');
      if (initialLab && labPills.some(function (p) {{ return p.getAttribute('data-lab-filter') === initialLab; }})) {{
        selectLabFilter(initialLab, false);
      }}
      if (initialEco && pills.some(function (p) {{ return p.getAttribute('data-filter') === initialEco; }})) {{
        selectEcoFilter(initialEco, false);
      }}
    }})();
  </script>
</body>
</html>
"""
    INDEX_PATH.write_text(html_content, encoding="utf-8")
    print(f"  [PASS] {INDEX_PATH} gerado com sucesso!")


def format_ecosystem_summary_text(base_summary: str, gain: str, eco_ids: list[str]) -> str:
    """Formata descrição enriquecida com ganho técnico mensurável e ecossistemas aplicáveis para RSS/Atom."""
    eco_lines = []
    for eco_id in eco_ids:
        cfg = ECOSYSTEM_DOMAINS[eco_id]
        eco_lines.append(f"{cfg['title']} ({', '.join(cfg['ecosystems'])})")
    eco_str = " | ".join(eco_lines) if eco_lines else "Systems Engineering & Open Hardware"
    return (
        f"{base_summary} "
        f"[Ganho técnico: {gain}] "
        f"[Ecossistemas aplicáveis: {eco_str}] "
        f"[Colaboração aberta — indústria local, academia e setor público responsável: {SITE_URL}/#win-contact | {REPO_URL}]"
    )


def build_rss_feeds(articles, commits):
    """Gera feed.xml e rss.xml com ganhos técnicos mensuráveis, categorias de ecossistema e WebSub."""
    print(f"[+] Gerando RSS 2.0 e WebSub Feeds em {FEED_PATH} e {RSS_PATH}...")
    ET.register_namespace('atom', 'http://www.w3.org/2005/Atom')
    ET.register_namespace('media', 'http://search.yahoo.com/mrss/')

    rss = ET.Element("rss", version="2.0")
    channel = ET.SubElement(rss, "channel")
    ET.SubElement(channel, "title").text = f"{PUBLISHER_NAME} — Devlog & Microkernel Releases"
    ET.SubElement(channel, "link").text = SITE_URL
    ET.SubElement(channel, "description").text = (
        "Artigos de engenharia (Schema.org TechArticle), benchmarks bare-metal, computação heterogênea (JHCS), "
        "Zero-Idle em SMP, isolamento Ring 3 e canal de colaboração aberta com indústria local, universidades e setor público."
    )
    ET.SubElement(channel, "language").text = "pt-br"

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
        ET.SubElement(item, "description").text = format_ecosystem_summary_text(
            art["description"], art["measurable_gain"], art["ecosystems"]
        )
        for eco_id in art["ecosystems"]:
            ET.SubElement(item, "category").text = eco_id
        for lab_id in art.get("labs", []):
            ET.SubElement(item, "category").text = lab_id

        enc = ET.SubElement(item, "enclosure")
        enc.set("url", art.get("image", DEFAULT_IMAGE_URL))
        enc.set("length", art.get("image_length", "70707"))
        enc.set("type", art.get("image_mime", "image/jpeg"))

    # 2. Commits Técnicos Recentes
    for c in commits:
        eco_ids = classify_ecosystems("COMMIT", [], "", c["title"], c["body"])
        gain = extract_measurable_gain(c["title"], c["body"])
        item = ET.SubElement(channel, "item")
        ET.SubElement(item, "title").text = c["title"]
        commit_url = f"{REPO_URL}/commit/{c['hash']}"
        ET.SubElement(item, "link").text = commit_url
        guid_el = ET.SubElement(item, "guid")
        guid_el.text = c["hash"]
        guid_el.set("isPermaLink", "false")
        ET.SubElement(item, "pubDate").text = c["date"]
        ET.SubElement(item, "description").text = format_ecosystem_summary_text(
            c["body"], gain, eco_ids
        )
        for eco_id in eco_ids:
            ET.SubElement(item, "category").text = eco_id

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

        news = ET.SubElement(url_art, "{http://www.google.com/schemas/sitemap-news/0.9}news")
        pub = ET.SubElement(news, "{http://www.google.com/schemas/sitemap-news/0.9}publication")
        ET.SubElement(pub, "{http://www.google.com/schemas/sitemap-news/0.9}name").text = PUBLISHER_NAME
        ET.SubElement(pub, "{http://www.google.com/schemas/sitemap-news/0.9}language").text = "pt-BR"
        ET.SubElement(news, "{http://www.google.com/schemas/sitemap-news/0.9}publication_date").text = art["date_iso"]
        ET.SubElement(news, "{http://www.google.com/schemas/sitemap-news/0.9}title").text = art["title"]

    tree = ET.ElementTree(urlset)
    ET.indent(tree, space="  ")
    tree.write(SITEMAP_PATH, encoding="utf-8", xml_declaration=True)
    print(f"  [PASS] {len(articles) + 2} URLs catalogadas com sucesso em sitemap.xml!")


def main():
    print("==================================================================")
    print("  JINN OBSERVATORY — TECHARTICLE JSON-LD, ECOSSISTEMAS & FEEDS")
    print("==================================================================")
    articles = get_all_articles(enrich=True)
    print(f"[+] Artigos identificados e enriquecidos em {ARTIGOS_DIR}: {len(articles)}")
    for a in articles:
        print(f"  - [{a['date_iso']}] [{','.join(a['ecosystems'])}] {a['title']}")

    commits = get_announced_commits(10)
    build_artigos_index(articles)
    build_rss_feeds(articles, commits)
    build_sitemap(articles)
    print("[+] Concluído com sucesso!")


if __name__ == "__main__":
    main()
