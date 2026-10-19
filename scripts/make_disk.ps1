# scripts/make_disk.ps1 - Gera um disco virtual RAW (disk.img) de 32 MiB formatado em FAT32 com MBR
param(
    [string]$DiskPath = "disk.img",
    [string]$AppElfPath = "iso_root\apps\ola-mundo.elf"
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $projectRoot

$targetDisk = Join-Path $projectRoot $DiskPath
Write-Host "Criando disco virtual FAT32 em: $targetDisk"

# Tamanho do disco: 32 MiB = 65536 setores de 512 bytes = 33554432 bytes
$totalSectors = 65536
$sectorSize = 512
$diskSizeBytes = [long]$totalSectors * $sectorSize

# Lê o binário de usuário (ola-mundo.elf)
$elfBytes = @()
$resolvedElf = Join-Path $projectRoot $AppElfPath
if (Test-Path $resolvedElf) {
    $elfBytes = [System.IO.File]::ReadAllBytes($resolvedElf)
    Write-Host "Binário de usuário encontrado: $resolvedElf ($($elfBytes.Length) bytes)"
} else {
    Write-Host "Aviso: $resolvedElf não encontrado. Usando payload dummy de 512 bytes."
    $elfBytes = [byte[]]::new(512)
}

# Texto do README.TXT
$readmeText = @"
=====================================================
  Jinn OS - Sistema de Arquivos FAT32 (Modo PIO IDE)
=====================================================

Parabens! A particao FAT32 foi montada com sucesso em /disk.
Este arquivo README.TXT foi lido diretamente dos clusters
do disco rigido virtual usando o driver de VFS.

Executaveis disponiveis nesta particao:
  - /disk/ola.elf (Processo de espaco de usuario Ring 3)

Experimente executar no shell do Jinn:
  jinn[K]> ls /disk
  jinn[K]> cat /disk/readme.txt
  jinn[K]> spawn /disk/ola.elf
  jinn[K]> ps
"@
$readmeBytes = [System.Text.Encoding]::ASCII.GetBytes($readmeText)

# Criar o arquivo no disco com o tamanho total
$fs = [System.IO.File]::Create($targetDisk)
$fs.SetLength($diskSizeBytes)
$bw = [System.IO.BinaryWriter]::new($fs)

function Write-BytesAt([long]$offset, [byte[]]$bytes) {
    $fs.Seek($offset, [System.IO.SeekOrigin]::Begin) | Out-Null
    $bw.Write($bytes)
}

# ----------------------------------------------------------------------
# 1. MBR (LBA 0)
# ----------------------------------------------------------------------
$mbr = [byte[]]::new(512)
# Partição 1 no offset 446 (0x1BE)
$part1Offset = 446
$mbr[$part1Offset + 0] = 0x80 # Bootable (Active)
$mbr[$part1Offset + 1] = 0x00 # CHS Start
$mbr[$part1Offset + 2] = 0x01
$mbr[$part1Offset + 3] = 0x01
$mbr[$part1Offset + 4] = 0x0C # Partition Type: FAT32 LBA
$mbr[$part1Offset + 5] = 0x00 # CHS End
$mbr[$part1Offset + 6] = 0x00
$mbr[$part1Offset + 7] = 0x00

# Start LBA = 2048 (0x00000800)
$startLba = 2048
[System.BitConverter]::GetBytes([uint32]$startLba).CopyTo($mbr, $part1Offset + 8)

# Sector Count = 65536 - 2048 = 63488 (0x0000F800)
$partSectors = $totalSectors - $startLba
[System.BitConverter]::GetBytes([uint32]$partSectors).CopyTo($mbr, $part1Offset + 12)

# MBR Signature 0x55AA
$mbr[510] = 0x55
$mbr[511] = 0xAA
Write-BytesAt 0 $mbr

# ----------------------------------------------------------------------
# 2. FAT32 Boot Sector / BPB (LBA 2048 -> Offset 1,048,576)
# ----------------------------------------------------------------------
$partByteOffset = [long]$startLba * $sectorSize
$bpb = [byte[]]::new(512)

# JMP boot
$bpb[0] = 0xEB; $bpb[1] = 0x58; $bpb[2] = 0x90
[System.Text.Encoding]::ASCII.GetBytes("MSWIN4.1").CopyTo($bpb, 3)

[System.BitConverter]::GetBytes([uint16]512).CopyTo($bpb, 11)   # BytsPerSec = 512
$bpb[13] = 8                                                    # SecPerClus = 8 (4096 bytes)
[System.BitConverter]::GetBytes([uint16]32).CopyTo($bpb, 14)    # RsvdSecCnt = 32
$bpb[16] = 2                                                    # NumFATs = 2
[System.BitConverter]::GetBytes([uint16]0).CopyTo($bpb, 17)     # RootEntCnt = 0 (FAT32)
[System.BitConverter]::GetBytes([uint16]0).CopyTo($bpb, 19)     # TotSec16 = 0
$bpb[21] = 0xF8                                                 # Media = Fixed disk
[System.BitConverter]::GetBytes([uint16]0).CopyTo($bpb, 22)     # FATSz16 = 0
[System.BitConverter]::GetBytes([uint16]63).CopyTo($bpb, 24)    # SecPerTrk = 63
[System.BitConverter]::GetBytes([uint16]255).CopyTo($bpb, 26)   # NumHeads = 255
[System.BitConverter]::GetBytes([uint32]$startLba).CopyTo($bpb, 28) # HiddSec = 2048
[System.BitConverter]::GetBytes([uint32]$partSectors).CopyTo($bpb, 32) # TotSec32 = 63488

# Campos FAT32
$sectorsPerFat = 256
[System.BitConverter]::GetBytes([uint32]$sectorsPerFat).CopyTo($bpb, 36) # FATSz32 = 256
[System.BitConverter]::GetBytes([uint16]0).CopyTo($bpb, 40)              # ExtFlags = 0
[System.BitConverter]::GetBytes([uint16]0).CopyTo($bpb, 42)              # FSVer = 0
[System.BitConverter]::GetBytes([uint32]2).CopyTo($bpb, 44)              # RootClus = 2
[System.BitConverter]::GetBytes([uint16]1).CopyTo($bpb, 48)              # FSInfo = 1
[System.BitConverter]::GetBytes([uint16]6).CopyTo($bpb, 50)              # BkBootSec = 6
$bpb[64] = 0x80                                                          # DrvNum = 0x80
$bpb[66] = 0x29                                                          # BootSig = 0x29
[System.BitConverter]::GetBytes([uint32]0x12345678).CopyTo($bpb, 67)     # VolID
[System.Text.Encoding]::ASCII.GetBytes("JINN DISK  ").CopyTo($bpb, 71)   # VolLab
[System.Text.Encoding]::ASCII.GetBytes("FAT32   ").CopyTo($bpb, 82)      # FilSysType
$bpb[510] = 0x55; $bpb[511] = 0xAA                                      # Boot signature

Write-BytesAt $partByteOffset $bpb
# Escreve também o backup do boot sector no setor 6
Write-BytesAt ($partByteOffset + 6 * $sectorSize) $bpb

# ----------------------------------------------------------------------
# 3. FSInfo Sector (LBA 2049 -> Setor 1 da partição)
# ----------------------------------------------------------------------
$fsinfo = [byte[]]::new(512)
[System.BitConverter]::GetBytes([uint32]0x41615252).CopyTo($fsinfo, 0)   # LeadSig "RRaA"
[System.BitConverter]::GetBytes([uint32]0x61417272).CopyTo($fsinfo, 484) # StrucSig "rrAa"
[System.BitConverter]::GetBytes([uint32]::MaxValue).CopyTo($fsinfo, 488) # FreeCount desconhecido
[System.BitConverter]::GetBytes([uint32]5).CopyTo($fsinfo, 492)          # Next free cluster = 5
$fsinfo[510] = 0x55; $fsinfo[511] = 0xAA
Write-BytesAt ($partByteOffset + 1 * $sectorSize) $fsinfo

# ----------------------------------------------------------------------
# 4. Tabelas FAT 1 e FAT 2 (Setores 32 e 32 + 256)
# ----------------------------------------------------------------------
# Cada entrada FAT32 tem 4 bytes.
$fatTable = [byte[]]::new($sectorsPerFat * $sectorSize)

function Set-FatEntry([int]$cluster, [uint32]$val) {
    [System.BitConverter]::GetBytes($val).CopyTo($fatTable, $cluster * 4)
}

# Cluster 0: Media low byte + 0x0FFFFF00 -> 0x0FFFFFF8
Set-FatEntry 0 0x0FFFFFF8
# Cluster 1: EOF marker -> 0x0FFFFFFF
Set-FatEntry 1 0x0FFFFFFF
# Cluster 2 (Root directory): Fim de cadeia (1 cluster é suficiente)
Set-FatEntry 2 0x0FFFFFFF
# Cluster 3 (README.TXT): Fim de cadeia
Set-FatEntry 3 0x0FFFFFFF

# Cluster 4..N (OLA.ELF): Se o ELF tiver mais que 4096 bytes, encadeia os clusters
$clusterBytes = 8 * 512 # 4096 bytes
$elfClustersNeeded = [Math]::Max(1, [int][Math]::Ceiling($elfBytes.Length / $clusterBytes))
for ($i = 0; $i -lt $elfClustersNeeded; $i++) {
    $currentClus = 4 + $i
    if ($i -eq $elfClustersNeeded - 1) {
        Set-FatEntry $currentClus 0x0FFFFFFF # Último cluster
    } else {
        Set-FatEntry $currentClus [uint32]($currentClus + 1)
    }
}

$fat1Offset = $partByteOffset + [long]32 * $sectorSize
$fat2Offset = $partByteOffset + [long](32 + $sectorsPerFat) * $sectorSize
Write-BytesAt $fat1Offset $fatTable
Write-BytesAt $fat2Offset $fatTable

# ----------------------------------------------------------------------
# 5. Área de Dados: Cluster 2 (Root Directory), Cluster 3 (README), Cluster 4 (OLA.ELF)
# ----------------------------------------------------------------------
# Data Start LBA = 2048 + 32 + 2 * 256 = 2592
$dataStartLba = $startLba + 32 + 2 * $sectorsPerFat
$dataStartByteOffset = [long]$dataStartLba * $sectorSize

# Cluster 2: Diretório Raiz (4096 bytes)
$rootDir = [byte[]]::new($clusterBytes)

function Add-DirEntry([int]$index, [string]$name11, [byte]$attr, [uint32]$firstCluster, [uint32]$size) {
    $offset = $index * 32
    $nameBytes = [System.Text.Encoding]::ASCII.GetBytes($name11.PadRight(11).Substring(0, 11))
    $nameBytes.CopyTo($rootDir, $offset)
    $rootDir[$offset + 11] = $attr
    
    $clusHi = [uint16]($firstCluster -shr 16)
    $clusLo = [uint16]($firstCluster -band 0xFFFF)
    [System.BitConverter]::GetBytes($clusHi).CopyTo($rootDir, $offset + 20)
    [System.BitConverter]::GetBytes($clusLo).CopyTo($rootDir, $offset + 26)
    [System.BitConverter]::GetBytes($size).CopyTo($rootDir, $offset + 28)
}

# Entrada 0: Volume ID
Add-DirEntry 0 "JINN DISK  " 0x08 0 0
# Entrada 1: README.TXT (Cluster 3)
Add-DirEntry 1 "README  TXT" 0x20 3 ($readmeBytes.Length)
# Entrada 2: OLA.ELF (Cluster 4)
Add-DirEntry 2 "OLA     ELF" 0x20 4 ($elfBytes.Length)

Write-BytesAt $dataStartByteOffset $rootDir

# Cluster 3: Dados de README.TXT
$cluster3Offset = $dataStartByteOffset + [long](3 - 2) * $clusterBytes
Write-BytesAt $cluster3Offset $readmeBytes

# Cluster 4: Dados de OLA.ELF
$cluster4Offset = $dataStartByteOffset + [long](4 - 2) * $clusterBytes
Write-BytesAt $cluster4Offset $elfBytes

$bw.Flush()
$bw.Close()
$fs.Close()

Write-Host "Disco virtual criado com sucesso em: $targetDisk"
Write-Host "  - Particao FAT32: LBA 2048 (31 MiB)"
Write-Host "  - Arquivos: README.TXT ($($readmeBytes.Length) bytes), OLA.ELF ($($elfBytes.Length) bytes)"
