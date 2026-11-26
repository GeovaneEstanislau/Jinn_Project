#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR Jinn-Commercial-1.0
"""
scripts/syndicate_broadcast.py

Pipeline de Sindicação e Difusão Automatizada para o Jinn Engineering Observatory (jinn-os.dev.br)
Implementa Engenharia como Marketing + Jinn Open Engineering Outreach com Zero Custo Manual:
1. Gera formatos enriquecidos com ganho técnico mensurável e ecossistemas aplicáveis:
   - Formato 1: Microblogging (Bluesky, Mastodon, X)
   - Formato 2: Comunidade / Chat (Discord, Telegram via Webhooks)
   - Formato 3: RSS Feed Estático (web/feed.xml e web/rss.xml) + Release Notes GitHub
2. Despacha automaticamente via HTTP (APIs e Webhooks nativos sem dependências externas)
"""

import os
import sys
import argparse
import html as html_lib
import json
import urllib.request
import urllib.error
import datetime
import subprocess
import re
from pathlib import Path

WEB_DIR = Path(__file__).resolve().parent.parent / "web"
if str(WEB_DIR) not in sys.path:
    sys.path.insert(0, str(WEB_DIR))

from build_feed import (
    DEFAULT_IMAGE_URL,
    ECOSYSTEM_DOMAINS,
    OBSERVATORY_LABS,
    SITE_URL,
    classify_ecosystems,
    classify_labs,
    classify_semantic_metadata,
    extract_measurable_gain,
    extract_visual_evidences,
    format_ecosystem_summary_text,
)

DEFAULT_PORTAL_URL = "https://jinn-os.dev.br"
DEFAULT_REPO_URL = "https://github.com/GeovaneEstanislau/Jinn_Project"
ASSETS_DIR = WEB_DIR / "assets"
ARTIGOS_DIR = WEB_DIR / "artigos"

def get_git_info(commit_ref="HEAD"):
    """Obtém tag, hash do commit e mensagem completa do commit."""
    info = {
        "tag": "v0.0.3-alpha",
        "commit": "HEAD",
        "message": "Atualizacao de Engenharia do Jinn OS",
        "body": ""
    }
    try:
        tag = subprocess.check_output(["git", "describe", "--tags", "--always"], stderr=subprocess.DEVNULL).decode().strip()
        if tag: info["tag"] = tag
    except Exception: pass

    try:
        commit = subprocess.check_output(["git", "rev-parse", "--short", commit_ref], stderr=subprocess.DEVNULL).decode().strip()
        if commit: info["commit"] = commit
    except Exception: pass

    try:
        msg = subprocess.check_output(["git", "log", "-1", commit_ref, "--pretty=%s"], stderr=subprocess.DEVNULL).decode().strip()
        if msg: info["message"] = msg
    except Exception: pass

    try:
        body = subprocess.check_output(["git", "log", "-1", commit_ref, "--pretty=%b"], stderr=subprocess.DEVNULL).decode().strip()
        if body: info["body"] = body
    except Exception: pass

    return info

def discover_visual_evidences(link: str, eco_ids: list, lab_ids: list, title: str, body_text: str = "") -> list[dict]:
    """Extrai evidências visuais do artigo linkado ou das capturas recentes em web/assets/."""
    evidences: list[dict] = []
    if link and "/artigos/" in link:
        slug_name = link.rstrip("/").split("/")[-1].split("#")[0].split("?")[0]
        if slug_name.endswith(".html"):
            art_path = ARTIGOS_DIR / slug_name
            if art_path.exists():
                raw_html = art_path.read_text(encoding="utf-8", errors="ignore")
                og_m = re.search(r'<meta\s+property=["\']og:image["\']\s+content=["\']([^"\']+)["\']', raw_html, re.I)
                default_img = og_m.group(1).strip() if og_m else DEFAULT_IMAGE_URL
                evidences = extract_visual_evidences(raw_html, default_img, title)

    if not evidences and ASSETS_DIR.exists():
        haystack = f"{title} {body_text} {' '.join(eco_ids)} {' '.join(lab_ids)}".lower()
        scored: list[tuple[int, float, Path]] = []
        for asset in ASSETS_DIR.glob("*.png"):
            tokens = [t for t in re.split(r"[^a-z0-9]+", asset.stem.lower()) if len(t) >= 3]
            hits = sum(1 for tok in tokens if tok in haystack)
            scored.append((hits, asset.stat().st_mtime, asset))
        scored.sort(reverse=True)
        for _, _, asset in scored[:3]:
            evidences.append({
                "url": f"{SITE_URL}/assets/{asset.name}",
                "caption": f"Evidência visual Observatory ({asset.name})",
            })

    if not evidences:
        evidences.append({"url": DEFAULT_IMAGE_URL, "caption": title[:110]})
    return evidences[:4]


