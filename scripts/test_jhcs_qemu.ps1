$qemuExe = "D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemuExe)) {
    $qemuExe = "C:\Program Files\qemu\qemu-system-x86_64.exe"
}

if (Test-Path serial_jhcs.log) {
    Remove-Item serial_jhcs.log -Force
}

Write-Host "Iniciando QEMU headless para validacao do JHCS Ring 3..."
$p = Start-Process -FilePath $qemuExe -ArgumentList "-boot order=d -cdrom jinn.iso -drive file=disk.img,format=raw,index=0,media=disk -netdev user,id=net0 -device virtio-net-pci,netdev=net0 -m 256M -smp 4 -display none -serial file:serial_jhcs.log" -PassThru

Write-Host "Aguardando 14 segundos de execucao..."
Start-Sleep -Seconds 14

Write-Host "Encerrando processo QEMU (PID $($p.Id))..."
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

if (Test-Path serial_jhcs.log) {
    Write-Host "=== CONTEUDO DO LOG SERIAL (serial_jhcs.log) ==="
    Get-Content serial_jhcs.log
} else {
    Write-Error "serial_jhcs.log nao foi gerado."
}
