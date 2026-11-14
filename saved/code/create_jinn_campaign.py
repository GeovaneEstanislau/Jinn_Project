#!/usr/bin/env python3
"""
create_jinn_campaign.py — Gerador Atômico da Campanha "Jinn OS - Microkernel Rust & OS Leve"
Google Ads API (v17/v18)

Cria de forma 100% atômica (via MutateOperations):
1. Orçamento diário exclusivo de R$ 1,64 (1.640.000 micros).
2. Campanha de Rede de Pesquisa estrita (target_google_search=True, display=False, parceiros=False).
3. Estratégia Maximize Clicks com teto rígido de CPC de R$ 0,50 (500.000 micros).
4. Segmentação geográfica (Brasil - 2076) e idioma (Português - 1014).
5. Grupo de anúncios com palavras-chave estritas (EXACT e PHRASE - zero BROAD match).
6. Lista robusta de palavras-chave negativas a nível de campanha.
7. Anúncio Responsivo de Pesquisa (RSA) técnico e otimizado para desenvolvedores.

Suporta flag --validate-only para dry-run sem gerar cobrança ou mutação real.
"""

import argparse
import sys
from typing import List
from google.ads.googleads.client import GoogleAdsClient
from google.ads.googleads.errors import GoogleAdsException

# ── Configurações e Parâmetros de Negócio ──────────────────────────────────────
DEFAULT_CUSTOMER_ID = "2940714035"  # Conta de destino Jinn OS

BUDGET_MICROS = 1_640_000           # R$ 1,64 / dia (R$ 50,00 / mês)
CPC_BID_CEILING_MICROS = 500_000    # Teto máximo de CPC: R$ 0,50 por clique
FINAL_URL = "https://jinn-os.dev.br/"

GEO_BRAZIL = "geoTargetConstants/2076"
LANG_PORTUGUESE = "languageConstants/1014"

# Resource Names temporários para criação em lote atômico
BUDGET_TEMP_ID = -1
CAMPAIGN_TEMP_ID = -2
AD_GROUP_TEMP_ID = -3


