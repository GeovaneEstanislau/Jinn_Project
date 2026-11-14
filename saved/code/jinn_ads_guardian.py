#!/usr/bin/env python3
"""
jinn_ads_guardian.py — Daemon Guardião de Orçamento e Negativação Automática
Jinn OS — Google Ads API (v17/v18)

Executa rotinas de proteção financeira a cada execução (agendado via cron/Task Scheduler):
1. Kill-Switch de Gasto Diário:
   - Monitora `metrics.cost_micros` no dia atual (DURING TODAY).
   - Se custo >= R$ 1,70 (1.700.000 micros) ou cliques >= 15 sem conversão/interação qualificada,
     pausa a campanha imediatamente para evitar cobranças indevidas.
2. Saneamento Automático de Termos de Pesquisa (search_term_view):
   - Avalia termos que geraram cliques (> 0) ou impressões significativas (>= 10).
   - Se o termo NÃO contiver nenhuma das raízes técnicas permitidas, adiciona-o
     imediatamente como palavra-chave negativa de correspondência EXATA na campanha.
3. Auditoria persistente em `saved/code/guardian_audit.log`.
"""

import argparse
import datetime
import os
import sys
from typing import Set, List
from google.ads.googleads.client import GoogleAdsClient
from google.ads.googleads.errors import GoogleAdsException

DEFAULT_CUSTOMER_ID = "2940714035"
CAMPAIGN_NAME_IDENTIFIER = "Jinn OS - Microkernel Rust & OS Leve"

# Limites rígidos de segurança financeira
MAX_DAILY_COST_MICROS = 1_700_000   # R$ 1,70 (Teto diário absoluto)
MAX_CLICKS_WITHOUT_ENGAGEMENT = 15  # Cliques sem conversão

# Raízes semânticas técnicas autorizadas (Whitelist de Intenção)
ALLOWED_TECHNICAL_ROOTS = [
    "jinn",
    "os",
    "sistema",
    "operacional",
    "kernel",
    "microkernel",
    "rust",
    "linux",
    "leve",
    "pc",
    "computador",
    "aladdin",
]

LOG_FILE_PATH = os.path.join(os.path.dirname(__file__), "guardian_audit.log")


def log_event(message: str):
    """Grava logs com timestamp no console e no arquivo de auditoria."""
    timestamp = datetime.datetime.now(datetime.timezone.utc).isoformat()
    entry = f"[{timestamp}] {message}"
    print(entry)
    try:
        with open(LOG_FILE_PATH, "a", encoding="utf-8") as f:
            f.write(entry + "\n")
    except Exception as e:
        print(f"[!] Falha ao gravar log no disco: {e}", file=sys.stderr)


def get_jinn_campaign_id(client: GoogleAdsClient, customer_id: str) -> str:
    """Busca o ID da campanha Jinn OS ativa ou pausada na conta."""
    ga_service = client.get_service("GoogleAdsService")
    query = f"""
        SELECT
            campaign.id,
            campaign.name,
            campaign.status
        FROM campaign
        WHERE campaign.name = '{CAMPAIGN_NAME_IDENTIFIER}'
          AND campaign.status != 'REMOVED'
        LIMIT 1
    """
    response = ga_service.search(customer_id=customer_id, query=query)
    for row in response:
        return str(row.campaign.id)
    return ""


def enforce_budget_killswitch(client: GoogleAdsClient, customer_id: str, campaign_id: str, dry_run: bool = False):
    """Monitora o gasto do dia e aciona o Kill-Switch se o teto for ultrapassado."""
    ga_service = client.get_service("GoogleAdsService")
    query = f"""
        SELECT
            campaign.id,
            campaign.name,
            campaign.status,
            metrics.cost_micros,
            metrics.clicks,
            metrics.impressions,
            metrics.interactions
        FROM campaign
        WHERE campaign.id = {campaign_id}
          AND segments.date DURING TODAY
    """
    response = ga_service.search(customer_id=customer_id, query=query)
    
    for row in response:
        cost_micros = row.metrics.cost_micros
        clicks = row.metrics.clicks
        status = row.campaign.status.name
        cost_brl = cost_micros / 1_000_000

        log_event(f"Métricas Hoje: Custo=R$ {cost_brl:.2f} ({cost_micros} micros), Cliques={clicks}, Status={status}")

        if status != "ENABLED":
            log_event(f"Campanha já está em status {status}. Nenhuma ação de Kill-Switch necessária.")
            return

        should_pause = False
        reason = ""

        if cost_micros >= MAX_DAILY_COST_MICROS:
            should_pause = True
            reason = f"Custo diário R$ {cost_brl:.2f} atingiu ou superou o teto de R$ {MAX_DAILY_COST_MICROS / 1e6:.2f}"
        elif clicks >= MAX_CLICKS_WITHOUT_ENGAGEMENT and row.metrics.interactions == 0:
            should_pause = True
            reason = f"Volume de {clicks} cliques sem engajamento detectado"

        if should_pause:
            log_event(f"[KILL-SWITCH ATIVADO] Motivo: {reason}. Pausando campanha imediatamente...")
            if not dry_run:
                campaign_service = client.get_service("CampaignService")
                campaign_operation = client.get_type("CampaignOperation")
                updated_campaign = campaign_operation.update
                updated_campaign.resource_name = campaign_service.campaign_path(customer_id, campaign_id)
                updated_campaign.status = client.enums.CampaignStatusEnum.PAUSED
                client.copy_from(
                    campaign_operation.update_mask,
                    protobuf_field_mask(None, updated_campaign._pb),
                )
                campaign_service.mutate_campaigns(customer_id=customer_id, operations=[campaign_operation])
                log_event(f"[KILL-SWITCH EXECUTADO] Campanha {campaign_id} pausada com sucesso via API.")
            else:
                log_event(f"[DRY-RUN] Mutação de pausa simulada com sucesso.")


