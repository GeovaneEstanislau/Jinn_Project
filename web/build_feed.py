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
from datetime import datetime, timedelta, timezone

TZ_BRT = timezone(timedelta(hours=-3))

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
        "ecosystems": [
            "Intel", "AMD", "Arm", "NVIDIA", "Tenstorrent", "SiFive",
            "Academic Systems Research Labs",
        ],
        "about": [
            "SIMD Vectorization",
            "Heterogeneous Compute Scheduling",
            "Zero-GPU 2D Compositing",
            "Workload Efficiency",
        ],
        "keywords": [
            "JHCS", "SIMD Vectorization", "AVX2", "FMA", "Zero-GPU", "Heterogeneous Compute",
            "Aladdin Compositor", "wgpu Breakeven", "Vector Acceleration",
        ],
        "match_terms": (
            "simd", "avx", "avx2", "fma", "neon", "jhcs", "zero-gpu", "acrílico", "acrilico",
            "glassmorphism", "frosted", "blur", "compositor", "aladdin", "gráfica", "grafica",
            "wgpu", "breakeven", "escalar", "scalar",
        ),
    },
    "zero-idle-efficiency": {
        "id": "zero-idle-efficiency",
        "short_label": "Zero-Idle & SMP",
        "title": "Energy-Proportional Runtime, SMP Accounting & Low-Overhead Scheduling",
        "badge_class": "bg-emerald-950/80 border-emerald-700/70 text-emerald-300 hover:border-emerald-400",
        "ecosystems": [
            "Intel", "AMD", "Arm", "Qualcomm", "Siemens", "Bosch", "STMicroelectronics",
            "LF Edge", "Industrial Automation Integrators", "Academic Systems Research Labs",
        ],
        "about": [
            "Deterministic Scheduling",
            "Energy-Proportional Computing",
            "SMP Per-CPU Accounting",
            "Lazy TTL Cache Evaluation",
        ],
        "keywords": [
            "Zero-Idle", "HLT", "SMP", "APIC Timer Gating", "Per-CPU Accounting",
            "Lazy TTL", "Preemptive Scheduler", "Energy-Proportional Runtime",
        ],
        "match_terms": (
            "zero-idle", "hlt", "smp", "per-cpu", "percpu", "apic", "tickless", "escalonador",
            "scheduler", "lazy", "prefetch", "prefet", "pic", "eoi", "0.00%", "0,00%", "ociosa",
            "edge", "fanless", "watt", "energia",
        ),
    },
    "microkernel-security": {
        "id": "microkernel-security",
        "short_label": "Microkernel & Segurança",
        "title": "Zero-Trust Isolation, W^X Enforcement & Supply-Chain Integrity",
        "badge_class": "bg-amber-950/80 border-amber-700/70 text-amber-300 hover:border-amber-400",
        "ecosystems": [
            "Red Hat", "Canonical", "Google", "Siemens", "Bosch", "STMicroelectronics",
            "Industrial Automation Integrators", "Sovereign Public Infrastructure",
        ],
        "about": [
            "Microkernel Architecture",
            "W^X Memory Enforcement",
            "Ring 3 Driver Isolation",
            "Supply-Chain Integrity",
            "TLS 1.3 Cryptography",
        ],
        "keywords": [
            "Microkernel Architecture", "Ring 3 Drivers", "W^X", "Zero-Trust Syscalls", "JAHA 1.2",
            "JinnWall SPI", "Supply-Chain Integrity", "TLS 1.3 Cryptography", "SHA-256",
        ],
        "match_terms": (
            "microkernel", "isolamento", "supervisor", "ring 3", "ring3", "w^x", "efault",
            "zero-trust", "segurança", "seguranca", "auditoria", "homolog", "jaha",
            "criptografia", "poly1305", "sha-256", "sha256", "jpkg", "pacote", "fat32",
            "ext2", "vfs", "jinnwall", "auto-recupera", "soberan",
        ),
    },
    "fastpath-ipc-net": {
        "id": "fastpath-ipc-net",
        "short_label": "IPC & Redes Fast-Path",
        "title": "Zero-Copy Ring Buffers & L2/L3 Fast-Path Networking",
        "badge_class": "bg-cyan-950/80 border-cyan-700/70 text-cyan-300 hover:border-cyan-400",
        "ecosystems": [
            "Cloudflare", "Akamai", "Google", "Huawei", "RISC-V International", "LF Edge",
            "Industrial Automation Integrators", "Sovereign Public Infrastructure",
        ],
        "about": [
            "Zero-Copy IPC",
            "L2/L3 Fast-Path Networking",
            "Three-Tier DNS Resolution",
            "TLS 1.3 Cryptography",
            "Bare-Metal TCP/IP Stack",
        ],
        "keywords": [
            "SPSC Ring Buffer", "Zero-Copy IPC", "io_uring", "HotRouteTable",
            "3-Tier DNS", "e1000", "VirtIO-Net", "Jinn Browser", "TLS 1.3", "PCAP Telemetry",
        ],
        "match_terms": (
            "spsc", "shared-rings", "io_uring", "zero-copy", "ipc", "rede", "redes", "dns",
            "arp", "e1000", "virtio", "hotroute", "fast-path", "tcp", "http", "tls",
            "browser", "navegador", "pcap", "redirecionamento", "sitemap", "websub",
        ),
    },
}