def format_ecosystem_labels(eco_ids):
    """Retorna rótulos concisos de domínios e ecossistemas de referência."""
    labels = []
    for eco_id in eco_ids:
        cfg = ECOSYSTEM_DOMAINS.get(eco_id)
        if cfg:
            labels.append(f"{cfg['short_label']} ({', '.join(cfg['ecosystems'][:4])})")
    return labels


def format_release_notes(
    title,
    version,
    summary,
    gain,
    eco_ids,
    lab_ids,
    link,
    commit,
    about_concepts=None,
    mention_entities=None,
    search_keywords=None,
    visual_evidences=None,
):
    """Gera template estruturado de Release Notes para GitHub Releases com ecossistemas, keywords e evidências visuais."""
    lab_bullets = ", ".join(OBSERVATORY_LABS[l]["title"] for l in lab_ids if l in OBSERVATORY_LABS) or "Architecture & RFCs"
    eco_bullets = "\n".join(
        f"- **{ECOSYSTEM_DOMAINS[e]['title']}** (`{e}`): {', '.join(ECOSYSTEM_DOMAINS[e]['ecosystems'])}"
        for e in eco_ids if e in ECOSYSTEM_DOMAINS
    )
    kw_line = ", ".join((search_keywords or [])[:12]) or "Jinn OS, Microkernel, Rust no_std"
    mentions_line = ", ".join((mention_entities or [])[:10]) or "Industrial Automation Integrators, Academic Systems Research Labs"
    about_line = ", ".join((about_concepts or [])[:6]) or "Microkernel Architecture"
    ev_lines = "\n".join(
        f"- [{ev.get('caption', 'Evidência visual')}]({ev.get('url', DEFAULT_IMAGE_URL)})"
        for ev in (visual_evidences or [])[:4]
    ) or f"- [Portal Observatory]({DEFAULT_IMAGE_URL})"
    return (
        f"## Jinn OS {version} — {title}\n\n"
        f"{summary}\n\n"
        f"### Bancada Observatory & Ganho Mensurável\n"
        f"- Bancada(s): **{lab_bullets}**\n"
        f"- Conceitos Semânticos (`about`): {about_line}\n"
        f"- {gain}\n"
        f"- Commit de referência: `{commit}`\n\n"
        f"### Tags de Ecossistemas & Menções Contextuais (`mentions`)\n"
        f"{eco_bullets}\n"
        f"- Radar & Organizações Contextuais: {mentions_line}\n\n"
        f"### Palavras-Chave Técnicas de Busca\n"
        f"`{kw_line}`\n\n"
        f"### Resumo das Evidências Visuais\n"
        f"{ev_lines}\n\n"
        f"### Aplicabilidade (Indústria, Uso Doméstico e Servidores) & Pilotos\n"
        f"Chamada Aberta para Pilotos (Indústria Local, Universidades & Setor Público Ético) — validação física vs QEMU (Tabela B):\n"
        f"- Relatório & Telemetria: {link}\n"
        f"- Contato Técnico: {DEFAULT_PORTAL_URL}/#win-contact\n"
        f"- Feed RSS / Google News: {DEFAULT_PORTAL_URL}/feed.xml\n"
    )


