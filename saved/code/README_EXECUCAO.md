# 🛡️ Guia de Execução e Operação — Automação Google Ads (Jinn OS)

> **Diretório dos Scripts:** `D:\Projetos\Jinn\saved\code`  
> **Conta de Destino:** `294-071-4035` (`2940714035`)  
> **Orçamento:** R$ 1,64 / dia (R$ 50,00 / mês) | **Teto de CPC:** R$ 0,50  
> **Campanha:** `Jinn OS - Microkernel Rust & OS Leve`

---

## 1. Localização e Ambiente Configurado

O ambiente virtual Python com o pacote oficial `google-ads` **já está criado e pronto para uso** em:
`D:\Projetos\Jinn\saved\code\venv`

No PowerShell, certifique-se de navegar até o diretório do projeto:

```powershell
Set-Location "D:\Projetos\Jinn\saved\code"
```

---

## 2. Configuração de Credenciais (`google-ads.yaml`)

Crie o arquivo `google-ads.yaml` dentro de `D:\Projetos\Jinn\saved\code\` (ou em `C:\Users\Geovane\google-ads.yaml`):

```yaml
# google-ads.yaml
developer_token: "SEU_DEVELOPER_TOKEN"
client_id: "SEU_CLIENT_ID.apps.googleusercontent.com"
client_secret: "SEU_CLIENT_SECRET"
refresh_token: "SEU_REFRESH_TOKEN"
login_customer_id: "ID_MCC_SE_APLICAVEL" # Preencher apenas se usar conta MCC (sem traços)
use_proto_plus: true
```

---

## 3. Passo 1 — Diagnóstico da Conta (Query GAQL Somente Leitura)

Para verificar o status das campanhas existentes na conta `2940714035` e confirmar o isolamento da campanha legada ("TecnoVó"):

```sql
SELECT
  campaign.id,
  campaign.name,
  campaign.status,
  campaign.advertising_channel_type,
  campaign_budget.amount_micros,
  campaign.network_settings.target_google_search,
  campaign.network_settings.target_search_network,
  campaign.network_settings.target_content_network,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros
FROM campaign
WHERE campaign.status != 'REMOVED'
ORDER BY campaign.id ASC
```

---

## 4. Passo 2 — Criação Atômica da Campanha Jinn OS

### Modo de Teste / Dry-Run (`--validate-only`)
Valida a conformidade de todas as entidades (Orçamento, Campanha, Grupo, Critérios, Negativas e Anúncio RSA) contra a API do Google Ads sem salvar alterações nem gerar cobranças:

```powershell
Set-Location "D:\Projetos\Jinn\saved\code"
.\venv\Scripts\python.exe create_jinn_campaign.py --validate-only
```
> **Saída esperada:** `[PASS] Todos os esquemas, limites de caracteres e restrições foram validados pela API sem erros.`

### Modo de Produção (Criação Real)
Quando as credenciais estiverem prontas e quiser criar a campanha:

```powershell
Set-Location "D:\Projetos\Jinn\saved\code"
.\venv\Scripts\python.exe create_jinn_campaign.py
```
> A campanha será criada no estado **PAUSADA (`PAUSED`)** por segurança. Você poderá revisá-la no painel do Google Ads e ativá-la quando desejar.

---

## 5. Passo 3 — Ativação do Daemon Guardião Financeiro

O script `jinn_ads_guardian.py` monitora o gasto e termos de pesquisa:
1. **Kill-Switch de Gastos:** Se a campanha gastar mais de **R$ 1,70** no dia atual ou registrar mais de **15 cliques sem interação**, ela é pausada imediatamente.
2. **Saneamento de Termos:** Qualquer pesquisa fora do escopo técnico de sistemas/Rust que gere clique ou mais de 10 impressões é negativada automaticamente.

### Teste Rápido do Guardião (Dry-Run):
```powershell
Set-Location "D:\Projetos\Jinn\saved\code"
.\venv\Scripts\python.exe jinn_ads_guardian.py --dry-run
```

### Agendamento no Windows Task Scheduler (PowerShell):
Para registrar a tarefa automática de hora em hora (com duração válida de 10 anos):

```powershell
Set-Location "D:\Projetos\Jinn\saved\code"
$Action = New-ScheduledTaskAction -Execute "$PWD\venv\Scripts\python.exe" -Argument "$PWD\jinn_ads_guardian.py" -WorkingDirectory "$PWD"
$Trigger = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Hours 1) -RepetitionDuration (New-TimeSpan -Days 3650)
Register-ScheduledTask -TaskName "JinnAdsGuardian" -Action $Action -Trigger $Trigger -Description "Guardião de Orçamento Jinn OS" -User "$env:USERNAME"
```

---

## 6. Arquivo de Auditoria

Todas as execuções do guardião, alertas de kill-switch e termos negativados são registrados em:
`D:\Projetos\Jinn\saved\code\guardian_audit.log`