# Regras finas de associação semântica para Schema.org `about` (conceito -> gatilhos no texto)
SEMANTIC_CONCEPT_RULES = [
    ("Microkernel Architecture", ("microkernel", "ring 3", "ring3", "syscall", "pml4", "supervisor")),
    ("SIMD Vectorization", ("simd", "avx2", "avx", "fma", "neon", "vetorial", "vector")),
    ("Zero-Copy IPC", ("zero-copy", "spsc", "shared-rings", "sharedrings", "io_uring", "ipc", "ring buffer")),
    ("TLS 1.3 Cryptography", ("tls", "tls 1.3", "poly1305", "chacha20", "aes-128-gcm", "x25519", "x.509", "x509", "criptografia")),
    ("Deterministic Scheduling", ("deterministic", "determinist", "zero-idle", "hlt", "apic", "scheduler", "escalonador", "smp", "per-cpu")),
    ("W^X Memory Enforcement", ("w^x", "pml4", "efault", "jaha", "nx bit", "pagina", "memory safety")),
    ("Heterogeneous Compute Scheduling", ("jhcs", "breakeven", "wgpu", "dag", "heterog", "offload")),
    ("Zero-GPU 2D Compositing", ("zero-gpu", "aladdin", "frosted glass", "acrylic", "acrílico", "dirtyrect", "dirty rect", "framebuffer")),
    ("L2/L3 Fast-Path Networking", ("hotroute", "fast-path", "arp", "e1000", "virtio", "pcap", "tcp/ip", "slirp")),
    ("Three-Tier DNS Resolution", ("dns", "lru", "/disk/hosts", "1.1.1.1", "8.8.8.8", "ttl")),
    ("Fault-Tolerant VFS Persistence", ("fat32", "ext2", "vfs", ".jpkg", "jpkg", "lfn", ".bak", "persist")),
    ("Semantic Web & News Indexing", ("json-ld", "techarticle", "newsarticle", "schema.org", "sitemap", "websub", "observatory", "google news")),
]

# Regras contextuais para Schema.org `mentions` (organizações, padrões e radar expandido)
MENTION_RULES = [
    ("Intel", ("x86_64", "intel", "avx2", "avx", "e1000", "apic", "8259a", "hlt")),
    ("AMD", ("x86_64", "amd", "avx2", "jhcs", "zen", "simd", "wgpu")),
    ("Arm", ("arm", "arm64", "aarch64", "neon", "edge", "zero-idle", "jhcs")),
    ("NVIDIA", ("nvidia", "gpu", "wgpu", "cuda", "breakeven", "offload", "computação heterogênea", "computacao heterogenea")),
    ("Cloudflare", ("cloudflare", "1.1.1.1", "tls", "dns", "http", "edge", "fast-path", "zero-trust")),
    ("Red Hat", ("microkernel", "linux", "red hat", "io_uring", "ebpf", "ring 3", "zero-idle")),
    ("Siemens", ("siemens", "plc", "scada", "ihm", "automação", "automacao", "industrial", "edge", "deterministic")),
    ("Bosch", ("bosch", "embarcado", "embedded", "automação", "automacao", "sensor", "industrial", "zero-idle")),
    ("STMicroelectronics", ("stmicroelectronics", "microcontrol", "embedded", "fanless", "telemetria", "sensor", "edge")),
    ("RISC-V International", ("risc-v", "riscv", "sifive", "portabilidade", "no_std", "microkernel")),
    ("LF Edge", ("lf edge", "edge", "gateway", "iot", "micro-edge", "fanless", "zero-idle")),
    ("Google", ("google", "8.8.8.8", "websub", "pubsubhubbub", "newsarticle", "json-ld", "sitemap", "google ads", "swg")),
    ("Tenstorrent", ("tenstorrent", "jhcs", "dag", "heterog", "acelerador")),
    ("SiFive", ("sifive", "risc-v", "riscv", "vetorial")),
    ("Qualcomm", ("qualcomm", "arm", "energy-proportional", "zero-idle", "bateria")),
    ("Canonical", ("canonical", "ubuntu", "linux", "jpkg", "supply-chain")),
    ("Huawei", ("huawei", "fast-path", "rede", "5g", "edge")),
    ("Industrial Automation Integrators", ("industrial", "automação", "automacao", "plc", "ihm", "scada", "gateway", "fanless", "zero-idle", "zero-gpu", "ipc", "fat32")),
    ("Academic Systems Research Labs", ("benchmark", "pesquisa", "acad", "universidade", "tabela b", "qemu", "microkernel", "scheduler", "jhcs", "auditoria")),
    ("Sovereign Public Infrastructure", ("soberan", "auditável", "auditavel", "zero crates.io", "setor público", "setor publico", "w^x", "jaha", "tls 1.3", ".jpkg", "browser")),
]

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
            "json-ld", "observatory", "captura", "sitemap", "indexacao",
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
    "Intel", "AMD", "Arm", "NVIDIA", "Tenstorrent", "SiFive", "Qualcomm", "LF Edge",
    "Red Hat", "Canonical", "Google", "Siemens", "Bosch", "STMicroelectronics",
    "Cloudflare", "Akamai", "Huawei", "RISC-V International",
}

RADAR_EXPANDED_ENTITIES = (
    "Industrial Automation Integrators",
    "Academic Systems Research Labs",
    "Sovereign Public Infrastructure",
)


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


