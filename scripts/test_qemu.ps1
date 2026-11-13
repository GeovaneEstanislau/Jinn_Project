$qemuCandidates = @(
    "D:\Projetos\Jinn\tools\qemu\qemu-system-x86_64.exe",
    "C:\Program Files\qemu\qemu-system-x86_64.exe",
    "C:\Program Files\QEMU\qemu-system-x86_64.exe"
)
$qemuExe = $qemuCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $qemuExe) {
    Write-Error "QEMU não encontrado nos caminhos conhecidos."
    exit 1
}

if (Test-Path qemu_serial.log) {
    Remove-Item qemu_serial.log -Force
}
$p = Start-Process -FilePath $qemuExe -ArgumentList "-boot order=d -cdrom jinn.iso -drive file=disk.img,format=raw,index=0,media=disk -netdev user,id=net0 -device virtio-net-pci,netdev=net0 -m 64M -smp 4 -display none -serial file:qemu_serial.log" -PassThru
Start-Sleep -Seconds 10
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
if (Test-Path qemu_serial.log) {
    Get-Content qemu_serial.log
}
