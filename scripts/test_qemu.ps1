if (Test-Path qemu_serial.log) {
    Remove-Item qemu_serial.log -Force
}
$p = Start-Process -FilePath "C:\Program Files\qemu\qemu-system-x86_64.exe" -ArgumentList "-boot order=d -cdrom jinn.iso -drive file=disk.img,format=raw,index=0,media=disk -netdev user,id=net0 -device virtio-net-pci,netdev=net0 -m 512M -display none -serial file:qemu_serial.log" -PassThru
Start-Sleep -Seconds 15
Stop-Process -Id $p.Id -Force
Start-Sleep -Milliseconds 500
if (Test-Path qemu_serial.log) {
    Get-Content qemu_serial.log
}
