# scripts/test_persistence.ps1 - Teste Automatizado de Persistencia FAT32 e Recuperacao Cold-Reboot
param(
    [int]$TimeoutSec = 16
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $projectRoot

$qemuExe = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemuExe)) {
    throw "QEMU executavel nao encontrado em: $qemuExe"
}

$isoPath = Join-Path $projectRoot "jinn.iso"
$masterDisk = Join-Path $projectRoot "disk.img"
$testDisk = Join-Path $projectRoot "test_persist_disk.img"
$boot1Log = Join-Path $projectRoot "qemu_persist_boot1.log"
$boot2Log = Join-Path $projectRoot "qemu_persist_boot2.log"

Write-Host "=================================================================="
Write-Host "  JINN OS 1.1 - TESTE DE PERSISTENCIA FAT32 E COLD-REBOOT"
Write-Host "=================================================================="

# 1. Prepara imagem descartavel de teste (Seção 10)
Write-Host "[1/6] Preparando imagem de teste descartavel: $testDisk ..."
Copy-Item -Path $masterDisk -Destination $testDisk -Force
$initialHash = (Get-FileHash -Path $testDisk -Algorithm SHA256).Hash
Write-Host "  SHA256 inicial da imagem de teste: $initialHash"

if (Test-Path $boot1Log) { Remove-Item $boot1Log -Force }
if (Test-Path $boot2Log) { Remove-Item $boot2Log -Force }

# 2. Inicializacao 1 (Gravacao, Expansao e Flush via sys_sync)
Write-Host "[2/6] Inicializando Instancia 1 do QEMU (Fase de Escrita, Flush e Sync)..."
$p1 = Start-Process -FilePath $qemuExe -ArgumentList "-boot order=d -cdrom `"$isoPath`" -drive file=`"$testDisk`",format=raw,index=0,media=disk -netdev user,id=net0 -device virtio-net-pci,netdev=net0 -m 512M -smp 4 -display none -serial file:`"$boot1Log`"" -PassThru

Start-Sleep -Seconds $TimeoutSec
if (-not $p1.HasExited) {
    Stop-Process -Id $p1.Id -Force
}
Start-Sleep -Milliseconds 500

Write-Host "[3/6] Analisando telemetria da Instancia 1 (Boot 1)..."
if (-not (Test-Path $boot1Log)) {
    throw "Falha: Log do Boot 1 nao foi gerado!"
}
$log1Content = Get-Content $boot1Log -Raw
Write-Host $log1Content

$requiredBoot1Milestones = @(
    "Syscall 23 (sys_sync) executada com sucesso",
    "Escrita e leitura imediata de pequeno arquivo validadas",
    "Escrita e leitura transpondo fronteira de setor (700B) validadas",
    "Escrita multi-cluster com alocacao dinamica de clusters (5000B) validada",
    "Escrita em offset nao-zero com preservacao de bytes adjacentes validada",
    "Payload deterministico gravado e sincronizado em /disk/PERSIST.TXT para teste de reboot"
)

foreach ($m in $requiredBoot1Milestones) {
    if ($log1Content -notmatch [regex]::Escape($m)) {
        throw "FALHA NO BOOT 1: Marco obrigatorio nao encontrado: $m"
    }
    Write-Host "  [OK] Marco Boot 1 validado: $m"
}

# 3. Verificacao forense no arquivo de disco no Host
Write-Host "[4/6] Verificando persistencia fisica real no arquivo de disco (Host Analysis)..."
$postWriteHash = (Get-FileHash -Path $testDisk -Algorithm SHA256).Hash
Write-Host "  SHA256 pos-escrita da imagem de teste: $postWriteHash"
if ($postWriteHash -eq $initialHash) {
    throw "FALHA: A imagem de disco fisica nao foi modificada apos o flush do QEMU!"
}
Write-Host "  [OK] Modificacao fisica comprovada: Hash do disco alterado com sucesso."

