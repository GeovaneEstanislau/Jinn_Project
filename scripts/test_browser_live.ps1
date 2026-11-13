$logFile = "d:\Projetos\Jinn\qemu_live.log"
if (Test-Path $logFile) { Remove-Item $logFile -Force }

$port = 5566
$qemuArgs = @(
    "-boot", "order=d",
    "-cdrom", "jinn.iso",
    "-drive", "file=disk.img,format=raw,index=0,media=disk",
    "-netdev", "user,id=net0",
    "-device", "pcnet,netdev=net0",
    "-m", "64M",
    "-smp", "4",
    "-display", "none",
    "-serial", "file:$logFile",
    "-monitor", "tcp:127.0.0.1:$port,server,nowait"
)

Write-Host "Iniciando QEMU..."
$p = Start-Process -FilePath "D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe" -ArgumentList $qemuArgs -PassThru

Start-Sleep -Seconds 6

Write-Host "Conectando ao monitor do QEMU na porta $port..."
$tcp = New-Object System.Net.Sockets.TcpClient
$tcp.Connect("127.0.0.1", $port)
$stream = $tcp.GetStream()
$writer = New-Object System.IO.StreamWriter($stream)
$writer.AutoFlush = $true

function Send-QemuKey($k) {
    $writer.WriteLine("sendkey $k")
    Start-Sleep -Milliseconds 120
}

Write-Host "Enviando comando 'browser' para o terminal do Aladdin..."
# Digita 'browser'
foreach ($ch in @('b','r','o','w','s','e','r')) {
    Send-QemuKey $ch
}
Send-QemuKey "ret"

Start-Sleep -Seconds 5

$writer.WriteLine("quit")
Start-Sleep -Seconds 1
$tcp.Close()

Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue

Write-Host "=== LOG SERIAL COMPLETO ==="
if (Test-Path $logFile) {
    Get-Content $logFile
} else {
    Write-Host "Log nao encontrado!"
}

