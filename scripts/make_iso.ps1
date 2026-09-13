# make_iso.ps1 - Pure PowerShell ISO9660 + El Torito Generator for Jinn OS / Limine
# Zero external dependencies: runs natively on Windows PowerShell

$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = (Resolve-Path (Join-Path $scriptDir "..")).Path
$isoPath = Join-Path $projectRoot "jinn.iso"

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host '  Jinn OS — Gerador Nativo de ISO (ISO9660 + El Torito / Limine)' -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan

# 1. Localizar componentes
$limineCdPath = Join-Path $projectRoot "limine\usr\share\limine\limine-bios-cd.bin"
if (-not (Test-Path $limineCdPath)) {
    $limineCdPath = Join-Path $projectRoot "limine\limine-bios-cd.bin"
}

$limineSysPath = Join-Path $projectRoot "limine\usr\share\limine\limine-bios.sys"
if (-not (Test-Path $limineSysPath)) {
    $limineSysPath = Join-Path $projectRoot "limine\limine-bios.sys"
}

$limineCfgPath = Join-Path $projectRoot "iso_root\limine.conf"
$kernelElfPath = Join-Path $projectRoot "iso_root\boot\kernel.elf"
if (-not (Test-Path $kernelElfPath)) {
    $kernelElfPath = Join-Path $projectRoot "boot\kernel.elf"
}

$bootx64Path = Join-Path $projectRoot "limine\usr\share\limine\BOOTX64.EFI"

if (-not (Test-Path $limineCdPath)) { throw "Arquivo nao encontrado: $limineCdPath" }
if (-not (Test-Path $limineSysPath)) { throw "Arquivo nao encontrado: $limineSysPath" }
if (-not (Test-Path $limineCfgPath)) { throw "Arquivo nao encontrado: $limineCfgPath" }
if (-not (Test-Path $kernelElfPath)) { throw "Arquivo nao encontrado: $kernelElfPath" }

Write-Host "[+] Componentes verificados:" -ForegroundColor Green
Write-Host "    Bootloader CD : $limineCdPath"
Write-Host "    Bootloader SYS: $limineSysPath"
Write-Host "    Limine Config : $limineCfgPath"
Write-Host "    Kernel ELF    : $kernelElfPath"

$limineCdBytes  = [System.IO.File]::ReadAllBytes($limineCdPath)
$limineSysBytes = [System.IO.File]::ReadAllBytes($limineSysPath)
$limineCfgBytes = [System.IO.File]::ReadAllBytes($limineCfgPath)
$kernelElfBytes = [System.IO.File]::ReadAllBytes($kernelElfPath)

$hasEfi = Test-Path $bootx64Path
$bootx64Bytes = if ($hasEfi) { [System.IO.File]::ReadAllBytes($bootx64Path) } else { $null }
if ($hasEfi) {
    Write-Host "    UEFI Boot     : $bootx64Path"
}

$SECTOR_SIZE = 2048

# Funcoes auxiliares para formatacao ISO9660
function To-U16Both([uint16]$v) {
    $lsb = [System.BitConverter]::GetBytes([uint16]$v)
    $msb = [System.BitConverter]::GetBytes([uint16]$v)
    [Array]::Reverse($msb)
    return ,($lsb + $msb)
}

function To-U32Both([uint32]$v) {
    $lsb = [System.BitConverter]::GetBytes([uint32]$v)
    $msb = [System.BitConverter]::GetBytes([uint32]$v)
    [Array]::Reverse($msb)
    return ,($lsb + $msb)
}

function To-PaddedAscii([string]$str, [int]$len) {
    $bytes = New-Object byte[] $len
    for ($i = 0; $i -lt $len; $i++) { $bytes[$i] = 0x20 } # espaco
    $src = [System.Text.Encoding]::ASCII.GetBytes($str)
    $copyLen = [Math]::Min($src.Length, $len)
    [Array]::Copy($src, 0, $bytes, 0, $copyLen)
    return ,$bytes
}

