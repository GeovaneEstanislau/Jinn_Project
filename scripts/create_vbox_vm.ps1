# create_vbox_vm.ps1 - Cria e configura automaticamente a VM do Aladdin OS no VirtualBox
$ErrorActionPreference = 'Stop'

$vbox = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"
if (-not (Test-Path $vbox)) {
    Write-Error "VirtualBox não encontrado em $vbox"
    exit 1
}

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$isoPath = Join-Path $projectRoot "jinn.iso"
$vdiPath = Join-Path $projectRoot "disk.vdi"
$vmName = "Aladdin OS"

if (-not (Test-Path $isoPath)) {
    Write-Error "ISO não encontrada em $isoPath. Execute scripts\run.ps1 -NoRun primeiro."
    exit 1
}

# Se a VM já existir, desregistra para recriar de forma limpa
$existing = & $vbox list vms
if ($existing -match [regex]::Escape($vmName)) {
    Write-Host "Removendo VM existente '$vmName'..."
    & $vbox unregistervm $vmName --delete
}

Write-Host "Criando Máquina Virtual '$vmName'..."
& $vbox createvm --name $vmName --ostype "Other_64" --register

Write-Host "Configurando hardware (1024MB RAM, 2 vCPUs, I/O APIC, VBoxSVGA, Mouse PS/2, Intel PRO/1000)..."
& $vbox modifyvm $vmName `
    --memory 1024 `
    --cpus 2 `
    --ioapic on `
    --mouse ps2 `
    --vram 64 `
    --graphicscontroller vboxsvga `
    --boot1 dvd `
    --boot2 disk `
    --boot3 none `
    --nic1 nat `
    --nictype1 82540EM `
    --pae on `
    --uart1 0x3F8 4 `
    --uartmode1 file (Join-Path $projectRoot "vbox_serial.log")

Write-Host "Adicionando controladora de armazenamento IDE..."
& $vbox storagectl $vmName --name "IDE Controller" --add ide --controller PIIX4

$imgPath = Join-Path $projectRoot "disk.img"
if (Test-Path $imgPath) {
    if (Test-Path $vdiPath) {
        Remove-Item $vdiPath -Force
    }
    Write-Host "Convertendo disk.img em disk.vdi para VirtualBox..."
    & $vbox convertfromraw $imgPath $vdiPath --format VDI
}

if (Test-Path $vdiPath) {
    Write-Host "Conectando disk.vdi (FAT32 + Ext2) na Porta 0 (Primary Master)..."
    & $vbox storageattach $vmName --storagectl "IDE Controller" --port 0 --device 0 --type hdd --medium $vdiPath
}

Write-Host "Conectando jinn.iso na Porta 1 (Secondary Master)..."
& $vbox storageattach $vmName --storagectl "IDE Controller" --port 1 --device 0 --type dvddrive --medium $isoPath

Write-Host ""
Write-Host "================================================================" -ForegroundColor Green
Write-Host " Máquina Virtual '$vmName' criada e configurada com sucesso!" -ForegroundColor Green
Write-Host " Você já pode abrir o VirtualBox e clicar em 'Iniciar'." -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Green
