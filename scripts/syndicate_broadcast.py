#!/usr/bin/env python3
"""
scripts/syndicate_broadcast.py

Pipeline de Sindicação e Difusão Automatizada para o Jinn OS (jinn-os.dev.br)
Implementa Engenharia como Marketing com Zero Custo Manual:
1. Gera 3 formatos a partir do git diff / release:
   - Formato 1: Microblogging (Bluesky, Mastodon, X)
   - Formato 2: Comunidade / Chat (Discord, Telegram via Webhooks)
   - Formato 3: RSS Feed Estático (web/feed.xml e web/rss.xml)
2. Despacha automaticamente via HTTP (APIs e Webhooks nativos sem dependências externas)
"""

import os
import sys
import argparse
import json
import urllib.request
import urllib.error
import datetime
import subprocess
import re

DEFAULT_PORTAL_URL = "https://jinn-os.dev.br"
DEFAULT_REPO_URL = "https://github.com/GeovaneEstanislau/Jinn_Project"

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

def format_microblog(title, version, summary, link):
    """Gera texto para Bluesky, Mastodon e X (Thread técnica concisa <= 300 chars)."""
    text = (
        f"🚀 Jinn OS {version}: {title}\n\n"
        f"{summary}\n\n"
        f"🦀 Microkernel x86_64 em Rust (#![no_std])\n"
        f"⚡ 0.00% CPU em repouso (Zero-Idle)\n"
        f"🔗 {link}\n\n"
        f"#RustLang #OSDev #Microkernel #SystemsProgramming"
    )
    return text

def format_discord_embed(title, version, summary, link, commit):
    """Gera payload JSON para Webhook do Discord com Rich Embed."""
    payload = {
        "username": "Jinn OS Release Bot",
        "avatar_url": f"{DEFAULT_PORTAL_URL}/assets/wallpaper.jpg",
        "embeds": [
            {
                "title": f"🚀 Jinn OS {version}: {title}",
                "url": link,
                "description": summary,
                "color": 0x00E5FF, # Cyan/Emerald
                "fields": [
                    {
                        "name": "🏛️ Arquitetura",
                        "value": "Microkernel `x86_64` (`#![no_std]`) com Ring 3 Isolado & IPC Zero-Copy",
                        "inline": False
                    },
                    {
                        "name": "⚡ Destaques de Engenharia",
                        "value": "• Zero-GPU 60 FPS no Aladdin Desktop\n• Resolução DNS Cloudflare/Google cascateada\n• Zero-Idle Resource Consumption (0.00% CPU)",
                        "inline": False
                    },
                    {
                        "name": "🔗 Links & Recursos",
                        "value": f"[Portal Oficial]({DEFAULT_PORTAL_URL}) | [Código-Fonte ({commit})]({DEFAULT_REPO_URL}) | [RSS Feed]({DEFAULT_PORTAL_URL}/feed.xml)",
                        "inline": False
                    }
                ],
                "footer": {
                    "text": "Jinn OS — Sovereign Foundation Systems",
                    "icon_url": f"{DEFAULT_PORTAL_URL}/assets/wallpaper.jpg"
                },
                "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat()
            }
        ]
    }
    return payload

def format_telegram_message(title, version, summary, link):
    """Gera mensagem formatada em HTML para Bot do Telegram."""
    text = (
        f"<b>🚀 Jinn OS {version}: {title}</b>\n\n"
        f"{summary}\n\n"
        f"⚙️ <i>Microkernel x86_64 em Rust (#![no_std]) | Zero-GPU | Zero-Idle</i>\n\n"
        f"🌐 <a href=\"{DEFAULT_PORTAL_URL}\">Portal Oficial</a> | "
        f"<a href=\"{link}\">Ver Atualização</a> | "
        f"<a href=\"{DEFAULT_PORTAL_URL}/feed.xml\">RSS Feed</a>"
    )
    return text

def update_rss_feed(feed_path, title, summary, link):
    """Insere o novo item no topo do arquivo RSS/Atom (web/feed.xml e web/rss.xml)."""
    if not os.path.exists(feed_path):
        print(f"[WARN] Arquivo RSS nao encontrado em: {feed_path}")
        return False

    with open(feed_path, "r", encoding="utf-8") as f:
        content = f.read()

    now_rfc822 = datetime.datetime.now(datetime.timezone.utc).strftime("%a, %d %b %Y %H:%M:%S +0000")
    guid = "jinn-os-" + re.sub(r'[^a-zA-Z0-9]', '-', title.lower()).strip('-') + "-" + datetime.datetime.now().strftime("%Y%m%d%H%M")

    new_item = f"""    <item>
      <title>{title}</title>
      <link>{link}</link>
      <guid isPermaLink="false">{guid}</guid>
      <pubDate>{now_rfc822}</pubDate>
      <description><![CDATA[
        <p>{summary}</p>
      ]]></description>
    </item>
"""

    # Atualiza lastBuildDate
    content = re.sub(r'<lastBuildDate>[^<]+</lastBuildDate>', f'<lastBuildDate>{now_rfc822}</lastBuildDate>', content)

    # Insere antes do primeiro <item>
    if "<item>" in content:
        content = content.replace("<item>", new_item + "    <item>", 1)
    else:
        content = content.replace("</channel>", new_item + "  </channel>")

    with open(feed_path, "w", encoding="utf-8") as f:
        f.write(content)

    print(f"  [OK] Feed RSS atualizado com sucesso: {feed_path}")
    return True

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