def format_microblog(title, version, summary, gain, eco_ids, lab_ids, link, search_keywords=None):
    """Gera texto técnico conciso para Bluesky (<=300 chars), Mastodon e X."""
    lab_short = " / ".join(OBSERVATORY_LABS[l]["title"] for l in lab_ids if l in OBSERVATORY_LABS) or "Observatory"
    eco_short = " · ".join(ECOSYSTEM_DOMAINS[e]["short_label"] for e in eco_ids if e in ECOSYSTEM_DOMAINS)
    t = title[:64].rstrip()
    g = gain.replace("Indicadores citados no relatório: ", "")[:54].rstrip()
    text = (
        f"Jinn [{lab_short[:28]}]: {t}\n\n"
        f"Metricas: {g}\n"
        f"Ecossistemas: {eco_short[:56]}\n"
        f"{link}\n"
        f"#RustLang #OSDev #Microkernel #NoStd"
    )
    if len(text) > 295:
        avail = max(18, len(t) - (len(text) - 295))
        t = t[:avail].rstrip()
        text = (
            f"Jinn [{lab_short[:24]}]: {t}\n\n"
            f"Metricas: {g}\n"
            f"Ecossistemas: {eco_short[:50]}\n"
            f"{link}\n"
            f"#RustLang #OSDev #Microkernel #NoStd"
        )
    return text


def format_discord_embed(
    title,
    version,
    summary,
    gain,
    eco_ids,
    link,
    commit,
    search_keywords=None,
    visual_evidences=None,
):
    """Gera payload JSON para Webhook do Discord com Rich Embed incluindo ganho, keywords e evidências visuais."""
    eco_text = "\n".join(f"• {lbl}" for lbl in format_ecosystem_labels(eco_ids)) or "• Microkernel & Open Hardware"
    kw_text = ", ".join((search_keywords or [])[:8]) or "Jinn OS, Microkernel, Rust no_std"
    ev_text = "\n".join(
        f"• [{ev.get('caption', 'Captura')[:70]}]({ev.get('url', DEFAULT_IMAGE_URL)})"
        for ev in (visual_evidences or [])[:3]
    ) or f"• [Observatory Preview]({DEFAULT_IMAGE_URL})"
    thumb_url = (visual_evidences[0]["url"] if visual_evidences else f"{DEFAULT_PORTAL_URL}/assets/wallpaper.jpg")
    payload = {
        "username": "Jinn Engineering Observatory Bot",
        "avatar_url": f"{DEFAULT_PORTAL_URL}/assets/wallpaper.jpg",
        "embeds": [
            {
                "title": f"Jinn OS {version}: {title}",
                "url": link,
                "description": summary,
                "color": 0x00E5FF,
                "image": {"url": thumb_url},
                "fields": [
                    {
                        "name": "Ganho Técnico Mensurável",
                        "value": gain,
                        "inline": False
                    },
                    {
                        "name": "Ecossistemas & Setores Aplicáveis",
                        "value": eco_text,
                        "inline": False
                    },
                    {
                        "name": "Palavras-Chave Técnicas de Busca",
                        "value": kw_text,
                        "inline": False
                    },
                    {
                        "name": "Evidências Visuais",
                        "value": ev_text,
                        "inline": False
                    },
                    {
                        "name": "Chamada Aberta para Pilotos (Indústria Local, Academia & Setor Público Ético)",
                        "value": f"[Observatory]({DEFAULT_PORTAL_URL}) | [GitHub ({commit})]({DEFAULT_REPO_URL}) | [Contato/Pilotos]({DEFAULT_PORTAL_URL}/#win-contact) | [RSS Feed]({DEFAULT_PORTAL_URL}/feed.xml)",
                        "inline": False
                    }
                ],
                "footer": {
                    "text": "Jinn Engineering Observatory — Research. Engineering. Outreach. Collaboration.",
                    "icon_url": f"{DEFAULT_PORTAL_URL}/assets/wallpaper.jpg"
                },
                "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat()
            }
        ]
    }
    return payload