def build_mutate_operations(client: GoogleAdsClient, customer_id: str) -> List:
    """Monta a lista ordenada de MutateOperations para criação em lote."""
    operations = []

    # 1. Orçamento Diário Exclusivo (CampaignBudget)
    budget_service = client.get_service("CampaignBudgetService")
    budget_op = client.get_type("MutateOperation")
    budget = budget_op.campaign_budget_operation.create
    budget.resource_name = budget_service.campaign_budget_path(customer_id, str(BUDGET_TEMP_ID))
    budget.name = f"Orçamento Jinn OS R$1,64/dia #{customer_id}"
    budget.amount_micros = BUDGET_MICROS
    budget.delivery_method = client.enums.BudgetDeliveryMethodEnum.STANDARD
    budget.explicitly_shared = False
    operations.append(budget_op)

    # 2. Campanha de Rede de Pesquisa Estrita
    campaign_service = client.get_service("CampaignService")
    campaign_op = client.get_type("MutateOperation")
    campaign = campaign_op.campaign_operation.create
    campaign.resource_name = campaign_service.campaign_path(customer_id, str(CAMPAIGN_TEMP_ID))
    campaign.name = "Jinn OS - Microkernel Rust & OS Leve"
    # Inicia PAUSADA por padrão para revisão antes do envio de tráfego
    campaign.status = client.enums.CampaignStatusEnum.PAUSED
    campaign.advertising_channel_type = client.enums.AdvertisingChannelTypeEnum.SEARCH
    campaign.campaign_budget = budget.resource_name

    # Desativação obrigatória de redes de desperdício
    campaign.network_settings.target_google_search = True
    campaign.network_settings.target_search_network = False
    campaign.network_settings.target_content_network = False
    campaign.network_settings.target_partner_search_network = False

    # Estratégia de Lance: Maximizar Cliques com Teto Rígido de CPC de R$ 0,50
    campaign.target_spend.cpc_bid_ceiling_micros = CPC_BID_CEILING_MICROS
    operations.append(campaign_op)

    # 3. Critérios de Segmentação da Campanha (Brasil + Português)
    # Geo: Brasil
    geo_op = client.get_type("MutateOperation")
    geo_criterion = geo_op.campaign_criterion_operation.create
    geo_criterion.campaign = campaign.resource_name
    geo_criterion.location.geo_target_constant = GEO_BRAZIL
    operations.append(geo_op)

    # Idioma: Português
    lang_op = client.get_type("MutateOperation")
    lang_criterion = lang_op.campaign_criterion_operation.create
    lang_criterion.campaign = campaign.resource_name
    lang_criterion.language.language_constant = LANG_PORTUGUESE
    operations.append(lang_op)

    # 4. Palavras-Chave Negativas a Nível de Campanha (Prevenção de Tráfego Irrelevante)
    negative_keywords = [
        "aladdin filme",
        "disney",
        "gênio da lâmpada",
        "jogo",
        "download grátis",
        "crack",
        "windows 11 crack",
        "curso",
        "desenho animado",
        "trilha sonora",
    ]
    for neg_kw in negative_keywords:
        neg_op = client.get_type("MutateOperation")
        neg_crit = neg_op.campaign_criterion_operation.create
        neg_crit.campaign = campaign.resource_name
        neg_crit.negative = True
        neg_crit.keyword.text = neg_kw
        neg_crit.keyword.match_type = client.enums.KeywordMatchTypeEnum.PHRASE
        operations.append(neg_op)

    # 5. Grupo de Anúncios (AdGroup)
    ad_group_service = client.get_service("AdGroupService")
    ad_group_op = client.get_type("MutateOperation")
    ad_group = ad_group_op.ad_group_operation.create
    ad_group.resource_name = ad_group_service.ad_group_path(customer_id, str(AD_GROUP_TEMP_ID))
    ad_group.name = "Desenvolvedores e Sistemas Leves"
    ad_group.campaign = campaign.resource_name
    ad_group.status = client.enums.AdGroupStatusEnum.ENABLED
    ad_group.type_ = client.enums.AdGroupTypeEnum.SEARCH_STANDARD
    operations.append(ad_group_op)

    # 6. Palavras-Chave Positivas (Strict Match: Apenas EXACT e PHRASE)
    positive_keywords = [
        # Correspondência Exata (Foco Cirúrgico)
        ("jinn os", client.enums.KeywordMatchTypeEnum.EXACT),
        ("microkernel rust", client.enums.KeywordMatchTypeEnum.EXACT),
        ("sistema operacional rust", client.enums.KeywordMatchTypeEnum.EXACT),
        # Correspondência de Frase (Intenção Qualificada)
        ("sistema operacional leve", client.enums.KeywordMatchTypeEnum.PHRASE),
        ("sistema operacional em rust", client.enums.KeywordMatchTypeEnum.PHRASE),
        ("alternativa linux leve", client.enums.KeywordMatchTypeEnum.PHRASE),
        ("sistema operacional para pc antigo", client.enums.KeywordMatchTypeEnum.PHRASE),
        ("distribuicao linux leve", client.enums.KeywordMatchTypeEnum.PHRASE),
    ]

    for kw_text, match_type in positive_keywords:
        kw_op = client.get_type("MutateOperation")
        kw_crit = kw_op.ad_group_criterion_operation.create
        kw_crit.ad_group = ad_group.resource_name
        kw_crit.status = client.enums.AdGroupCriterionStatusEnum.ENABLED
        kw_crit.keyword.text = kw_text
        kw_crit.keyword.match_type = match_type
        operations.append(kw_op)

    # 7. Anúncio Responsivo de Pesquisa (Responsive Search Ad - RSA)
    ad_op = client.get_type("MutateOperation")
    ad_group_ad = ad_op.ad_group_ad_operation.create
    ad_group_ad.ad_group = ad_group.resource_name
    ad_group_ad.status = client.enums.AdGroupAdStatusEnum.ENABLED

    rsa = ad_group_ad.ad
    rsa.final_urls.append(FINAL_URL)

    # Títulos (máx 30 caracteres cada)
    headlines = [
        "Jinn OS — Microkernel em Rust",   # 29 chars
        "Sistema Operacional Leve",        # 24 chars
        "Código Aberto & Baixa RAM",       # 26 chars
        "Alternativa Rápida para PC",      # 27 chars
        "Ambiente Aladdin Desktop",        # 24 chars
    ]
    for text in headlines:
        headline = client.get_type("AdTextAsset")
        headline.text = text
        rsa.responsive_search_ad.headlines.append(headline)

    # Descrições (máx 90 caracteres cada)
    descriptions = [
        "Microkernel modular moderno em Rust no_std. Rápido, seguro e ultra leve para qualquer PC.", # 89 chars
        "Conheça o Jinn OS com desktop Aladdin. Código aberto com consumo menor que 256MB de RAM.",    # 89 chars
        "Sistema operacional em Rust sem sobrecarga. Arquitetura Ring 0/Ring 3 modular e rápida.",   # 88 chars
    ]
    for text in descriptions:
        desc = client.get_type("AdTextAsset")
        desc.text = text
        rsa.responsive_search_ad.descriptions.append(desc)

    operations.append(ad_op)

    return operations


