if (Test-Path qemu_serial_256m.log) {
    Remove-Item qemu_serial_256m.log -Force
}
$p = Start-Process -FilePath "C:\Program Files\qemu\qemu-system-x86_64.exe" -ArgumentList "-boot order=d -cdrom jinn.iso -drive file=disk.img,format=raw,index=0,media=disk -netdev user,id=net0 -device virtio-net-pci,netdev=net0 -m 256M -smp 1 -display none -serial file:qemu_serial_256m.log" -PassThru
Start-Sleep -Seconds 12
Stop-Process -Id $p.Id -Force
Start-Sleep -Milliseconds 500
if (Test-Path qemu_serial_256m.log) {
    Get-Content qemu_serial_256m.log
}