def format_telegram_message(
    title,
    version,
    summary,
    gain,
    eco_ids,
    link,
    search_keywords=None,
    visual_evidences=None,
):
    """Gera mensagem formatada em HTML para Bot do Telegram com ganhos, keywords e evidência visual."""
    eco_str = html_lib.escape(", ".join(ECOSYSTEM_DOMAINS[e]["short_label"] for e in eco_ids if e in ECOSYSTEM_DOMAINS))
    kw_str = html_lib.escape(", ".join((search_keywords or [])[:6]) or "Jinn OS, Microkernel")
    ev_url = visual_evidences[0]["url"] if visual_evidences else DEFAULT_IMAGE_URL
    text = (
        f"<b>Jinn OS {html_lib.escape(version)}: {html_lib.escape(title)}</b>\n\n"
        f"{html_lib.escape(summary)}\n\n"
        f"<b>Ganho Mensurável:</b> {html_lib.escape(gain)}\n"
        f"<b>Ecossistemas:</b> {eco_str}\n"
        f"<b>Palavras-chave:</b> {kw_str}\n\n"
        f"<a href=\"{DEFAULT_PORTAL_URL}\">Observatory</a> | "
        f"<a href=\"{link}\">Relatório</a> | "
        f"<a href=\"{html_lib.escape(ev_url)}\">Evidência Visual</a> | "
        f"<a href=\"{DEFAULT_PORTAL_URL}/#win-contact\">Pilotos &amp; Colaboração</a>"
    )
    return text


def update_rss_feed(
    feed_path,
    title,
    summary,
    link,
    gain="",
    eco_ids=None,
    lab_ids=None,
    search_keywords=None,
    mention_entities=None,
    visual_evidences=None,
):
    """Insere o novo item no topo do arquivo RSS/Atom (web/feed.xml e web/rss.xml) com tags, keywords e evidências."""
    if not os.path.exists(feed_path):
        print(f"[WARN] Arquivo RSS nao encontrado em: {feed_path}")
        return False

    with open(feed_path, "r", encoding="utf-8") as f:
        content = f.read()

    eco_ids = eco_ids or ["microkernel-security"]
    lab_ids = lab_ids or []
    rich_desc = format_ecosystem_summary_text(
        summary,
        gain or "Evidência empírica reproduzível em QEMU/PCAP",
        eco_ids,
        keywords=search_keywords,
        mentions=mention_entities,
        evidences=visual_evidences,
    )
    cat_vals = list(dict.fromkeys([*eco_ids, *lab_ids]))
    categories_xml = "\n".join(f"      <category>{html_lib.escape(c)}</category>" for c in cat_vals)
    primary_ev_url = visual_ev_url(visual_evidences)
    ev_mime = "image/png" if primary_ev_url.lower().endswith(".png") else "image/jpeg"

    now_rfc822 = datetime.datetime.now(datetime.timezone.utc).strftime("%a, %d %b %Y %H:%M:%S +0000")
    guid = "jinn-os-" + re.sub(r'[^a-zA-Z0-9]', '-', title.lower()).strip('-') + "-" + datetime.datetime.now().strftime("%Y%m%d%H%M")

    new_item = f"""    <item>
      <title>{html_lib.escape(title)}</title>
      <link>{html_lib.escape(link)}</link>
      <guid isPermaLink="false">{guid}</guid>
      <pubDate>{now_rfc822}</pubDate>
{categories_xml}
      <description><![CDATA[
        <p>{rich_desc}</p>
      ]]></description>
      <enclosure url="{html_lib.escape(primary_ev_url)}" length="70707" type="{ev_mime}" />
    </item>
"""

    content = re.sub(r'<lastBuildDate>[^<]+</lastBuildDate>', f'<lastBuildDate>{now_rfc822}</lastBuildDate>', content)

    if "<item>" in content:
        content = content.replace("<item>", new_item + "    <item>", 1)
    else:
        content = content.replace("</channel>", new_item + "  </channel>")

    with open(feed_path, "w", encoding="utf-8") as f:
        f.write(content)

    print(f"  [OK] Feed RSS atualizado com sucesso: {feed_path}")
    return True