# Layout dos setores
$pvdSector         = 16
$bootDescSector    = 17
$termSector        = 18
$bootCatalogSector = 19
$pathTableLSector  = 20
$pathTableMSector  = 21
$rootDirSector     = 22
$bootDirSector     = 23
$efiDirSector      = 24
$efiBootDirSector  = 25

$currentFileSector = 26

# Mapeamento de arquivos para setores
$limineCdSector  = $currentFileSector; $currentFileSector += [int][Math]::Ceiling($limineCdBytes.Length / $SECTOR_SIZE)
$limineSysSector = $currentFileSector; $currentFileSector += [int][Math]::Ceiling($limineSysBytes.Length / $SECTOR_SIZE)
$limineCfgSector = $currentFileSector; $currentFileSector += [int][Math]::Ceiling($limineCfgBytes.Length / $SECTOR_SIZE)
$kernelElfSector = $currentFileSector; $currentFileSector += [int][Math]::Ceiling($kernelElfBytes.Length / $SECTOR_SIZE)

$bootx64Sector = 0
if ($hasEfi) {
    $bootx64Sector = $currentFileSector
    $currentFileSector += [int][Math]::Ceiling($bootx64Bytes.Length / $SECTOR_SIZE)
}

# ── Patch Boot Info Table na imagem limine-bios-cd.bin ────────────────────────
$pvdLbaBytes = [System.BitConverter]::GetBytes([uint32]$pvdSector)
[Array]::Copy($pvdLbaBytes, 0, $limineCdBytes, 8, 4)

$bootLbaBytes = [System.BitConverter]::GetBytes([uint32]$limineCdSector)
[Array]::Copy($bootLbaBytes, 0, $limineCdBytes, 12, 4)

$bootLenBytes = [System.BitConverter]::GetBytes([uint32]$limineCdBytes.Length)
[Array]::Copy($bootLenBytes, 0, $limineCdBytes, 16, 4)

for ($i = 24; $i -lt 64; $i++) {
    $limineCdBytes[$i] = 0
}

# 32-bit checksum dos dwords a partir do byte 64
$chkSum = [double]0
for ($i = 64; $i -lt $limineCdBytes.Length; $i += 4) {
    $dw = [System.BitConverter]::ToUInt32($limineCdBytes, $i)
    $chkSum = ($chkSum + $dw) % 4294967296
}
$bootChkBytes = [System.BitConverter]::GetBytes([uint32]$chkSum)
[Array]::Copy($bootChkBytes, 0, $limineCdBytes, 20, 4)

Write-Host ('[+] Boot Info Table patch aplicada (PVD LBA=' + $pvdSector + ', Boot LBA=' + $limineCdSector + ', Checksum=0x' + ([uint32]$chkSum).ToString("X8") + ')') -ForegroundColor Cyan

$totalSectors = $currentFileSector + 2
$totalBytes = $totalSectors * $SECTOR_SIZE
$iso = New-Object byte[] $totalBytes

$totKb = [Math]::Round($totalBytes / 1024, 2)
Write-Host ('[+] Construindo ISO9660 com ' + $totalSectors + ' setores (' + $totKb + ' KB)...') -ForegroundColor Yellow