def classify_semantic_metadata(
    tag: str,
    all_tags: list[str],
    slug: str,
    title: str,
    body_text: str,
    ecosystems: list[str],
) -> tuple[list[str], list[str], list[str]]:
    """Extrai conceitos `about`, entidades `mentions` contextuais e keywords técnicas de busca."""
    haystack = " ".join([tag, *all_tags, slug, title, body_text[:3500]]).lower()

    about_list: list[str] = []
    seen_about: set[str] = set()
    for concept, triggers in SEMANTIC_CONCEPT_RULES:
        if any(t in haystack for t in triggers):
            if concept.lower() not in seen_about:
                seen_about.add(concept.lower())
                about_list.append(concept)
    for eco_id in ecosystems:
        for topic in ECOSYSTEM_DOMAINS[eco_id]["about"]:
            if topic.lower() not in seen_about and len(about_list) < 8:
                seen_about.add(topic.lower())
                about_list.append(topic)

    mention_scored: list[tuple[int, int, str]] = []
    for idx, (org, triggers) in enumerate(MENTION_RULES):
        hits = sum(1 for t in triggers if t in haystack)
        if hits > 0:
            mention_scored.append((hits, -idx, org))
    mention_scored.sort(reverse=True)
    mentions_list: list[str] = [org for _, _, org in mention_scored[:10]]
    seen_mentions = {m.lower() for m in mentions_list}
    for eco_id in ecosystems:
        for org in ECOSYSTEM_DOMAINS[eco_id]["ecosystems"]:
            if org.lower() not in seen_mentions and len(mentions_list) < 12:
                seen_mentions.add(org.lower())
                mentions_list.append(org)
    # Garante presença contextual das entidades do Radar Expandido pertinentes ao tema
    for entity in RADAR_EXPANDED_ENTITIES:
        if entity.lower() not in seen_mentions:
            # Adiciona se o tema tem afinidade ou como radar complementar (máx. 12 menções)
            if len(mentions_list) < 12:
                seen_mentions.add(entity.lower())
                mentions_list.append(entity)

    keywords = ["Jinn OS", "Microkernel", "Rust no_std", "x86_64", *all_tags, *about_list[:5]]
    for eco_id in ecosystems:
        keywords.extend(ECOSYSTEM_DOMAINS[eco_id]["keywords"])
    dedup_kw: list[str] = []
    seen_kw: set[str] = set()
    for kw in keywords:
        kl = kw.strip().lower()
        if kl and kl not in seen_kw:
            seen_kw.add(kl)
            dedup_kw.append(kw.strip())

    return about_list[:8], mentions_list[:12], dedup_kw[:18]


def clamp_headline(title: str, max_len: int = 110) -> str:
    """Garante headline <= 110 caracteres preservando contexto técnico máximo."""
    norm = " ".join(title.split()).strip()
    if len(norm) <= max_len:
        return norm
    for sep in (" — ", " – ", ": ", " - "):
        if sep in norm:
            prefix = norm.split(sep, 1)[0].strip()
            if 72 <= len(prefix) <= max_len:
                return prefix
    clipped = norm[: max_len - 1]
    if " " in clipped:
        clipped = clipped.rsplit(" ", 1)[0]
    return clipped.rstrip(" ,;:-—–") + "…"


