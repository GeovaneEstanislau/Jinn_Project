# test_interop.ps1 - Suite de Execucao e Validacao de Interoperabilidade TCP Real
param(
    [int]$TimeoutSec = 30
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location -Path $projectRoot

Write-Host "=================================================================="
Write-Host " JINN OS NETWORK INTEROPERABILITY 1.0 - TEST RUNNER"
Write-Host "=================================================================="

# 1. Localiza executaveis QEMU e Python
$qemu = (Get-Command qemu-system-x86_64 -ErrorAction SilentlyContinue).Source
if (-not $qemu) {
    $qemuCandidates = @(
        "C:\Program Files\qemu\qemu-system-x86_64.exe",
        "C:\Program Files\QEMU\qemu-system-x86_64.exe"
    )
    $qemu = $qemuCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $qemu) {
    Write-Error "qemu-system-x86_64 nao encontrado."
    exit 1
}

$peerScript = Join-Path $projectRoot "tools\interop-peer\host_peer.py"
if (-not (Test-Path $peerScript)) {
    Write-Error "Script host_peer.py nao encontrado em $peerScript"
    exit 1
}

$isoPath = Join-Path $projectRoot "jinn.iso"
if (-not (Test-Path $isoPath)) {
    Write-Error "jinn.iso nao encontrada. Execute scripts\run.ps1 -NoRun primeiro."
    exit 1
}

$diskImg = Join-Path $projectRoot "disk.img"
$pcapPath = Join-Path $projectRoot "interop_capture.pcap"
$serialLog = Join-Path $projectRoot "interop_serial.log"

# Limpa logs e capturas anteriores
Remove-Item $pcapPath -Force -ErrorAction SilentlyContinue
Remove-Item $serialLog -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $projectRoot "host_peer_stdout.log") -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $projectRoot "host_peer_stderr.log") -Force -ErrorAction SilentlyContinue

# 2. Inicia o Host Peer TCP Independente (Python / Windows TCP/IP) em segundo plano
Write-Host "[1/4] Iniciando Host Peer TCP independente na porta 9999..."
$peerProcess = Start-Process -FilePath "py" `
    -ArgumentList @("-u", $peerScript, "--mode", "server", "--connections", "2", "--timeout", "$TimeoutSec") `
    -PassThru -NoNewWindow -RedirectStandardOutput (Join-Path $projectRoot "host_peer_stdout.log") `
    -RedirectStandardError (Join-Path $projectRoot "host_peer_stderr.log")

Start-Sleep -Milliseconds 1200

# 3. Dispara QEMU com captura PCAP e redirecionamento de porta
Write-Host "[2/4] Disparando QEMU com VirtIO-Net, filter-dump (.pcap) e porta redirecionada 8088->8080..."
$qemuArgs = @(
    "-boot", "order=d",
    "-cdrom", $isoPath,
    "-m", "512M",
    "-netdev", "user,id=net0,hostfwd=tcp:127.0.0.1:8088-:8080",
    "-device", "virtio-net-pci,netdev=net0",
    "-object", "filter-dump,id=f0,netdev=net0,file=$pcapPath",
    "-serial", "file:$serialLog",
    "-display", "none"
)
if (Test-Path $diskImg) {
    $qemuArgs += @("-drive", "file=$diskImg,format=raw,index=0,media=disk")
}

$qemuProcess = Start-Process -FilePath $qemu -ArgumentList $qemuArgs -PassThru

# 4. Monitora execucao ate que os testes no serialLog sejam concluidos ou timeout
Write-Host "[3/4] Monitorando comunicacao do kernel com o peer independente..."
$elapsed = 0
$done = $false

while ($elapsed -lt $TimeoutSec) {
    Start-Sleep -Seconds 1
    $elapsed += 1

    if (Test-Path $serialLog) {
        $content = Get-Content $serialLog -Raw -ErrorAction SilentlyContinue
        if ($content -and ($content.Contains("CONCLUSAO DOS TESTES DE INTEROPERABILIDADE") -or $content.Contains("TODOS OS TESTES DA SUITE ADVERSARIAL FORAM CONCLUIDOS"))) {
            Write-Host "Testes concluidos no kernel apos ${elapsed}s!"
            $done = $true
            break
        }
    }

    if ($peerProcess.HasExited) {
        Write-Host "Host peer finalizou sua sessao."
        Start-Sleep -Seconds 2
        $done = $true
        break
    }
}

# 5. Finaliza QEMU e Peer
Write-Host "[4/4] Encerrando processos e coletando evidencias..."
if (-not $qemuProcess.HasExited) {
    Stop-Process -Id $qemuProcess.Id -Force -ErrorAction SilentlyContinue
}
if (-not $peerProcess.HasExited) {
    Stop-Process -Id $peerProcess.Id -Force -ErrorAction SilentlyContinue
}

# 6. Exibe resultados
Write-Host "`n=== SAIDA DO HOST PEER INDEPENDENTE ==="
if (Test-Path (Join-Path $projectRoot "host_peer_stdout.log")) {
    Get-Content (Join-Path $projectRoot "host_peer_stdout.log") | Write-Host
}
if (Test-Path (Join-Path $projectRoot "host_peer_stderr.log")) {
    $errs = Get-Content (Join-Path $projectRoot "host_peer_stderr.log")
    if ($errs) {
        Write-Host "Host peer stderr: $errs"
    }
}

Write-Host "`n=== SAIDA DO SERIAL DO KERNEL (JINN OS) ==="
if (Test-Path $serialLog) {
    $serialLines = Get-Content $serialLog
    $serialLines | Select-String -Pattern "INTEROP|CONCLUSAO|PASS|FAIL" | Write-Host
}

Write-Host "`n=== ARTEFATOS GERADOS ==="
if (Test-Path $pcapPath) {
    $pcapItem = Get-Item $pcapPath
    $pLen = $pcapItem.Length
    Write-Host "Captura PCAP gerada: $($pcapItem.FullName) ($pLen bytes)"
} else {
    Write-Host "Aviso: Arquivo PCAP nao foi gerado."
}

if (Test-Path $serialLog) {
    $serialItem = Get-Item $serialLog
    $sLen = $serialItem.Length
    Write-Host "Log serial do kernel: $($serialItem.FullName) ($sLen bytes)"
}
