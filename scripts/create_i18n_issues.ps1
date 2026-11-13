<#
.SYNOPSIS
    Cria ou gera links para as Issues de internacionalização e chamamento de contribuidores no GitHub.

.DESCRIPTION
    Este script lê os rascunhos de issues multilíngues em docs/i18n-issues/ e:
    1. Se fornecido um token do GitHub (-Token ou $env:GITHUB_TOKEN), cria as issues diretamente via GitHub REST API.
    2. Se executado sem token, exibe URLs diretas clicáveis com título, corpo e labels pré-preenchidos.

.PARAMETER Token
    Token de Acesso Pessoal (PAT) do GitHub com escopo 'repo' ou 'public_repo'.

.EXAMPLE
    .\scripts\create_i18n_issues.ps1 -Token "ghp_xxxxxxxxxxxx"
    .\scripts\create_i18n_issues.ps1
#>

param (
    [string]$Token = $env:GITHUB_TOKEN,
    [string]$Repo = "GeovaneEstanislau/Jinn_Project"
)

$issueFiles = @(
    "docs\i18n-issues\ISSUE_PT.md",
    "docs\i18n-issues\ISSUE_EN.md",
    "docs\i18n-issues\ISSUE_ZH.md",
    "docs\i18n-issues\ISSUE_JA.md",
    "docs\i18n-issues\ISSUE_HI.md",
    "docs\i18n-issues\ISSUE_ES.md",
    "docs\i18n-issues\ISSUE_RU.md",
    "docs\i18n-issues\ISSUE_DE.md",
    "docs\i18n-issues\ISSUE_IT.md",
    "docs\i18n-issues\ISSUE_HU.md",
    "docs\i18n-issues\ISSUE_HR.md",
    "docs\i18n-issues\ISSUE_NO.md"
)

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Jinn OS — Publicador de Issues Multilíngues no GitHub" -ForegroundColor Cyan
Write-Host " Repositório: $Repo" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$results = @()

foreach ($file in $issueFiles) {
    if (-not (Test-Path $file)) {
        Write-Warning "Arquivo não encontrado: $file"
        continue
    }

    $rawLines = Get-Content $file -Encoding UTF8
    $title = ""
    $labels = @("documentation", "i18n", "help wanted", "good first issue")
    $bodyLines = @()

    $headerDone = $false
    foreach ($line in $rawLines) {
        if (-not $headerDone) {
            if ($line -match '^# Title:\s*(.+)$') {
                $title = $matches[1].Trim()
                continue
            }
            if ($line -match '^# Labels:\s*(.+)$') {
                $labels = $matches[1].Split(',') | ForEach-Object { $_.Trim() }
                continue
            }
            if ([string]::IsNullOrWhiteSpace($line)) {
                $headerDone = $true
                continue
            }
        }
        $bodyLines += $line
    }

    $body = $bodyLines -join "`n"

    if (-not [string]::IsNullOrWhiteSpace($Token)) {
        Write-Host "Publicando issue: $title..." -NoNewline
        $headers = @{
            "Authorization" = "token $Token"
            "Accept" = "application/vnd.github.v3+json"
            "User-Agent" = "Jinn-Issue-Publisher"
        }

        $payload = @{
            title = $title
            body = $body
            labels = $labels
        } | ConvertTo-Json -Depth 5

        try {
            $resp = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/issues" -Method Post -Headers $headers -Body ([System.Text.Encoding]::UTF8.GetBytes($payload)) -ContentType "application/json; charset=utf-8"
            Write-Host " [CRIADA: #$($resp.number)]" -ForegroundColor Green
            $results += [PSCustomObject]@{
                Title = $title
                Status = "Criada (#$($resp.number))"
                Url = $resp.html_url
            }
        } catch {
            Write-Host " [ERRO: $_]" -ForegroundColor Red
            $results += [PSCustomObject]@{
                Title = $title
                Status = "Erro na API"
                Url = ""
            }
        }
    } else {
        # Gerar URL pré-formatada para criação direta no navegador
        $encodedTitle = [System.Uri]::EscapeDataString($title)
        $encodedBody = [System.Uri]::EscapeDataString($body)
        $encodedLabels = [System.Uri]::EscapeDataString(($labels -join ','))
        $webUrl = "https://github.com/$Repo/issues/new?title=$encodedTitle&body=$encodedBody&labels=$encodedLabels"
        
        $results += [PSCustomObject]@{
            Title = $title
            Status = "Pronto para Criar"
            Url = $webUrl
        }
    }
}

Write-Host "`nResumo de Processamento:" -ForegroundColor Yellow
$results | Format-Table -AutoSize