def visual_ev_url(visual_evidences):
    """Retorna URL da evidência visual principal para enclosure RSS."""
    if visual_evidences and visual_evidences[0].get("url"):
        return visual_evidences[0]["url"]
    return DEFAULT_IMAGE_URL

def dispatch_discord(webhook_url, payload, dry_run=False):
    """Envia webhook para canal do Discord."""
    if not webhook_url:
        print("  [SKIP] DISCORD_WEBHOOK_URL nao configurada.")
        return
    if dry_run:
        print("  [DRY-RUN] Discord payload preparado (envio ignorado).")
        return

    req = urllib.request.Request(
        webhook_url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json", "User-Agent": "Jinn-Broadcast-Bot/1.0"}
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            print(f"  [PASS] Discord Webhook enviado com sucesso! Status: {resp.status}")
    except Exception as e:
        print(f"  [FAIL] Falha ao enviar Discord Webhook: {e}")

def dispatch_telegram(bot_token, chat_id, text, dry_run=False):
    """Envia mensagem para canal ou grupo do Telegram."""
    if not bot_token or not chat_id:
        print("  [SKIP] TELEGRAM_BOT_TOKEN ou TELEGRAM_CHAT_ID nao configurados.")
        return
    if dry_run:
        print("  [DRY-RUN] Telegram mensagem preparada (envio ignorado).")
        return

    url = f"https://api.telegram.org/bot{bot_token}/sendMessage"
    payload = {
        "chat_id": chat_id,
        "text": text,
        "parse_mode": "HTML",
        "disable_web_page_preview": False
    }
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json"}
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            print(f"  [PASS] Mensagem Telegram enviada com sucesso! Status: {resp.status}")
    except Exception as e:
        print(f"  [FAIL] Falha ao enviar mensagem Telegram: {e}")

def dispatch_mastodon(instance_url, access_token, status_text, dry_run=False):
    """Publica status no Mastodon via ActivityPub REST API."""
    if not instance_url or not access_token:
        print("  [SKIP] MASTODON_INSTANCE_URL ou MASTODON_ACCESS_TOKEN nao configurados.")
        return
    if dry_run:
        print("  [DRY-RUN] Mastodon status preparado (envio ignorado).")
        return

    endpoint = f"{instance_url.rstrip('/')}/api/v1/statuses"
    payload = {"status": status_text, "visibility": "public"}
    req = urllib.request.Request(
        endpoint,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {access_token}",
            "User-Agent": "Jinn-Broadcast-Bot/1.0"
        }
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            print(f"  [PASS] Post no Mastodon publicado com sucesso! Status: {resp.status}")
    except Exception as e:
        print(f"  [FAIL] Falha ao publicar no Mastodon: {e}")

def dispatch_bluesky(handle, app_password, text, dry_run=False):
    """Publica no Bluesky via AT Protocol (com.atproto.repo.createRecord)."""
    if not handle or not app_password:
        print("  [SKIP] BLUESKY_HANDLE ou BLUESKY_APP_PASSWORD nao configurados.")
        return
    if dry_run:
        print("  [DRY-RUN] Bluesky post preparado (envio ignorado).")
        return

    try:
        # 1. Cria sessao AT Protocol
        session_url = "https://bsky.social/xrpc/com.atproto.server.createSession"
        session_payload = {"identifier": handle, "password": app_password}
        req_session = urllib.request.Request(
            session_url,
            data=json.dumps(session_payload).encode("utf-8"),
            headers={"Content-Type": "application/json"}
        )
        with urllib.request.urlopen(req_session, timeout=10) as resp:
            session_data = json.loads(resp.read().decode())
            access_jwt = session_data.get("accessJwt")
            did = session_data.get("did")

        if not access_jwt or not did:
            print("  [FAIL] Nao foi possivel autenticar no Bluesky.")
            return

        # 2. Cria registro de post (app.bsky.feed.post)
        post_url = "https://bsky.social/xrpc/com.atproto.repo.createRecord"
        now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")
        post_payload = {
            "repo": did,
            "collection": "app.bsky.feed.post",
            "record": {
                "$type": "app.bsky.feed.post",
                "text": text,
                "createdAt": now_iso
            }
        }
        req_post = urllib.request.Request(
            post_url,
            data=json.dumps(post_payload).encode("utf-8"),
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {access_jwt}"
            }
        )
        with urllib.request.urlopen(req_post, timeout=10) as resp:
            print(f"  [PASS] Post no Bluesky publicado com sucesso! Status: {resp.status}")

    except Exception as e:
        print(f"  [FAIL] Falha ao publicar no Bluesky: {e}")