def extract_visual_evidences(raw_html: str, default_image_url: str, default_title: str) -> list[dict]:
    """Extrai evidências visuais (<figure> / <img>) de um artigo HTML para RSS e Release Notes."""
    evidences: list[dict] = []
    seen_urls: set[str] = set()
    for fig in re.findall(r"<figure\b[^>]*>(.*?)</figure>", raw_html, flags=re.IGNORECASE | re.DOTALL):
        img_m = re.search(r'<img\b[^>]*src=["\']([^"\']+)["\'][^>]*>', fig, flags=re.IGNORECASE)
        if not img_m:
            continue
        src = img_m.group(1).strip()
        if "valid-rss" in src:
            continue
        if src.startswith("../assets/"):
            full_url = f"{SITE_URL}/assets/{src[len('../assets/'):]}"
        elif src.startswith("/assets/"):
            full_url = f"{SITE_URL}{src}"
        elif src.startswith("assets/"):
            full_url = f"{SITE_URL}/{src}"
        elif src.startswith("http"):
            full_url = src
        else:
            continue
        if full_url in seen_urls:
            continue
        seen_urls.add(full_url)
        alt_m = re.search(r'alt=["\']([^"\']*)["\']', img_m.group(0), flags=re.IGNORECASE)
        cap_m = re.search(r"<figcaption\b[^>]*>(.*?)</figcaption>", fig, flags=re.IGNORECASE | re.DOTALL)
        caption = ""
        if cap_m:
            caption = " ".join(html_lib.unescape(re.sub(r"<[^>]+>", " ", cap_m.group(1))).split())
        elif alt_m:
            caption = html_lib.unescape(alt_m.group(1).strip())
        evidences.append({"url": full_url, "caption": caption[:160]})
    if not evidences:
        evidences.append({"url": default_image_url, "caption": clamp_headline(default_title, 110)})
    return evidences[:6]


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
        r"(?<![A-Za-z])(?:\d+(?:[.,]\d+)?\s*(?:%|bytes|\sB\b|(?<=\d)B\b|KiB|MB|GB|ms|µs|us|ns|Hz|FPS|pkts|pacotes|ciclos|vCPUs?|cores|HLT|discrepancy)|\b0[.,]00%\b|\b0 bytes\b|\b0 pacotes\b)",
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
    """Constrói bloco JSON-LD Schema.org TechArticle + NewsArticle (headline <= 110, TZ -03:00)."""
    about_names = meta.get("about_concepts") or []
    mention_names = meta.get("mention_entities") or []
    keywords = meta.get("search_keywords") or ["Jinn OS", "Microkernel", "Rust no_std", *meta["all_tags"]]

    about_terms = [{"@type": "Thing", "name": topic} for topic in about_names]
    mentions_list = [
        {"@type": ("Organization" if org in CORPORATE_ORGS else "Thing"), "name": org}
        for org in mention_names
    ]

    lab_titles = [OBSERVATORY_LABS[l]["title"] for l in meta.get("labs", []) if l in OBSERVATORY_LABS]
    pub_iso_brt = meta["pub_iso_full"]
    if not pub_iso_brt.endswith("-03:00"):
        try:
            dt = datetime.fromisoformat(pub_iso_brt.replace("Z", "+00:00"))
            pub_iso_brt = dt.astimezone(TZ_BRT).isoformat(timespec="seconds")
        except Exception:
            pub_iso_brt = f"{meta['date_iso']}T18:00:00-03:00"

    img_url = meta.get("image", DEFAULT_IMAGE_URL)
    payload = {
        "@context": "https://schema.org",
        "@type": ["TechArticle", "NewsArticle"],
        "headline": clamp_headline(meta["title"], 110),
        "description": meta["description"],
        "image": [img_url],
        "datePublished": pub_iso_brt,
        "dateModified": pub_iso_brt,
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


DOMAIN_APPLICABILITY = {
    "compute-simd": {
        "industry": "Visão computacional leve, inspeção em esteira e IHMs sem GPU discreta usando kernels AVX2/FMA e breakeven escalar/GPU no JHCS.",
        "home": "Desktop fluido e navegador nativo em PCs/laptops legados ou sem aceleração 3D via compositor Aladdin Zero-GPU.",
        "server": "Brokers heterogêneos (jhcs-rs) evitando penalidade PCIe em micro-lotes matriciais e pipelines mistos CPU+GPU.",
    },
    "zero-idle-efficiency": {
        "industry": "Controladores fanless, subestações e telemetria off-grid com APs em HLT e contabilidade SMP por núcleo em QEMU (validação térmica/watts física via Tabela B).",
        "home": "Notebooks antigos, mini-PCs domésticos e estações silenciosas sem polling contínuo em repouso.",
        "server": "Consolidação de micro-instâncias, gateways de borda e redução de wake-ups espúrios de timer APIC.",
    },
    "microkernel-security": {
        "industry": "Terminais críticos, PLCs e gateways auditáveis com isolamento Ring 3, W^X em PML4 e integridade offline SHA-256 (.jpkg).",
        "home": "Navegação isolada, recuperação automática de sistema de arquivos FAT32/Ext2 e operação sem telemetria oculta.",
        "server": "Contenção de falhas de drivers em espaço de usuário, fronteira zero-trust de syscalls e redução de cadeia de suprimentos (0 crates.io no kernel).",
    },
    "fastpath-ipc-net": {
        "industry": "Gateways industriais, coletores SCADA e frotas com links intermitentes via SharedRings SPSC (4096 B), HotRouteTable e diagnóstico de falha por estágio.",
        "home": "Navegação ágil com DNS 3 camadas (/disk/hosts → LRU → upstream) e fallback resiliente de conectividade.",
        "server": "Caminhos críticos IPC lock-free e observabilidade de pilha TCP/TLS 1.3 no_std com inspeção direta por syscalls.",
    },
}


def audit_rrm_compliance(html_content: str) -> dict:
    """Verifica conformidade com Google News Reader Revenue Manager (autoPromptType: 'none' e CTA apenas no rodapé)."""
    has_swg = "swg-basic.js" in html_content
    has_auto_none = bool(re.search(r'autoPromptType\s*:\s*["\']none["\']', html_content))
    cta_m = re.search(r"rrm-inline-cta", html_content)
    footer_only = False
    if cta_m and len(html_content) > 0:
        footer_only = (cta_m.start() / len(html_content)) >= 0.50
    return {
        "swg": has_swg,
        "auto_prompt_none": has_auto_none,
        "footer_cta_only": footer_only,
    }


def build_collaboration_section_html(meta: dict) -> str:
    """Gera bloco visual padronizado com badges clicáveis, Aplicabilidade (Indústria/Doméstico/Servidores) e Chamada Aberta para Pilotos."""
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
    for eco_id in meta["ecosystems"]:
        cfg = ECOSYSTEM_DOMAINS[eco_id]
        domain_badges.append(
            f'<a href="/artigos/?eco={eco_id}" '
            f'class="inline-flex items-center gap-1.5 px-3 py-1 rounded-full border text-xs font-mono transition-colors {cfg["badge_class"]}">'
            f'<span>{html_lib.escape(cfg["short_label"])}</span>'
            f'<span class="opacity-70">&bull; {html_lib.escape(cfg["title"])}</span>'
            f"</a>"
        )

    mention_badges = []
    for eco_name in meta.get("mention_entities", []):
        q_param = html_lib.escape(eco_name.replace(" ", "+"), quote=True)
        is_radar = eco_name in RADAR_EXPANDED_ENTITIES
        pill_cls = (
            "bg-emerald-950/60 border-emerald-700/70 text-emerald-200 hover:border-emerald-400"
            if is_radar
            else "bg-slate-900 border-slate-700/80 text-slate-300 hover:border-cyan-400 hover:text-cyan-200"
        )
        mention_badges.append(
            f'<a href="/artigos/?q={q_param}" '
            f'class="px-2.5 py-0.5 rounded border text-[11px] font-mono transition-colors {pill_cls}">'
            f'{html_lib.escape(eco_name)}</a>'
        )

    primary_eco = meta["ecosystems"][0] if meta["ecosystems"] else "microkernel-security"
    app_cfg = DOMAIN_APPLICABILITY.get(primary_eco, DOMAIN_APPLICABILITY["microkernel-security"])

    labs_html = "\n          ".join(lab_badges)
    badges_html = "\n          ".join(domain_badges)
    ecos_html = "\n          ".join(mention_badges)

    return f"""      {ECO_BLOCK_START}
      <section aria-labelledby="ecosystems-collaboration-heading" class="not-prose my-10 p-6 rounded-xl bg-slate-900/75 border border-slate-800 space-y-5">
        <div class="space-y-2">
          <p class="text-[11px] font-mono uppercase tracking-wider text-cyan-400">Jinn Engineering Observatory • Semântica &amp; Colaboração Aberta</p>
          <h3 id="ecosystems-collaboration-heading" class="text-lg sm:text-xl font-bold text-white">
            Domínios Técnicos &amp; Ecossistemas Relacionados
          </h3>
          <p class="text-xs sm:text-sm text-slate-300 leading-relaxed">
            Badges clicáveis indexados por bancada, domínio de engenharia e organizações/padrões do Radar Expandido (Nível B contextual, sem endosso implícito):
          </p>
        </div>

        <div class="flex flex-wrap gap-2">
          {labs_html}
          {badges_html}
        </div>

        <div class="pt-1">
          <div class="text-[11px] font-mono text-slate-400 mb-2">Ecossistemas, Padrões &amp; Radar Expandido (clique para filtrar o acervo):</div>
          <div class="flex flex-wrap gap-1.5">
            {ecos_html}
          </div>
        </div>

        <div class="border-t border-slate-800 pt-4 space-y-3">
          <h4 class="text-sm font-bold text-cyan-300 font-mono">
            Aplicabilidade: Indústria, Uso Doméstico e Servidores
          </h4>
          <div class="grid grid-cols-1 sm:grid-cols-3 gap-3 text-xs">
            <div class="p-3 rounded-lg bg-slate-950/80 border border-slate-800/90 space-y-1">
              <div class="font-mono font-semibold text-emerald-300">Indústria &amp; Edge</div>
              <p class="text-slate-400 leading-relaxed">{html_lib.escape(app_cfg["industry"])}</p>
            </div>
            <div class="p-3 rounded-lg bg-slate-950/80 border border-slate-800/90 space-y-1">
              <div class="font-mono font-semibold text-cyan-300">Uso Doméstico &amp; Educação</div>
              <p class="text-slate-400 leading-relaxed">{html_lib.escape(app_cfg["home"])}</p>
            </div>
            <div class="p-3 rounded-lg bg-slate-950/80 border border-slate-800/90 space-y-1">
              <div class="font-mono font-semibold text-purple-300">Servidores &amp; HPC</div>
              <p class="text-slate-400 leading-relaxed">{html_lib.escape(app_cfg["server"])}</p>
            </div>
          </div>
        </div>

        <div class="border-t border-slate-800 pt-4 space-y-3">
          <h4 class="text-sm font-bold text-emerald-300 font-mono">
            Chamada Aberta para Pilotos (Indústria Local, Universidades &amp; Setor Público Ético)
          </h4>
          <p class="text-xs sm:text-sm text-slate-300 leading-relaxed">
            Convidamos <strong>integradores de automação industrial</strong>, <strong>laboratórios universitários de arquitetura/SO</strong> e <strong>instituições públicas eticamente responsáveis</strong> a reproduzirem estes experimentos em silício real (fechando lacunas emulador vs. hardware da Tabela B) ou propor estudos-piloto com <strong>Jinn OS</strong>, <strong>Aladdin</strong> e <strong>JHCS</strong>.
          </p>
          <div class="flex flex-wrap gap-2.5 text-xs font-mono pt-1">
            <a href="{REPO_URL}" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-cyan-400 text-slate-200 hover:text-cyan-300 transition-colors">
              GitHub: Jinn_Project
            </a>
            <a href="{JHCS_REPO_URL}" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-purple-400 text-slate-200 hover:text-purple-300 transition-colors">
              GitHub: jhcs-rs
            </a>
            <a href="{REPO_URL}/blob/main/docs/ENGINEERING_STATUS_AND_LIMITATIONS.md" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-amber-400 text-slate-200 hover:text-amber-300 transition-colors">
              Fronteira Honesta &amp; Tabela B
            </a>
            <a href="/solucoes/edge-industrial-zero-idle.html" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-emerald-400 text-slate-200 hover:text-emerald-300 transition-colors">
              Soluções &amp; Benchmarks
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

    # Data de modificação / publicação normalizada em ISO 8601 com timezone -03:00
    pub_date_rfc2822 = "Tue, 24 Nov 2026 18:00:00 -0300"
    pub_date_iso = "2026-11-24"
    pub_iso_full = "2026-11-24T18:00:00-03:00"
    sort_ts = "2026-11-24T21:00:00"

    pub_match = re.search(r'<meta\s+name=["\']article:published_time["\']\s+content=["\'](.*?)["\']', content, re.IGNORECASE)
    dt_resolved = None
    if pub_match:
        raw_time = pub_match.group(1).strip()
        try:
            dt_parsed = datetime.fromisoformat(raw_time.replace("Z", "+00:00"))
            if dt_parsed.tzinfo is None:
                dt_parsed = dt_parsed.replace(tzinfo=TZ_BRT)
            dt_resolved = dt_parsed
        except Exception:
            dt_resolved = None

    if dt_resolved is None:
        try:
            git_date = subprocess.check_output(
                ["git", "log", "-1", "--format=%cI", str(html_path)],
                text=True, encoding="utf-8"
            ).strip()
            if git_date:
                dt_resolved = datetime.fromisoformat(git_date.replace("Z", "+00:00"))
        except Exception:
            dt_resolved = None

    if dt_resolved is None:
        stat = html_path.stat()
        dt_resolved = datetime.fromtimestamp(stat.st_mtime, timezone.utc)

    dt_brt = dt_resolved.astimezone(TZ_BRT)
    pub_iso_full = dt_brt.isoformat(timespec="seconds")
    pub_date_rfc2822 = dt_brt.strftime("%a, %d %b %Y %H:%M:%S %z")
    pub_date_iso = dt_brt.strftime("%Y-%m-%d")
    sort_ts = dt_resolved.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S")

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
    about_concepts, mention_entities, search_keywords = classify_semantic_metadata(
        tag, all_tags, html_path.stem, title, plain_text, ecosystems
    )
    visual_evidences = extract_visual_evidences(body_src, image_url, title)
    rrm_audit = audit_rrm_compliance(content)
    measurable_gain = extract_measurable_gain(description, plain_text)
    guid = f"jinn-os-artigo-{html_path.stem}"

    meta = {
        "slug": html_path.stem,
        "filename": html_path.name,
        "title": title,
        "headline": clamp_headline(title, 110),
        "description": description,
        "tag": tag,
        "all_tags": all_tags,
        "ecosystems": ecosystems,
        "labs": labs,
        "buckets": ecosystems,
        "about_concepts": about_concepts,
        "mention_entities": mention_entities,
        "search_keywords": search_keywords,
        "visual_evidences": visual_evidences,
        "rrm_audit": rrm_audit,
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
        about_terms = " ".join(a.get("about_concepts", []))
        mentions_list = a.get("mention_entities") or []
        kw_terms = " ".join(a.get("search_keywords", []))
        tags_text = " ".join(a["all_tags"])
        search_blob = html_lib.escape(
            f"{a['title']} {a['description']} {tags_text} {lab_terms} {eco_terms} {about_terms} {' '.join(mentions_list)} {kw_terms}".lower(),
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
        eco_badges_html = "".join(lab_buttons + eco_buttons[:2])

        mention_pills = []
        for org_name in mentions_list[:5]:
            q_esc = html_lib.escape(org_name, quote=True)
            is_radar = org_name in RADAR_EXPANDED_ENTITIES
            m_cls = (
                "border-emerald-800/80 text-emerald-300 bg-emerald-950/40 hover:border-emerald-400"
                if is_radar
                else "border-slate-700/80 text-slate-300 bg-slate-950/80 hover:border-cyan-400 hover:text-cyan-200"
            )
            mention_pills.append(
                f'<button type="button" data-q-badge="{q_esc}" '
                f'title="Filtrar artigos relacionados a {q_esc}" '
                f'class="px-1.5 py-0.5 rounded border text-[10px] font-mono transition-colors {m_cls}">'
                f'{html_lib.escape(org_name)}</button>'
            )
        mentions_pills_html = "".join(mention_pills)
        ref_line = html_lib.escape(", ".join(mentions_list[:6]))

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
          <div class="flex flex-wrap items-center gap-1 mt-2" title="Ecossistemas relacionados: {ref_line}">
            <span class="text-[10px] font-mono text-slate-500 mr-0.5">Ecossistemas:</span>{mentions_pills_html}
          </div>
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
        <div class="flex flex-col md:flex-row md:items-center gap-2.5 justify-between">
          <div class="relative w-full md:w-80 lg:w-96 shrink-0">
            <i data-lucide="search" class="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none"></i>
            <input id="article-search" type="search" placeholder="Buscar por título, bancada, ecossistema ou métrica..."
              class="w-full pl-9 pr-3 py-1.5 text-xs sm:text-sm rounded-lg bg-slate-950/90 border border-slate-700/80 text-slate-100 placeholder-slate-500 focus:outline-none focus:border-cyan-400 transition-colors" />
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

    <!-- Bloco Institucional: Aplicabilidade & Chamada Aberta para Pilotos -->
    <section aria-labelledby="central-collaboration-heading" class="mt-14 p-6 sm:p-8 rounded-2xl bg-gradient-to-br from-slate-900/90 via-slate-900/70 to-cyan-950/30 border border-slate-800 space-y-6">
      <div class="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div class="space-y-2 max-w-3xl">
          <p class="text-xs font-mono uppercase tracking-wider text-cyan-400">Jinn Open Engineering Outreach • Schema.org &amp; Radar Expandido</p>
          <h2 id="central-collaboration-heading" class="text-xl sm:text-2xl font-bold text-white">
            Aplicabilidade: Indústria, Uso Doméstico e Servidores
          </h2>
          <p class="text-sm text-slate-300 leading-relaxed">
            Cada publicação deste portal possui classificação automática <code>TechArticle</code>/<code>NewsArticle</code> (<code>about</code> + <code>mentions</code> contextuais) ligando engenharia verificável aos três cenários reais abaixo e ao Radar Expandido:
          </p>
        </div>
        <a href="/#win-contact" class="shrink-0 inline-flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl bg-cyan-500/15 border border-cyan-400/60 hover:bg-cyan-500/25 text-cyan-200 font-mono text-xs font-semibold transition-colors">
          <i data-lucide="mail" class="w-4 h-4"></i>
          <span>Propor Piloto ou Benchmark</span>
        </a>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-3 gap-4 text-xs sm:text-sm">
        <div class="p-4 rounded-xl bg-slate-950/75 border border-slate-800/90 space-y-1.5">
          <div class="font-mono text-emerald-300 font-semibold">Indústria &amp; Automação (Edge/IHM)</div>
          <p class="text-slate-400 leading-relaxed">Integradores PLC/IHM SCADA, subestações e gateways <em>fanless</em> buscando isolamento Ring 3, orçamento <code>64–256 MiB</code>, persistência FAT32/Ext2 e operação sem GPU discreta.</p>
        </div>
        <div class="p-4 rounded-xl bg-slate-950/75 border border-slate-800/90 space-y-1.5">
          <div class="font-mono text-cyan-300 font-semibold">Uso Doméstico &amp; Laboratórios Educacionais</div>
          <p class="text-slate-400 leading-relaxed">Reaproveitamento de microcomputadores e ensino de SO em Rust <code>no_std</code> com compositor Zero-GPU <code>Aladdin</code>, navegador isolado e transparência completa de código.</p>
        </div>
        <div class="p-4 rounded-xl bg-slate-950/75 border border-slate-800/90 space-y-1.5">
          <div class="font-mono text-purple-300 font-semibold">Servidores, Bordas &amp; Computação Heterogênea</div>
          <p class="text-slate-400 leading-relaxed">Orquestração <code>jhcs-rs</code> (SIMD AVX2 vs offload GPU <code>wgpu</code>), <code>HotRouteTable</code> L2/L3 e verificação <code>W^X</code>/JAHA 1.2 (sujeitos às fronteiras da Tabela B).</p>
        </div>
      </div>

      <div class="border-t border-slate-800/80 pt-5 space-y-3">
        <h3 class="text-base sm:text-lg font-bold text-emerald-300 font-mono">
          Chamada Aberta para Pilotos (Indústria Local, Universidades &amp; Setor Público Ético)
        </h3>
        <p class="text-xs sm:text-sm text-slate-300 leading-relaxed">
          Convidamos <strong>empresas regionais de automação industrial</strong>, <strong>grupos universitários de pesquisa em sistemas operacionais e arquitetura</strong> e <strong>instituições públicas éticas</strong> a validar cargas em hardware físico real (wattímetro/RAPL, nic e drivers de campo), comparar benchmarks reproduzíveis e propor estudos conjuntos.
        </p>
      </div>

      <div class="flex flex-wrap items-center gap-3 text-xs font-mono pt-1">
        <a href="{REPO_URL}" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-cyan-400 text-slate-200 transition-colors">Repositório Jinn_Project</a>
        <a href="{JHCS_REPO_URL}" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-purple-400 text-slate-200 transition-colors">Workspace JHCS (jhcs-rs)</a>
        <a href="/solucoes/computacao-heterogenea-jhcs.html" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-emerald-400 text-slate-200 transition-colors">Landing Pages /solucoes</a>
        <a href="{REPO_URL}/blob/main/docs/ENGINEERING_STATUS_AND_LIMITATIONS.md" target="_blank" rel="noopener noreferrer" class="px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 hover:border-amber-400 text-slate-200 transition-colors">Fronteira Honesta &amp; Tabela B</a>
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
      const qBadges = Array.from(document.querySelectorAll('[data-q-badge]'));
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
          const qVal = (searchInput ? searchInput.value : '').trim();
          if (!qVal) url.searchParams.delete('q'); else url.searchParams.set('q', qVal);
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

      qBadges.forEach(function (badge) {{
        badge.addEventListener('click', function (ev) {{
          ev.preventDefault();
          ev.stopPropagation();
          if (searchInput) {{
            searchInput.value = badge.getAttribute('data-q-badge') || '';
          }}
          syncUrlParams();
          applyFilters();
          window.scrollTo({{ top: 0, behavior: 'smooth' }});
        }});
      }});

      if (searchInput) searchInput.addEventListener('input', function () {{
        syncUrlParams();
        applyFilters();
      }});
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
      const initialQuery = params.get('q');
      if (initialQuery && searchInput) {{
        searchInput.value = initialQuery;
      }}
      if (initialLab && labPills.some(function (p) {{ return p.getAttribute('data-lab-filter') === initialLab; }})) {{
        selectLabFilter(initialLab, false);
      }}
      if (initialEco && pills.some(function (p) {{ return p.getAttribute('data-filter') === initialEco; }})) {{
        selectEcoFilter(initialEco, false);
      }}
      if (initialQuery && !initialLab && !initialEco) {{
        applyFilters();
      }}
    }})();
  </script>
</body>
</html>
"""
    INDEX_PATH.write_text(html_content, encoding="utf-8")
    print(f"  [PASS] {INDEX_PATH} gerado com sucesso!")


def format_ecosystem_summary_text(
    base_summary: str,
    gain: str,
    eco_ids: list[str],
    keywords: list[str] | None = None,
    mentions: list[str] | None = None,
    evidences: list[dict] | None = None,
) -> str:
    """Formata descrição enriquecida para RSS/Atom com ganhos, ecossistemas, keywords e evidências visuais."""
    eco_lines = []
    for eco_id in eco_ids:
        cfg = ECOSYSTEM_DOMAINS[eco_id]
        eco_lines.append(f"{cfg['short_label']} ({', '.join(cfg['ecosystems'][:4])})")
    eco_str = " | ".join(eco_lines) if eco_lines else "Systems Engineering & Open Hardware"
    mentions_str = ", ".join((mentions or [])[:8])
    kw_str = ", ".join((keywords or [])[:10])
    ev_parts = []
    for ev in (evidences or [])[:3]:
        cap = ev.get("caption") or "Evidência técnica"
        ev_parts.append(f"{cap} ({ev.get('url', DEFAULT_IMAGE_URL)})")
    ev_str = " ; ".join(ev_parts) if ev_parts else f"Captura Observatory ({DEFAULT_IMAGE_URL})"
    return (
        f"{base_summary} "
        f"[Ganho técnico: {gain}] "
        f"[Ecossistemas & Menções: {mentions_str or eco_str}] "
        f"[Palavras-chave: {kw_str or eco_str}] "
        f"[Evidências visuais: {ev_str}] "
        f"[Aplicabilidade: Indústria, Uso Doméstico e Servidores | Chamada Aberta para Pilotos: {SITE_URL}/#win-contact | {REPO_URL}]"
    )


def build_rss_feeds(articles, commits):
    """Gera feed.xml e rss.xml com ganhos técnicos, ecossistemas, palavras-chave, evidências visuais e WebSub."""
    print(f"[+] Gerando RSS 2.0 e WebSub Feeds em {FEED_PATH} e {RSS_PATH}...")
    ET.register_namespace('atom', 'http://www.w3.org/2005/Atom')
    ET.register_namespace('media', 'http://search.yahoo.com/mrss/')

    rss = ET.Element("rss", version="2.0")
    channel = ET.SubElement(rss, "channel")
    ET.SubElement(channel, "title").text = f"{PUBLISHER_NAME} — Devlog & Microkernel Releases"
    ET.SubElement(channel, "link").text = SITE_URL
    ET.SubElement(channel, "description").text = (
        "Artigos de engenharia (Schema.org TechArticle/NewsArticle), benchmarks bare-metal, computação heterogênea (JHCS), "
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
            art["description"],
            art["measurable_gain"],
            art["ecosystems"],
            keywords=art.get("search_keywords"),
            mentions=art.get("mention_entities"),
            evidences=art.get("visual_evidences"),
        )
        for eco_id in art["ecosystems"]:
            ET.SubElement(item, "category").text = eco_id
        for lab_id in art.get("labs", []):
            ET.SubElement(item, "category").text = lab_id
        for concept in art.get("about_concepts", [])[:3]:
            ET.SubElement(item, "category").text = concept

        enc = ET.SubElement(item, "enclosure")
        enc.set("url", art.get("image", DEFAULT_IMAGE_URL))
        enc.set("length", art.get("image_length", "70707"))
        enc.set("type", art.get("image_mime", "image/jpeg"))

    # 2. Commits Técnicos Recentes
    for c in commits:
        eco_ids = classify_ecosystems("COMMIT", [], "", c["title"], c["body"])
        labs_c = classify_labs("COMMIT", [], "", c["title"], c["body"])
        about_c, mentions_c, kw_c = classify_semantic_metadata(
            "COMMIT", labs_c, "", c["title"], c["body"], eco_ids
        )
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
            c["body"],
            gain,
            eco_ids,
            keywords=kw_c,
            mentions=mentions_c,
            evidences=[{"url": DEFAULT_IMAGE_URL, "caption": c["title"]}],
        )
        for eco_id in eco_ids:
            ET.SubElement(item, "category").text = eco_id
        for lab_id in labs_c:
            ET.SubElement(item, "category").text = lab_id
        for concept in about_c[:2]:
            ET.SubElement(item, "category").text = concept

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
    ET.SubElement(url_home, "{http://www.sitemaps.org/schemas/sitemap/0.9}lastmod").text = datetime.now(TZ_BRT).strftime("%Y-%m-%d")
    ET.SubElement(url_home, "{http://www.sitemaps.org/schemas/sitemap/0.9}changefreq").text = "daily"
    ET.SubElement(url_home, "{http://www.sitemaps.org/schemas/sitemap/0.9}priority").text = "1.0"

    # Índice de Artigos
    url_artigos = ET.SubElement(urlset, "{http://www.sitemaps.org/schemas/sitemap/0.9}url")
    ET.SubElement(url_artigos, "{http://www.sitemaps.org/schemas/sitemap/0.9}loc").text = f"{SITE_URL}/artigos/"
    ET.SubElement(url_artigos, "{http://www.sitemaps.org/schemas/sitemap/0.9}lastmod").text = datetime.now(TZ_BRT).strftime("%Y-%m-%d")
    ET.SubElement(url_artigos, "{http://www.sitemaps.org/schemas/sitemap/0.9}changefreq").text = "daily"
    ET.SubElement(url_artigos, "{http://www.sitemaps.org/schemas/sitemap/0.9}priority").text = "0.9"

    # Landing Pages Técnicas (/solucoes/)
    sol_count = 0
    sol_dir = WEB_DIR / "solucoes"
    if sol_dir.exists():
        for sol_file in sorted(sol_dir.glob("*.html")):
            url_sol = ET.SubElement(urlset, "{http://www.sitemaps.org/schemas/sitemap/0.9}url")
            ET.SubElement(url_sol, "{http://www.sitemaps.org/schemas/sitemap/0.9}loc").text = f"{SITE_URL}/solucoes/{sol_file.name}"
            ET.SubElement(url_sol, "{http://www.sitemaps.org/schemas/sitemap/0.9}lastmod").text = datetime.now(TZ_BRT).strftime("%Y-%m-%d")
            ET.SubElement(url_sol, "{http://www.sitemaps.org/schemas/sitemap/0.9}changefreq").text = "weekly"
            ET.SubElement(url_sol, "{http://www.sitemaps.org/schemas/sitemap/0.9}priority").text = "0.9"
            sol_count += 1

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
        ET.SubElement(news, "{http://www.google.com/schemas/sitemap-news/0.9}title").text = art["headline"]

    tree = ET.ElementTree(urlset)
    ET.indent(tree, space="  ")
    tree.write(SITEMAP_PATH, encoding="utf-8", xml_declaration=True)
    print(f"  [PASS] {len(articles) + 2 + sol_count} URLs catalogadas com sucesso em sitemap.xml!")