def protobuf_field_mask(dummy, pb_obj):
    """Gera FieldMask para atualizações parciais."""
    from google.protobuf import field_mask_pb2
    return field_mask_pb2.FieldMask(paths=["status"])


def get_existing_campaign_negatives(client: GoogleAdsClient, customer_id: str, campaign_id: str) -> Set[str]:
    """Obtém conjunto de palavras-chave negativas já registradas para evitar duplicatas."""
    ga_service = client.get_service("GoogleAdsService")
    query = f"""
        SELECT
            campaign_criterion.keyword.text,
            campaign_criterion.negative
        FROM campaign_criterion
        WHERE campaign.id = {campaign_id}
          AND campaign_criterion.type = 'KEYWORD'
          AND campaign_criterion.negative = TRUE
    """
    negatives = set()
    response = ga_service.search(customer_id=customer_id, query=query)
    for row in response:
        negatives.add(row.campaign_criterion.keyword.text.lower().strip())
    return negatives


def audit_and_negate_search_terms(client: GoogleAdsClient, customer_id: str, campaign_id: str, dry_run: bool = False):
    """Consulta search_term_view e negativa termos fora do escopo técnico."""
    ga_service = client.get_service("GoogleAdsService")
    query = f"""
        SELECT
            search_term_view.search_term,
            search_term_view.status,
            metrics.clicks,
            metrics.impressions,
            metrics.cost_micros
        FROM search_term_view
        WHERE campaign.id = {campaign_id}
          AND segments.date DURING LAST_7_DAYS
    """
    
    existing_negatives = get_existing_campaign_negatives(client, customer_id, campaign_id)
    terms_to_negate: List[str] = []

    try:
        response = ga_service.search(customer_id=customer_id, query=query)
        for row in response:
            term = row.search_term_view.search_term.lower().strip()
            clicks = row.metrics.clicks
            impressions = row.metrics.impressions

            # Critério de auditoria: gerou clique ou mais de 10 impressões
            if clicks > 0 or impressions >= 10:
                # Checa se possui qualquer raiz técnica autorizada
                has_allowed_root = any(root in term for root in ALLOWED_TECHNICAL_ROOTS)
                if not has_allowed_root:
                    if term not in existing_negatives and term not in terms_to_negate:
                        terms_to_negate.append(term)
                        log_event(f"[TERMO IRRELEVANTE DETECTADO] '{term}' (Cliques: {clicks}, Impr: {impressions})")

    except GoogleAdsException as e:
        log_event(f"[AVISO] Não foi possível consultar search_term_view (campanha sem tráfego recente): {e.error.code().name}")
        return

    if not terms_to_negate:
        log_event("[AUDITORIA DE TERMOS] Nenhum termo de busca fora de escopo pendente de negativação.")
        return

    log_event(f"Preparando negativação de {len(terms_to_negate)} termos...")
    campaign_service = client.get_service("CampaignService")
    campaign_path = campaign_service.campaign_path(customer_id, campaign_id)

    operations = []
    for term in terms_to_negate:
        op = client.get_type("CampaignCriterionOperation")
        crit = op.create
        crit.campaign = campaign_path
        crit.negative = True
        crit.keyword.text = term
        crit.keyword.match_type = client.enums.KeywordMatchTypeEnum.EXACT
        operations.append(op)

    if not dry_run:
        campaign_criterion_service = client.get_service("CampaignCriterionService")
        response = campaign_criterion_service.mutate_campaign_criteria(
            customer_id=customer_id,
            operations=operations,
        )
        log_event(f"[NEGATIVAÇÃO CONCLUÍDA] {len(response.results)} termos negativados com sucesso na campanha.")
    else:
        log_event(f"[DRY-RUN] Negativação simulada de {len(operations)} termos.")


def main():
    parser = argparse.ArgumentParser(description="Daemon Guardião Financeiro e Auditor do Jinn OS")
    parser.add_argument("--customer-id", default=DEFAULT_CUSTOMER_ID, help="ID da conta cliente")
    parser.add_argument("--dry-run", action="store_true", help="Simula verificações sem mutações na API")
    args = parser.parse_args()

    log_event("=== Iniciando Ciclo de Proteção do Jinn Ads Guardian ===")
    try:
        client = GoogleAdsClient.load_from_storage()
        campaign_id = get_jinn_campaign_id(client, args.customer_id)

        if not campaign_id:
            log_event(f"[AVISO] Campanha '{CAMPAIGN_NAME_IDENTIFIER}' ainda não encontrada na conta {args.customer_id}.")
            log_event("Certifique-se de executar 'create_jinn_campaign.py' primeiro.")
            sys.exit(0)

        log_event(f"Campanha Jinn OS localizada (ID: {campaign_id}).")
        
        # 1. Kill-Switch de Gasto Diário
        enforce_budget_killswitch(client, args.customer_id, campaign_id, dry_run=args.dry_run)
        
        # 2. Saneamento de Termos de Busca
        audit_and_negate_search_terms(client, args.customer_id, campaign_id, dry_run=args.dry_run)

        log_event("=== Ciclo de Proteção Concluído com Sucesso ===\n")

    except GoogleAdsException as ex:
        log_event(f"[ERRO CRÍTICO GOOGLE ADS API]: {ex.error.code().name}")
        for error in ex.failure.errors:
            log_event(f"    - Detalhe: {error.message}")
        sys.exit(1)
    except Exception as ex:
        log_event(f"[ERRO INESPERADO]: {ex}")
        sys.exit(1)


if __name__ == "__main__":
    main()
