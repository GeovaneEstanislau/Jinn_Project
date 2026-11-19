<#
.SYNOPSIS
    Publica ou gera links de 1-clique para as 6 Issues Estratégicas de Embarcados e JHCS no GitHub.

.DESCRIPTION
    Lê os arquivos de especificação em docs/strategic-issues/ e:
    1. Se fornecido -Token (ou $env:GITHUB_TOKEN), cria as issues diretamente via API do GitHub.
    2. Se executado sem token, exibe URLs de 1-clique pré-formatadas para criação instantânea no navegador.

.PARAMETER Token
    Token de Acesso Pessoal (PAT) do GitHub com escopo 'repo' ou 'public_repo'.

.EXAMPLE
    .\scripts\publish_strategic_issues.ps1 -Token "ghp_xxxx"
    .\scripts\publish_strategic_issues.ps1
#>

param (
    [string]$Token = $env:GITHUB_TOKEN,
    [string]$DefaultOwner = "GeovaneEstanislau"
)

$issues = @(
    @{
        File = "docs\strategic-issues\01_NEON_ARM64_SIMD.md"
        Repo = "$DefaultOwner/jhcs-rs"
        Title = "[Backend SIMD] Implement NEON Vector Acceleration for AArch64 (ARM64)"
        Labels = @("good first issue", "help wanted", "jhcs", "simd", "embedded", "arm64")
    },
    @{
        File = "docs\strategic-issues\02_ZERO_ALLOC_COMPUTE_GRAPH.md"
        Repo = "$DefaultOwner/jhcs-rs"
        Title = "[Core / Embedded] Zero-Alloc Static Mode for ComputeGraph (no extern crate alloc)"
        Labels = @("help wanted", "jhcs", "embedded", "hard-real-time", "no-std")
    },
    @{
        File = "docs\strategic-issues\03_CYCLE_PRECISION_TELEMETRY.md"
        Repo = "$DefaultOwner/jhcs-rs"
        Title = "[Benchmark / Telemetry] Cycle-Accurate Timers (RDTSC / CNTVCT_EL0) and Energy Efficiency Metrics in ComputeBroker"
        Labels = @("help wanted", "jhcs", "embedded", "telemetry", "dsp")
    },
    @{
        File = "docs\strategic-issues\04_RING3_EDGE_PROCESSING_DAEMON.md"
        Repo = "$DefaultOwner/Jinn_Project"
        Title = "[JHCS / Ring 3] Real-Time Edge Signal Processing & Inference Daemon Showcase"
        Labels = @("help wanted", "jhcs", "embedded", "ring3", "showcase")
    },
    @{
        File = "docs\strategic-issues\05_VIRTIO_GPU_KDM_BACKEND.md"
        Repo = "$DefaultOwner/Jinn_Project"
        Title = "[KDM / Drivers] VirtIO-GPU Compute / Zero-Copy DMA Backend for JHCS"
        Labels = @("help wanted", "jhcs", "drivers", "dma", "virtio", "gpu")
    },
    @{
        File = "docs\strategic-issues\06_JINN_EDGE_MINIMAL_PROFILE.md"
        Repo = "$DefaultOwner/Jinn_Project"
        Title = "[Embedded / Boot] 'Jinn-Edge' Minimal Footprint Compilation Profile (< 16 MiB RAM Baseline)"
        Labels = @("help wanted", "embedded", "kernel", "footprint", "optimization")
    }
)

Write-Host "==========================================================================" -ForegroundColor Cyan
Write-Host " Jinn OS & JHCS — Publicador de Issues Estratégicas no GitHub" -ForegroundColor Cyan
Write-Host "==========================================================================" -ForegroundColor Cyan

$hasToken = -not [string]::IsNullOrWhiteSpace($Token)

foreach ($item in $issues) {
    if (-not (Test-Path $item.File)) {
        Write-Warning "Arquivo não encontrado: $($item.File)"
        continue
    }

    $body = Get-Content -Path $item.File -Raw -Encoding UTF8
    $repo = $item.Repo
    $title = $item.Title
    $labels = $item.Labels

    Write-Host "`n[*] [$repo] $title" -ForegroundColor Yellow

    if ($hasToken) {
        $headers = @{
            "Authorization" = "Bearer $Token"
            "Accept"        = "application/vnd.github.v3+json"
            "User-Agent"    = "Jinn-Issue-Publisher"
        }

        $payload = @{
            title  = $title
            body   = $body
            labels = $labels
        } | ConvertTo-Json -Depth 3

        $uri = "https://api.github.com/repos/$repo/issues"

        try {
            $resp = Invoke-RestMethod -Uri $uri -Method Post -Headers $headers -Body $payload -ContentType "application/json; charset=utf-8"
            Write-Host "    [OK] Criada com sucesso! URL: $($resp.html_url)" -ForegroundColor Green
        } catch {
            Write-Warning "    [!] Falha na API do GitHub: $_"
            # Fallback para link manual
            $encodedTitle = [System.Uri]::EscapeDataString($title)
            $encodedBody  = [System.Uri]::EscapeDataString($body)
            $encodedLabels = [System.Uri]::EscapeDataString(($labels -join ","))
            $webUrl = "https://github.com/$repo/issues/new?title=$encodedTitle&body=$encodedBody&labels=$encodedLabels"
            Write-Host "    Link de 1-Clique: $webUrl" -ForegroundColor Gray
        }
    } else {
        $encodedTitle = [System.Uri]::EscapeDataString($title)
        $encodedBody  = [System.Uri]::EscapeDataString($body)
        $encodedLabels = [System.Uri]::EscapeDataString(($labels -join ","))
        $webUrl = "https://github.com/$repo/issues/new?title=$encodedTitle&body=$encodedBody&labels=$encodedLabels"
        Write-Host "    Labels: $($labels -join ', ')" -ForegroundColor DarkGray
        Write-Host "    Link de 1-Clique:" -ForegroundColor White
        Write-Host "    $webUrl" -ForegroundColor Cyan
    }
}

Write-Host "`n==========================================================================" -ForegroundColor Cyan
if (-not $hasToken) {
    Write-Host "[i] Dica: Para criar todas automaticamente sem abrir o navegador, execute:" -ForegroundColor Yellow
    Write-Host "    .\scripts\publish_strategic_issues.ps1 -Token 'ghp_SEU_GITHUB_TOKEN'" -ForegroundColor White
}
Write-Host "==========================================================================" -ForegroundColor Cyan