def create_campaign(client: GoogleAdsClient, customer_id: str, validate_only: bool = False):
    """Executa a mutação atômica no Google Ads API."""
    google_ads_service = client.get_service("GoogleAdsService")
    operations = build_mutate_operations(client, customer_id)

    mode_label = "DRY-RUN (VALIDATE ONLY)" if validate_only else "PRODUÇÃO (MUTATE REAL)"
    print(f"[*] Iniciando criação atômica da campanha Jinn OS...")
    print(f"[*] Modo: {mode_label}")
    print(f"[*] Conta Cliente: {customer_id}")
    print(f"[*] Total de Operações em Lote: {len(operations)}")

    try:
        response = google_ads_service.mutate(
            customer_id=customer_id,
            mutate_operations=operations,
            validate_only=validate_only,
        )

        print("\n[+] Operação concluída com SUCESSO!")
        if validate_only:
            print("    [PASS] Todos os esquemas, limites de caracteres e restrições foram validados pela API sem erros.")
            print("    [INFO] Nenhuma entidade foi persistida nem cobrança gerada na conta.")
        else:
            for result in response.mutate_operation_responses:
                # Exibe resource names gerados
                if result.campaign_result.resource_name:
                    print(f"    - Campanha Criada: {result.campaign_result.resource_name}")
                elif result.campaign_budget_result.resource_name:
                    print(f"    - Orçamento Criado: {result.campaign_budget_result.resource_name}")
                elif result.ad_group_result.resource_name:
                    print(f"    - Grupo de Anúncios Criado: {result.ad_group_result.resource_name}")

    except GoogleAdsException as ex:
        print(f"\n[!] Falha na API do Google Ads (Código: {ex.error.code().name}):")
        for error in ex.failure.errors:
            print(f"    - Erro: {error.message}")
            if error.location:
                for field_path in error.location.field_path_elements:
                    print(f"      Campo afetado: {field_path.field_name} (índice: {field_path.index})")
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(description="Criador Automatizado da Campanha Jinn OS no Google Ads")
    parser.add_argument("--customer-id", default=DEFAULT_CUSTOMER_ID, help="ID da conta de cliente (sem traços)")
    parser.add_argument("--validate-only", action="store_true", help="Valida a mutação sem persistir alterações (dry-run)")
    args = parser.parse_args()

    # Inicializa cliente a partir do google-ads.yaml ou variáveis de ambiente padrão
    client = GoogleAdsClient.load_from_storage()
    create_campaign(client, args.customer_id, validate_only=args.validate_only)


if __name__ == "__main__":
    main()