function Write-DirRecord($buf, [int]$offset, [uint32]$extentSector, [uint32]$sizeBytes, [bool]$isDir, [byte[]]$nameBytes) {
    $recLen = (33 + $nameBytes.Length + 1) -band -bnot 1
    $buf[$offset + 0] = [byte]$recLen
    $buf[$offset + 1] = 0 # Ext attr length
    
    $extBoth = To-U32Both $extentSector
    [Array]::Copy($extBoth, 0, $buf, $offset + 2, 8)
    
    $szBoth = To-U32Both $sizeBytes
    [Array]::Copy($szBoth, 0, $buf, $offset + 10, 8)
    
    # Data: 2026-08-24 22:30:00 GMT
    $buf[$offset + 18] = 126 # 2026 - 1900
    $buf[$offset + 19] = 8   # Agosto
    $buf[$offset + 20] = 24  # Dia 24
    $buf[$offset + 21] = 22
    $buf[$offset + 22] = 30
    $buf[$offset + 23] = 0
    $buf[$offset + 24] = 0
    
    $buf[$offset + 25] = if ($isDir) { 0x02 } else { 0x00 }
    $buf[$offset + 26] = 0
    $buf[$offset + 27] = 0
    
    $vSeq = To-U16Both 1
    [Array]::Copy($vSeq, 0, $buf, $offset + 28, 4)
    
    $buf[$offset + 32] = [byte]$nameBytes.Length
    [Array]::Copy($nameBytes, 0, $buf, $offset + 33, $nameBytes.Length)
    
    return $recLen
}

# ── 1. Sector 16: Primary Volume Descriptor (PVD) ─────────────────────────────
$pvdOff = $pvdSector * $SECTOR_SIZE
$iso[$pvdOff + 0] = 0x01
[Array]::Copy([System.Text.Encoding]::ASCII.GetBytes("CD001"), 0, $iso, $pvdOff + 1, 5)
$iso[$pvdOff + 6] = 0x01

$sysId = To-PaddedAscii "LINUX" 32
[Array]::Copy($sysId, 0, $iso, $pvdOff + 8, 32)

$volId = To-PaddedAscii "JINN" 32
[Array]::Copy($volId, 0, $iso, $pvdOff + 40, 32)

$totBoth = To-U32Both $totalSectors
[Array]::Copy($totBoth, 0, $iso, $pvdOff + 80, 8)

$setBoth = To-U16Both 1
[Array]::Copy($setBoth, 0, $iso, $pvdOff + 120, 4)
[Array]::Copy($setBoth, 0, $iso, $pvdOff + 124, 4)

$blkBoth = To-U16Both $SECTOR_SIZE
[Array]::Copy($blkBoth, 0, $iso, $pvdOff + 128, 4)

$pathSzBoth = To-U32Both 64
[Array]::Copy($pathSzBoth, 0, $iso, $pvdOff + 132, 8)

$pTableLLe = [System.BitConverter]::GetBytes([uint32]$pathTableLSector)
[Array]::Copy($pTableLLe, 0, $iso, $pvdOff + 140, 4)

$pTableMBe = [System.BitConverter]::GetBytes([uint32]$pathTableMSector)
[Array]::Reverse($pTableMBe)
[Array]::Copy($pTableMBe, 0, $iso, $pvdOff + 148, 4)

# Root directory record no PVD (34 bytes)
Write-DirRecord $iso ($pvdOff + 156) $rootDirSector $SECTOR_SIZE $true (New-Object byte[] 1) | Out-Null

$setDesc = To-PaddedAscii "JINN_SET" 128
[Array]::Copy($setDesc, 0, $iso, $pvdOff + 190, 128)
$pubDesc = To-PaddedAscii "JINN_OS_TEAM" 128
[Array]::Copy($pubDesc, 0, $iso, $pvdOff + 318, 128)
$prepDesc = To-PaddedAscii "JINN_PREPARER" 128
[Array]::Copy($prepDesc, 0, $iso, $pvdOff + 446, 128)
$appDesc = To-PaddedAscii "JINN_APP" 128
[Array]::Copy($appDesc, 0, $iso, $pvdOff + 574, 128)
$iso[$pvdOff + 881] = 0x01

# ── 2. Sector 17: Boot Record Volume Descriptor (El Torito) ───────────────────
$brOff = $bootDescSector * $SECTOR_SIZE
$iso[$brOff + 0] = 0x00
[Array]::Copy([System.Text.Encoding]::ASCII.GetBytes("CD001"), 0, $iso, $brOff + 1, 5)
$iso[$brOff + 6] = 0x01
$elToritoId = To-PaddedAscii "EL TORITO SPECIFICATION" 32
[Array]::Copy($elToritoId, 0, $iso, $brOff + 7, 32)
$bootCatLe = [System.BitConverter]::GetBytes([uint32]$bootCatalogSector)
[Array]::Copy($bootCatLe, 0, $iso, $brOff + 71, 4)