# Inspeciona bytes brutos do arquivo de disco
$diskRawBytes = [System.IO.File]::ReadAllBytes($testDisk)
$expectedToken = [System.Text.Encoding]::ASCII.GetBytes("JINN_OS_PERSISTENCE_MAGIC_V11_FAT32_COLD_REBOOT_VALIDATION_TOKEN_987654321")
$tokenFound = $false
for ($i = 0; $i -le ($diskRawBytes.Length - $expectedToken.Length); $i += 512) {
    # Busca rapida alinhada a setor
    $match = $true
    for ($k = 0; $k -lt 32; $k++) {
        if ($diskRawBytes[$i + $k] -ne $expectedToken[$k]) {
            $match = $false
            break
        }
    }
    if ($match) {
        $tokenFound = $true
        Write-Host "  [EVIDENCE] Token deterministico encontrado fisicamente no setor LBA $([math]::Floor($i / 512)) (offset $i)!"
        break
    }
}
if (-not $tokenFound) {
    # Busca exata linear
    for ($i = 0; $i -le ($diskRawBytes.Length - $expectedToken.Length); $i++) {
        if ($diskRawBytes[$i] -eq $expectedToken[0]) {
            $match = $true
            for ($k = 1; $k -lt $expectedToken.Length; $k++) {
                if ($diskRawBytes[$i + $k] -ne $expectedToken[$k]) { $match = $false; break }
            }
            if ($match) {
                $tokenFound = $true
                Write-Host "  [EVIDENCE] Token deterministico encontrado no offset byte $i do arquivo RAW de disco!"
                break
            }
        }
    }
}
if (-not $tokenFound) {
    throw "FALHA CRITICA: O payload nao esta presente nos bytes fisicos do arquivo disk.img!"
}

# 4. Inicializacao 2 (Cold Reboot - Leitura e Validacao Apos Reinicializacao Completa)
Write-Host "[5/6] Inicializando Instancia 2 do QEMU (Cold Reboot - Leitura a Frio do Disco Persistido)..."
$p2 = Start-Process -FilePath $qemuExe -ArgumentList "-boot order=d -cdrom `"$isoPath`" -drive file=`"$testDisk`",format=raw,index=0,media=disk -netdev user,id=net0 -device virtio-net-pci,netdev=net0 -m 512M -smp 4 -display none -serial file:`"$boot2Log`"" -PassThru

Start-Sleep -Seconds $TimeoutSec
if (-not $p2.HasExited) {
    Stop-Process -Id $p2.Id -Force
}
Start-Sleep -Milliseconds 500

Write-Host "[6/6] Analisando telemetria da Instancia 2 (Cold Reboot)..."
if (-not (Test-Path $boot2Log)) {
    throw "Falha: Log do Boot 2 nao foi gerado!"
}
$log2Content = Get-Content $boot2Log -Raw
Write-Host $log2Content

$requiredBoot2Milestones = @(
    "PERSISTENCIA COMPROVADA: Payload intacto recuperado apos reinicializacao do QEMU!",
    "Integridade estrita de arquivos preexistentes (/disk/README.TXT) preservada 100%!",
    "test_adv_fat32_persistent_write_and_recovery: Auditoria e Integridade de Escrita FAT32 APROVADA!"
)

foreach ($m in $requiredBoot2Milestones) {
    if ($log2Content -notmatch [regex]::Escape($m)) {
        throw "FALHA NO COLD REBOOT: Marco obrigatorio nao encontrado: $m"
    }
    Write-Host "  [OK] Marco Cold Reboot validado: $m"
}

Write-Host ""
Write-Host "=================================================================="
Write-Host "  SUCESSO TOTAL: PERSISTENCIA FAT32 E RECUPERACAO COLD-REBOOT 100% VALIDADA!"
Write-Host "=================================================================="

# Limpeza de artefatos temporarios descartaveis
if (Test-Path $testDisk) { Remove-Item $testDisk -Force }