def dispatch_websub(feed_urls=None, hub_url="https://pubsubhubbub.appspot.com/", dry_run=False):
    """Notifica hubs WebSub / PubSubHubbub do Google sobre atualizações nos feeds RSS/Atom."""
    if feed_urls is None:
        feed_urls = [f"{DEFAULT_PORTAL_URL}/rss.xml", f"{DEFAULT_PORTAL_URL}/feed.xml"]

    for feed_url in feed_urls:
        if dry_run:
            print(f"  [DRY-RUN] WebSub ping para {feed_url} em {hub_url} preparado.")
            continue
        try:
            import urllib.parse
            payload = urllib.parse.urlencode({"hub.mode": "publish", "hub.url": feed_url}).encode("utf-8")
            req = urllib.request.Request(
                hub_url,
                data=payload,
                headers={
                    "Content-Type": "application/x-www-form-urlencoded",
                    "User-Agent": "Jinn-WebSub-Publisher/1.0"
                }
            )
            with urllib.request.urlopen(req, timeout=10) as resp:
                print(f"  [PASS] WebSub Hub Ping ({feed_url}) -> Status: {resp.status} {resp.reason}")
        except Exception as e:
            print(f"  [FAIL] Falha no WebSub Hub Ping ({feed_url}): {e}")