# ── 3. Sector 18: Volume Descriptor Set Terminator ───────────────────────────
$termOff = $termSector * $SECTOR_SIZE
$iso[$termOff + 0] = 0xFF
[Array]::Copy([System.Text.Encoding]::ASCII.GetBytes("CD001"), 0, $iso, $termOff + 1, 5)
$iso[$termOff + 6] = 0x01

# ── 4. Sector 19: Boot Catalog (El Torito) ───────────────────────────────────
$catOff = $bootCatalogSector * $SECTOR_SIZE
# Validation Entry (32 bytes)
$iso[$catOff + 0] = 0x01 # Header ID
$iso[$catOff + 1] = 0x00 # 80x86
$catId = To-PaddedAscii "Limine Bootloader" 24
[Array]::Copy($catId, 0, $iso, $catOff + 4, 24)
$iso[$catOff + 30] = 0x55
$iso[$catOff + 31] = 0xAA

# Calculo de Checksum da Entrada de Validacao
$sum16 = [double]0
for ($i = 0; $i -lt 16; $i++) {
    if ($i -ne 14) {
        $w = [System.BitConverter]::ToUInt16($iso, $catOff + ($i * 2))
        $sum16 = ($sum16 + $w) % 65536
    }
}
$chk16 = [uint16]((65536 - $sum16) % 65536)
$chk16Bytes = [System.BitConverter]::GetBytes($chk16)
[Array]::Copy($chk16Bytes, 0, $iso, $catOff + 28, 2)

# Initial / Default Boot Entry (offset 32, 32 bytes)
$entryOff = $catOff + 32
$bootSectors512 = [uint16][Math]::Ceiling($limineCdBytes.Length / 512)
$iso[$entryOff + 0] = 0x88 # Bootable
$iso[$entryOff + 1] = 0x00 # No emulation
$iso[$entryOff + 2] = 0x00 # Load Segment (0x07C0 default)
$iso[$entryOff + 3] = 0x00
$iso[$entryOff + 4] = 0x00
$iso[$entryOff + 5] = 0x00
$sec512Bytes = [System.BitConverter]::GetBytes($bootSectors512)
[Array]::Copy($sec512Bytes, 0, $iso, $entryOff + 6, 2)
$rbaBytes = [System.BitConverter]::GetBytes([uint32]$limineCdSector)
[Array]::Copy($rbaBytes, 0, $iso, $entryOff + 8, 4)

# ── 5. Sectors 20 & 21: Path Tables (Type L e Type M) ─────────────────────────
$pL = New-Object System.IO.MemoryStream
$pM = New-Object System.IO.MemoryStream

function Add-PathTableEntry($ms, [string]$name, [uint32]$sector, [uint16]$parent, [bool]$isBe) {
    $nBytes = if ($name -eq "") { [byte[]]@(0) } else { [System.Text.Encoding]::ASCII.GetBytes($name) }
    $ms.WriteByte([byte]$nBytes.Length)
    $ms.WriteByte(0) # ext attr
    
    $secBytes = [System.BitConverter]::GetBytes($sector)
    if ($isBe) { [Array]::Reverse($secBytes) }
    $ms.Write($secBytes, 0, 4)
    
    $parBytes = [System.BitConverter]::GetBytes($parent)
    if ($isBe) { [Array]::Reverse($parBytes) }
    $ms.Write($parBytes, 0, 2)
    
    $ms.Write($nBytes, 0, $nBytes.Length)
    if (($nBytes.Length % 2) -ne 0) { $ms.WriteByte(0) } # padding
}

