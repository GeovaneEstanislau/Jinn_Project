$p = Start-Process -FilePath "C:\Program Files\qemu\qemu-system-x86_64.exe" -ArgumentList "-boot order=d -cdrom jinn.iso -drive file=disk.img,format=raw,index=0,media=disk -m 512M -display none -serial file:qemu_serial.log" -PassThru
Start-Sleep -Seconds 8
Stop-Process -Id $p.Id -Force
if (Test-Path qemu_serial.log) {
    Get-Content qemu_serial.log
}