def main():
    print("==================================================================")
    print("  JINN OBSERVATORY — TECHARTICLE JSON-LD, ECOSSISTEMAS & FEEDS")
    print("==================================================================")
    articles = get_all_articles(enrich=True)
    print(f"[+] Artigos identificados e enriquecidos em {ARTIGOS_DIR}: {len(articles)}")
    max_hl = 0
    tz_ok = 0
    rrm_ok = 0
    for a in articles:
        hl_len = len(a["headline"])
        max_hl = max(max_hl, hl_len)
        if a["pub_iso_full"].endswith("-03:00"):
            tz_ok += 1
        rrm = a["rrm_audit"]
        if rrm["auto_prompt_none"] and rrm["footer_cta_only"]:
            rrm_ok += 1
        print(f"  - [{a['pub_iso_full']}] (hl={hl_len:3d}) [{','.join(a['ecosystems'])}] {a['title']}")
    print(f"[AUDIT] headlines max={max_hl}/110 | ISO -03:00={tz_ok}/{len(articles)} | SWG autoPromptType=none+rodape={rrm_ok}/{len(articles)}")

    commits = get_announced_commits(10)
    build_artigos_index(articles)
    build_rss_feeds(articles, commits)
    build_sitemap(articles)
    print("[+] Concluído com sucesso!")


if __name__ == "__main__":
    main()