Add-PathTableEntry $pL "" $rootDirSector 1 $false
Add-PathTableEntry $pL "BOOT" $bootDirSector 1 $false
Add-PathTableEntry $pL "EFI" $efiDirSector 1 $false
Add-PathTableEntry $pL "BOOT" $efiBootDirSector 3 $false

Add-PathTableEntry $pM "" $rootDirSector 1 $true
Add-PathTableEntry $pM "BOOT" $bootDirSector 1 $true
Add-PathTableEntry $pM "EFI" $efiDirSector 1 $true
Add-PathTableEntry $pM "BOOT" $efiBootDirSector 3 $true

$pLBytes = $pL.ToArray(); [Array]::Copy($pLBytes, 0, $iso, ($pathTableLSector * $SECTOR_SIZE), $pLBytes.Length)
$pMBytes = $pM.ToArray(); [Array]::Copy($pMBytes, 0, $iso, ($pathTableMSector * $SECTOR_SIZE), $pMBytes.Length)

# ── 6. Sector 22: Root Directory ──────────────────────────────────────────────
$rOff = $rootDirSector * $SECTOR_SIZE
$cur = $rOff
$cur += Write-DirRecord $iso $cur $rootDirSector $SECTOR_SIZE $true (New-Object byte[] 1) # .
$dotdotName = [byte[]]@(1)
$cur += Write-DirRecord $iso $cur $rootDirSector $SECTOR_SIZE $true $dotdotName            # ..
$cur += Write-DirRecord $iso $cur $bootDirSector $SECTOR_SIZE $true ([System.Text.Encoding]::ASCII.GetBytes("BOOT"))
$cur += Write-DirRecord $iso $cur $efiDirSector $SECTOR_SIZE $true ([System.Text.Encoding]::ASCII.GetBytes("EFI"))

# Full Level 2 and 8.3 names for Limine compatibility
$cur += Write-DirRecord $iso $cur $kernelElfSector $kernelElfBytes.Length $false ([System.Text.Encoding]::ASCII.GetBytes("KERNEL.ELF;1"))
$cur += Write-DirRecord $iso $cur $limineCdSector $limineCdBytes.Length $false ([System.Text.Encoding]::ASCII.GetBytes("LIMINE-BIOS-CD.BIN;1"))
$cur += Write-DirRecord $iso $cur $limineCdSector $limineCdBytes.Length $false ([System.Text.Encoding]::ASCII.GetBytes("LIMINE_B.BIN;1"))
$cur += Write-DirRecord $iso $cur $limineSysSector $limineSysBytes.Length $false ([System.Text.Encoding]::ASCII.GetBytes("LIMINE-BIOS.SYS;1"))
$cur += Write-DirRecord $iso $cur $limineSysSector $limineSysBytes.Length $false ([System.Text.Encoding]::ASCII.GetBytes("LIMINE_B.SYS;1"))
$cur += Write-DirRecord $iso $cur $limineCfgSector $limineCfgBytes.Length $false ([System.Text.Encoding]::ASCII.GetBytes("LIMINE.CONF;1"))
$cur += Write-DirRecord $iso $cur $limineCfgSector $limineCfgBytes.Length $false ([System.Text.Encoding]::ASCII.GetBytes("LIMINE.CON;1"))

# ── 7. Sector 23: BOOT Directory ──────────────────────────────────────────────
$bOff = $bootDirSector * $SECTOR_SIZE
$cur = $bOff
$cur += Write-DirRecord $iso $cur $bootDirSector $SECTOR_SIZE $true (New-Object byte[] 1) # .
$cur += Write-DirRecord $iso $cur $rootDirSector $SECTOR_SIZE $true $dotdotName            # ..
$cur += Write-DirRecord $iso $cur $limineSysSector $limineSysBytes.Length $false ([System.Text.Encoding]::ASCII.GetBytes("LIMINE-BIOS.SYS;1"))
$cur += Write-DirRecord $iso $cur $limineCfgSector $limineCfgBytes.Length $false ([System.Text.Encoding]::ASCII.GetBytes("LIMINE.CONF;1"))

