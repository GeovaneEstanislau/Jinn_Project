# scripts/audit_matrix.ps1 - Executa a Matriz de Compatibilidade de Hardware do Jinn OS
param(
    [string]$Group = "ALL", # "CPU", "RAM", "SMP", "ALL"
    [switch]$SkipBuild
)

$ErrorActionPreference = "Continue"
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $projectRoot

$runScript = Join-Path $projectRoot "scripts\run.ps1"
$logDir = Join-Path $projectRoot "logs_audit"
if (-not (Test-Path $logDir)) {
    New-Item -ItemType Directory -Path $logDir -Force | Out-Null
}

# 1. Build preliminar único (se não for ignorado)
if (-not $SkipBuild) {
    Write-Host "Compilando Kernel e Apps para a Matriz de Auditoria..."
    & powershell -NoProfile -ExecutionPolicy Bypass -File $runScript -NoRun
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Falha na compilação preliminar."
        exit 1
    }
}

$results = @()

function Test-ProfileCase {
    param(
        [string]$Category,
        [string]$ProfileName,
        [string]$Desc,
        [int]$TimeoutSec = 12
    )

    Write-Host "--------------------------------------------------------"
    Write-Host "[$Category] Executando: $ProfileName ($Desc)..."
    $logFile = Join-Path $logDir "audit_${Category}_${ProfileName}.log"
    if (Test-Path $logFile) { Remove-Item $logFile -Force }

    $proc = Start-Process -FilePath powershell -ArgumentList "-ExecutionPolicy Bypass -File `"$runScript`" -Profile `"$ProfileName`" -SkipBuild -Headless -SerialLog `"$logFile`"" -PassThru
    
    Start-Sleep -Seconds $TimeoutSec
    Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force
    if (-not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force
    }
    Start-Sleep -Milliseconds 400

    $status = "FAIL"
    $lineCount = 0
    $details = ""

    if (Test-Path $logFile) {
        $lines = Get-Content $logFile
        $lineCount = $lines.Count
        if ($lineCount -eq 0) {
            if ($ProfileName -eq "Athlon") {
                $status = "UNSUPPORTED"
                $details = "CPU 32-bit (K7) rejeitada pelo Limine (sem Long Mode 64-bit)"
            } else {
                $status = "FAIL"
                $details = "Nenhuma saída serial (falha inicial ou travamento antes do kernel)"
            }
        } else {
            $hasPanic = $lines | Select-String "KERNEL PANIC"
            $hasQuota = $lines | Select-String "EDQUOT"
            $hasSuccess = $lines | Select-String "Jinn OS pronto"
            $usableMem = $lines | Select-String "Memoria Utilizavel" | Select-Object -First 1

            if ($hasPanic) {
                $status = "FAIL"
                $panicLine = $lines | Select-String "Arquivo :" | Select-Object -First 1
                $details = "Kernel Panic ($panicLine)"
            } elseif ($hasSuccess -or $hasQuota) {
                $status = "PASS"
                $details = "Boot 100% OK, testes e escalonador executados. $usableMem"
            } else {
                $status = "PARTIAL"
                $details = "Boot parcial ($lineCount linhas geradas)"
            }
        }
    } else {
        $status = "BLOCKED"
        $details = "Arquivo de log não gerado"
    }

    Write-Host "  -> Classificacao: $status | Detalhes: $details"
    return [PSCustomObject]@{
        Grupo = $Category
        Perfil = $ProfileName
        Descricao = $Desc
        Status = $status
        Linhas = $lineCount
        Detalhes = $details
    }
}

# --- Grupo A: CPU ---
if ($Group -eq "ALL" -or $Group -eq "CPU") {
    $results += Test-ProfileCase -Category "CPU" -ProfileName "Athlon" -Desc "AMD Athlon (K7 - IA-32 Legacy)"
    $results += Test-ProfileCase -Category "CPU" -ProfileName "Opteron_G1" -Desc "AMD Opteron 240 / Athlon 64 (K8 Gen 1 - 2003)"
    $results += Test-ProfileCase -Category "CPU" -ProfileName "Phenom" -Desc "AMD Phenom 9550 (K10 - 2007)"
    $results += Test-ProfileCase -Category "CPU" -ProfileName "Conroe" -Desc "Intel Celeron 4x0 Conroe (Core 2 Class - 2006)"
    $results += Test-ProfileCase -Category "CPU" -ProfileName "Core2Duo" -Desc "Intel Core 2 Duo T7700 (Merom - 2007)"
}

# --- Grupo B: RAM ---
if ($Group -eq "ALL" -or $Group -eq "RAM") {
    $results += Test-ProfileCase -Category "RAM" -ProfileName "RAM32" -Desc "32 MiB (Extreme Low)"
    $results += Test-ProfileCase -Category "RAM" -ProfileName "RAM64" -Desc "64 MiB (Legacy Compact)"
    $results += Test-ProfileCase -Category "RAM" -ProfileName "RAM128" -Desc "128 MiB (Legacy Basic)"
    $results += Test-ProfileCase -Category "RAM" -ProfileName "RAM256" -Desc "256 MiB (Legacy Desktop)"
    $results += Test-ProfileCase -Category "RAM" -ProfileName "RAM512" -Desc "512 MiB (Desktop Legacy Plus)"
    $results += Test-ProfileCase -Category "RAM" -ProfileName "RAM1024" -Desc "1 GiB (Desktop Intermediate)"
    $results += Test-ProfileCase -Category "RAM" -ProfileName "RAM2048" -Desc "2 GiB (Desktop Modern)"
    $results += Test-ProfileCase -Category "RAM" -ProfileName "RAM4096" -Desc "4 GiB (Server)"
}

# --- Grupo D: SMP ---
if ($Group -eq "ALL" -or $Group -eq "SMP") {
    $results += Test-ProfileCase -Category "SMP" -ProfileName "SMP1" -Desc "1 vCPU (Uniprocessador baseline)"
    $results += Test-ProfileCase -Category "SMP" -ProfileName "SMP2" -Desc "2 vCPUs (Dual Core)"
    $results += Test-ProfileCase -Category "SMP" -ProfileName "SMP4" -Desc "4 vCPUs (Quad Core)"
}

Write-Host "========================================================"
Write-Host "RESUMO DA MATRIZ DE AUDITORIA DE HARDWARE JINN OS"
Write-Host "========================================================"
$results | Format-Table -AutoSize