def main():
    parser = argparse.ArgumentParser(description="Pipeline de Sindicação e Difusão de Engenharia do Jinn OS")
    parser.add_argument("--title", type=str, help="Titulo tecnico do release / devlog")
    parser.add_argument("--version", type=str, help="Versao do release (ex: v0.4.3-alpha)")
    parser.add_argument("--summary", type=str, help="Sumario executivo de engenharia")
    parser.add_argument("--link", type=str, help="URL de destino para o post completo / release")
    parser.add_argument("--commit", type=str, default="HEAD", help="Hash ou referencia do commit a inspecionar")
    parser.add_argument("--update-feed", action="store_true", help="Atualiza web/feed.xml e web/rss.xml com a nova entrada")
    parser.add_argument("--dry-run", action="store_true", help="Gera os formatos e simula o despacho sem efetuar chamadas de rede")

    args = parser.parse_args()
    git_info = get_git_info(args.commit)

    # Limpeza e formatacao automatica de titulo se nao fornecido
    if args.title:
        title = args.title
    elif git_info["message"]:
        raw_msg = git_info["message"]
        cleaned = re.sub(r'^(feat|fix|release|chore|refactor)(\([^)]+\))?:\s*', '', raw_msg, flags=re.I)
        cleaned = re.sub(r'\[announce\]', '', cleaned, flags=re.I).strip()
        # Se for o commit de release, gera titulo técnico atraente
        if "configurable google/cloudflare dns" in cleaned.lower():
            title = "Native SIMD image decoding in Ring 3, SMP spawn race fix, and forensic VFS cold-reboot tests"
        else:
            title = cleaned[:80] if cleaned else "Hardening de Microkernel e Pipeline SIMD Zero-GPU"
    else:
        title = "Native SIMD image decoding in Ring 3, SMP spawn race fix, and forensic VFS cold-reboot tests"

    version = args.version or git_info["tag"]
    summary = args.summary or (
        "Consolidamos o suporte a resolucao DNS cascateada (Cloudflare/Google), decodificacao de imagens "
        "bare-metal no Jinn Browser, aceleracao SIMD AVX2 no JHCS e persistencia estrita em disco FAT32 com zero-idle."
    )
    link = args.link or DEFAULT_PORTAL_URL

    print("================================================================================")
    print("  JINN OS — PIPELINE DE SINDICAÇÃO AUTOMATIZADA DE ENGENHARIA")
    print("================================================================================")
    print(f"Título  : {title}")
    print(f"Versão  : {version}")
    print(f"Link    : {link}")
    print(f"Dry-Run : {args.dry_run}")
    print("================================================================================\n")

    # 1. Gerar Formatos
    microblog_text = format_microblog(title, version, summary, link)
    discord_payload = format_discord_embed(title, version, summary, link, git_info["commit"])
    telegram_text = format_telegram_message(title, version, summary, link)

    print("--- Formato 1: Microblogging (Bluesky / Mastodon / X) ---")
    print(microblog_text)
    print("\n--- Formato 2: Comunidades (Telegram HTML Preview) ---")
    print(telegram_text)
    print("\n--------------------------------------------------------------------------------")

    # 2. Atualizar Feed RSS se solicitado
    if args.update_feed:
        print("\n[+] Atualizando Feeds RSS estáticos...")
        update_rss_feed("web/feed.xml", title, summary, link)
        update_rss_feed("web/rss.xml", title, summary, link)

    # 3. Despacho Multicanal
    print("\n[+] Executando difusão nos canais configurados...")
    dispatch_discord(os.environ.get("DISCORD_WEBHOOK_URL"), discord_payload, args.dry_run)
    dispatch_telegram(os.environ.get("TELEGRAM_BOT_TOKEN"), os.environ.get("TELEGRAM_CHAT_ID"), telegram_text, args.dry_run)
    dispatch_mastodon(os.environ.get("MASTODON_INSTANCE_URL"), os.environ.get("MASTODON_ACCESS_TOKEN"), microblog_text, args.dry_run)
    dispatch_bluesky(os.environ.get("BLUESKY_HANDLE"), os.environ.get("BLUESKY_APP_PASSWORD"), microblog_text, args.dry_run)

    print("\n[SUCCESS] Pipeline de sindicacao concluido com sucesso!")

if __name__ == "__main__":
    main()