# ── 8. Sector 24: EFI Directory ───────────────────────────────────────────────
$eOff = $efiDirSector * $SECTOR_SIZE
$cur = $eOff
$cur += Write-DirRecord $iso $cur $efiDirSector $SECTOR_SIZE $true (New-Object byte[] 1)
$cur += Write-DirRecord $iso $cur $rootDirSector $SECTOR_SIZE $true $dotdotName
$cur += Write-DirRecord $iso $cur $efiBootDirSector $SECTOR_SIZE $true ([System.Text.Encoding]::ASCII.GetBytes("BOOT"))

# ── 9. Sector 25: EFI/BOOT Directory ──────────────────────────────────────────
$ebOff = $efiBootDirSector * $SECTOR_SIZE
$cur = $ebOff
$cur += Write-DirRecord $iso $cur $efiBootDirSector $SECTOR_SIZE $true (New-Object byte[] 1)
$cur += Write-DirRecord $iso $cur $efiDirSector $SECTOR_SIZE $true $dotdotName
if ($hasEfi) {
    $cur += Write-DirRecord $iso $cur $bootx64Sector $bootx64Bytes.Length $false ([System.Text.Encoding]::ASCII.GetBytes("BOOTX64.EFI;1"))
}

# ── 10. Copiar Conteudo dos Arquivos ──────────────────────────────────────────
[Array]::Copy($limineCdBytes, 0, $iso, ($limineCdSector * $SECTOR_SIZE), $limineCdBytes.Length)
Write-Host ('    [OK] LIMINE_B.BIN -> Setor ' + $limineCdSector + ' (' + $limineCdBytes.Length + ' bytes)')
[Array]::Copy($limineSysBytes, 0, $iso, ($limineSysSector * $SECTOR_SIZE), $limineSysBytes.Length)
Write-Host ('    [OK] LIMINE_B.SYS -> Setor ' + $limineSysSector + ' (' + $limineSysBytes.Length + ' bytes)')
[Array]::Copy($limineCfgBytes, 0, $iso, ($limineCfgSector * $SECTOR_SIZE), $limineCfgBytes.Length)
Write-Host ('    [OK] LIMINE.CON   -> Setor ' + $limineCfgSector + ' (' + $limineCfgBytes.Length + ' bytes)')
[Array]::Copy($kernelElfBytes, 0, $iso, ($kernelElfSector * $SECTOR_SIZE), $kernelElfBytes.Length)
Write-Host ('    [OK] KERNEL.ELF   -> Setor ' + $kernelElfSector + ' (' + $kernelElfBytes.Length + ' bytes)')

if ($hasEfi) {
    [Array]::Copy($bootx64Bytes, 0, $iso, ($bootx64Sector * $SECTOR_SIZE), $bootx64Bytes.Length)
    Write-Host ('    [OK] BOOTX64.EFI  -> Setor ' + $bootx64Sector + ' (' + $bootx64Bytes.Length + ' bytes)')
}

# ── Gravar ISO Final ──────────────────────────────────────────────────────────
[System.IO.File]::WriteAllBytes($isoPath, $iso)

$finalItem = Get-Item $isoPath
Write-Host ""
Write-Host "================================================================" -ForegroundColor Green
Write-Host "  [SUCESSO] ISO gerada com sucesso para VirtualBox / QEMU!" -ForegroundColor Green
Write-Host "  Arquivo : $($finalItem.FullName)" -ForegroundColor White
$finalSizeKb = [Math]::Round($finalItem.Length / 1024, 2)
Write-Host ('  Tamanho : ' + $finalSizeKb + ' KB (' + $finalItem.Length + ' bytes)') -ForegroundColor White
Write-Host "  Data    : $($finalItem.LastWriteTime)" -ForegroundColor White
Write-Host "================================================================" -ForegroundColor Green