def main():
    parser = argparse.ArgumentParser(description="Pipeline de Sindicação e Difusão de Engenharia do Jinn Engineering Observatory")
    parser.add_argument("--title", type=str, help="Titulo tecnico do release / devlog")
    parser.add_argument("--version", type=str, help="Versao do release (ex: v0.4.3-alpha)")
    parser.add_argument("--summary", type=str, help="Sumario executivo de engenharia")
    parser.add_argument("--gain", type=str, help="Ganho tecnico mensuravel (ex: 0.00%% CPU idle, 54 KB TLS 1.3)")
    parser.add_argument("--link", type=str, help="URL de destino para o post completo / release")
    parser.add_argument("--commit", type=str, default="HEAD", help="Hash ou referencia do commit a inspecionar")
    parser.add_argument("--update-feed", action="store_true", help="Atualiza web/feed.xml e web/rss.xml com a nova entrada")
    parser.add_argument("--dry-run", action="store_true", help="Gera os formatos e simula o despacho sem efetuar chamadas de rede")

    args = parser.parse_args()
    git_info = get_git_info(args.commit)

    if args.title:
        title = args.title
    elif git_info["message"]:
        raw_msg = git_info["message"]
        cleaned = re.sub(r'^(feat|fix|perf|docs|release|chore|refactor)(\([^)]+\))?:\s*', '', raw_msg, flags=re.I)
        cleaned = re.sub(r'\[announce\]', '', cleaned, flags=re.I).strip()
        title = cleaned[:96] if cleaned else "Hardening de Microkernel e Arquitetura Zero-Idle"
    else:
        title = "Hardening de Microkernel, Pilha TLS 1.3 e Arquitetura Zero-Idle"

    version = args.version or git_info["tag"]
    summary = args.summary or (
        git_info["body"][:260].replace("\n", " ").strip()
        if git_info["body"]
        else "Avanço verificado no Jinn Microkernel, compositor Aladdin e computação heterogênea JHCS com foco em isolamento Ring 3 e previsibilidade de recursos."
    )
    link = args.link or DEFAULT_PORTAL_URL
    body_all = f"{summary} {git_info['body']}"
    eco_ids = classify_ecosystems("RELEASE", [], title.lower(), title, body_all)
    lab_ids = classify_labs("RELEASE", [], title.lower(), title, body_all)
    about_concepts, mention_entities, search_keywords = classify_semantic_metadata(
        "RELEASE", lab_ids, title.lower(), title, body_all, eco_ids
    )
    visual_evidences = discover_visual_evidences(link, eco_ids, lab_ids, title, body_all)
    gain = args.gain or extract_measurable_gain(title, body_all)

    print("================================================================================")
    print("  JINN ENGINEERING OBSERVATORY — SINDICAÇÃO DE ENGENHARIA & OUTREACH")
    print("================================================================================")
    print(f"Título      : {title}")
    print(f"Versão      : {version}")
    print(f"Bancadas    : {', '.join(lab_ids)}")
    print(f"Ecossistemas: {', '.join(eco_ids)}")
    print(f"Keywords    : {', '.join(search_keywords[:8])}")
    print(f"Evidências  : {len(visual_evidences)} arquivo(s)")
    print(f"Ganho       : {gain}")
    print(f"Link        : {link}")
    print(f"Dry-Run     : {args.dry_run}")
    print("================================================================================\n")

    # 1. Gerar Formatos
    microblog_text = format_microblog(title, version, summary, gain, eco_ids, lab_ids, link, search_keywords=search_keywords)
    discord_payload = format_discord_embed(
        title, version, summary, gain, eco_ids, link, git_info["commit"],
        search_keywords=search_keywords, visual_evidences=visual_evidences,
    )
    telegram_text = format_telegram_message(
        title, version, summary, gain, eco_ids, link,
        search_keywords=search_keywords, visual_evidences=visual_evidences,
    )
    release_notes = format_release_notes(
        title, version, summary, gain, eco_ids, lab_ids, link, git_info["commit"],
        about_concepts=about_concepts, mention_entities=mention_entities,
        search_keywords=search_keywords, visual_evidences=visual_evidences,
    )

    print("--- Formato 1: Microblogging (Bluesky / Mastodon / X) ---")
    print(microblog_text)
    print("\n--- Formato 2: Comunidades (Telegram HTML Preview) ---")
    print(telegram_text)
    print("\n--- Formato 3: Template GitHub Release Notes ---")
    print(release_notes)
    print("\n--------------------------------------------------------------------------------")

    # 2. Atualizar Feed RSS se solicitado
    if args.update_feed:
        print("\n[+] Atualizando Feeds RSS estáticos...")
        for fp in ("web/feed.xml", "web/rss.xml"):
            update_rss_feed(
                fp, title, summary, link, gain=gain, eco_ids=eco_ids, lab_ids=lab_ids,
                search_keywords=search_keywords, mention_entities=mention_entities,
                visual_evidences=visual_evidences,
            )

    # 3. Despacho Multicanal
    print("\n[+] Executando difusão nos canais configurados...")
    dispatch_websub(dry_run=args.dry_run)
    dispatch_discord(os.environ.get("DISCORD_WEBHOOK_URL"), discord_payload, args.dry_run)
    dispatch_telegram(os.environ.get("TELEGRAM_BOT_TOKEN"), os.environ.get("TELEGRAM_CHAT_ID"), telegram_text, args.dry_run)
    dispatch_mastodon(os.environ.get("MASTODON_INSTANCE_URL"), os.environ.get("MASTODON_ACCESS_TOKEN"), microblog_text, args.dry_run)
    dispatch_bluesky(os.environ.get("BLUESKY_HANDLE"), os.environ.get("BLUESKY_APP_PASSWORD"), microblog_text, args.dry_run)

    print("\n[SUCCESS] Pipeline de sindicacao concluido com sucesso!")

if __name__ == "__main__":
    main()
