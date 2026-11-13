param(
    [string]$Mem = "64M",
    [switch]$NoDisk = $false,
    [string]$Smp = "4"
)

$qemu = "D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe"
$port = 5566
$diskSuffix = if ($NoDisk) { "nodisk" } else { "disk" }
$logFile = "qemu_${Mem}_${diskSuffix}_${Smp}.log"
if (Test-Path $logFile) { Remove-Item $logFile -Force }

$argsList = @(
    "-boot", "order=d",
    "-cdrom", "jinn.iso",
    "-m", $Mem,
    "-smp", $Smp,
    "-display", "none",
    "-serial", "file:$logFile",
    "-monitor", "tcp:127.0.0.1:$port,server,nowait"
)
if (-not $NoDisk) {
    $argsList += @("-drive", "file=disk.img,format=raw,index=0,media=disk")
}

Write-Host "[TEST] Starting QEMU (Mem=$Mem, NoDisk=$NoDisk, Smp=$Smp)..."
$p = Start-Process -FilePath $qemu -ArgumentList $argsList -PassThru

Start-Sleep -Seconds 2

$client = $null
for ($i = 0; $i -lt 25; $i++) {
    try {
        $client = New-Object System.Net.Sockets.TcpClient("127.0.0.1", $port)
        if ($client.Connected) {
            Write-Host "[TEST] Connected to QEMU monitor on attempt $($i+1)"
            break
        }
    } catch {
        Start-Sleep -Milliseconds 800
    }
}

if (-not $client -or -not $client.Connected) {
    Write-Host "[TEST] Failed to connect to QEMU monitor!"
    if (-not $p.HasExited) { $p.Kill() }
    exit 1
}

$stream = $client.GetStream()
$writer = New-Object System.IO.StreamWriter($stream)
$writer.AutoFlush = $true

function Send-Cmd($cmd) {
    $writer.WriteLine($cmd)
    Start-Sleep -Milliseconds 150
}

Write-Host "[TEST] Waiting 22s for boot..."
for ($s = 1; $s -le 22; $s++) {
    Start-Sleep -Seconds 1
    if ($s % 5 -eq 0) { Write-Host "  ... waited ${s}s" }
}

Write-Host "[TEST] Typing 'browser' into terminal..."
foreach ($ch in "browser".ToCharArray()) {
    Send-Cmd "sendkey $ch"
}
Send-Cmd "sendkey ret"

Write-Host "[TEST] Waiting 8s for processing..."
Start-Sleep -Seconds 8

Send-Cmd "quit"
Start-Sleep -Seconds 1

$writer.Close()
$client.Close()
if (-not $p.HasExited) { $p.Kill() }

Write-Host "[TEST] Log output ($logFile):"
if (Test-Path $logFile) {
    Get-Content $logFile -Tail 35
}
