# scripts/make_disk.ps1 - Gera um disco virtual RAW (disk.img) de 64 MiB formatado com MBR Dual-Partition (FAT32 + Ext2)
param(
    [string]$DiskPath = "disk.img",
    [string]$AppElfPath = "iso_root\apps\ola-mundo.elf"
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $projectRoot

$targetDisk = Join-Path $projectRoot $DiskPath
Write-Host "Criando disco virtual Dual-Partition (FAT32 + Ext2) em: $targetDisk"

# Tamanho do disco: 64 MiB = 131072 setores de 512 bytes = 67108864 bytes
$totalSectors = 131072
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

# Texto do README.TXT da partição FAT32 (/disk)
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

# Texto do SERVER.TXT da partição Ext2 (/ext2)
$serverText = @"
=====================================================
  Jinn OS - Sistema de Arquivos Ext2 (Linux Native)
=====================================================

Volume Ext2 montado com sucesso em /ext2!
Este arquivo SERVER.TXT foi lido diretamente dos
inodes e blocos do sistema de arquivos Ext2.

Modo: Somente Leitura (Read-Only)
Integridade: Verificacao estrita de Inodes e blocos
Perfil recomendado: Simbad (Server) / Workstation

Experimente executar no shell ou terminal:
  jinn[K]> ls /ext2
  jinn[K]> cat /ext2/server.txt
  jinn[K]> fsinfo /ext2
"@
$serverBytes = [System.Text.Encoding]::ASCII.GetBytes($serverText)

# Criar o arquivo no disco com o tamanho total de 64 MiB
$fs = [System.IO.File]::Create($targetDisk)
$fs.SetLength($diskSizeBytes)
$bw = [System.IO.BinaryWriter]::new($fs)

function Write-BytesAt([long]$offset, [byte[]]$bytes) {
    $fs.Seek($offset, [System.IO.SeekOrigin]::Begin) | Out-Null
    $bw.Write($bytes)
}

# ----------------------------------------------------------------------
# 1. MBR (LBA 0) com 2 Partições
# ----------------------------------------------------------------------
$mbr = [byte[]]::new(512)

# --- Partição 1: FAT32 (Offset 446 / 0x1BE) ---
$part1Offset = 446
$mbr[$part1Offset + 0] = 0x80 # Bootable (Active)
$mbr[$part1Offset + 1] = 0x00 # CHS Start
$mbr[$part1Offset + 2] = 0x01
$mbr[$part1Offset + 3] = 0x01
$mbr[$part1Offset + 4] = 0x0C # Partition Type: FAT32 LBA
$mbr[$part1Offset + 5] = 0x00 # CHS End
$mbr[$part1Offset + 6] = 0x00
$mbr[$part1Offset + 7] = 0x00

$startLba1 = 2048 # LBA 2048 (1 MiB)
$part1Sectors = 63488 # 31 MiB
[System.BitConverter]::GetBytes([uint32]$startLba1).CopyTo($mbr, $part1Offset + 8)
[System.BitConverter]::GetBytes([uint32]$part1Sectors).CopyTo($mbr, $part1Offset + 12)

# --- Partição 2: Ext2 (Offset 462 / 0x1CE) ---
$part2Offset = 462
$mbr[$part2Offset + 0] = 0x00 # Non-bootable
$mbr[$part2Offset + 1] = 0x00 # CHS Start
$mbr[$part2Offset + 2] = 0x01
$mbr[$part2Offset + 3] = 0x01
$mbr[$part2Offset + 4] = 0x83 # Partition Type: Linux Native (Ext2)
$mbr[$part2Offset + 5] = 0x00 # CHS End
$mbr[$part2Offset + 6] = 0x00
$mbr[$part2Offset + 7] = 0x00

$startLba2 = 65536 # LBA 65536 (32 MiB)
$part2Sectors = 63488 # 31 MiB
[System.BitConverter]::GetBytes([uint32]$startLba2).CopyTo($mbr, $part2Offset + 8)
[System.BitConverter]::GetBytes([uint32]$part2Sectors).CopyTo($mbr, $part2Offset + 12)

# MBR Signature 0x55AA
$mbr[510] = 0x55
$mbr[511] = 0xAA
Write-BytesAt 0 $mbr

# ======================================================================
# 2. FORMATAÇÃO DA PARTIÇÃO 1: FAT32 (/disk)
# ======================================================================
$part1ByteOffset = [long]$startLba1 * $sectorSize
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
[System.BitConverter]::GetBytes([uint32]$startLba1).CopyTo($bpb, 28) # HiddSec = 2048
[System.BitConverter]::GetBytes([uint32]$part1Sectors).CopyTo($bpb, 32) # TotSec32 = 63488

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

Write-BytesAt $part1ByteOffset $bpb
Write-BytesAt ($part1ByteOffset + 6 * $sectorSize) $bpb

# FSInfo Sector (Setor 1 da partição FAT32)
$fsinfo = [byte[]]::new(512)
[System.BitConverter]::GetBytes([uint32]0x41615252).CopyTo($fsinfo, 0)   # LeadSig "RRaA"
[System.BitConverter]::GetBytes([uint32]0x61417272).CopyTo($fsinfo, 484) # StrucSig "rrAa"
[System.BitConverter]::GetBytes([uint32]::MaxValue).CopyTo($fsinfo, 488) # FreeCount desconhecido
[System.BitConverter]::GetBytes([uint32]5).CopyTo($fsinfo, 492)          # Next free cluster = 5
$fsinfo[510] = 0x55; $fsinfo[511] = 0xAA
Write-BytesAt ($part1ByteOffset + 1 * $sectorSize) $fsinfo

# Tabelas FAT 1 e FAT 2
$fatTable = [byte[]]::new($sectorsPerFat * $sectorSize)
function Set-FatEntry([int]$cluster, [uint32]$val) {
    [System.BitConverter]::GetBytes($val).CopyTo($fatTable, $cluster * 4)
}
Set-FatEntry 0 0x0FFFFFF8
Set-FatEntry 1 0x0FFFFFFF
Set-FatEntry 2 0x0FFFFFFF
Set-FatEntry 3 0x0FFFFFFF

$clusterBytes = 8 * 512 # 4096 bytes
$elfClustersNeeded = [Math]::Max(1, [int][Math]::Ceiling($elfBytes.Length / $clusterBytes))
for ($i = 0; $i -lt $elfClustersNeeded; $i++) {
    $currentClus = 4 + $i
    if ($i -eq $elfClustersNeeded - 1) {
        Set-FatEntry $currentClus 0x0FFFFFFF
    } else {
        Set-FatEntry $currentClus ([uint32]($currentClus + 1))
    }
}

$fat1Offset = $part1ByteOffset + [long]32 * $sectorSize
$fat2Offset = $part1ByteOffset + [long](32 + $sectorsPerFat) * $sectorSize
Write-BytesAt $fat1Offset $fatTable
Write-BytesAt $fat2Offset $fatTable

# Área de Dados FAT32
$dataStartLba1 = $startLba1 + 32 + 2 * $sectorsPerFat
$dataStartByteOffset1 = [long]$dataStartLba1 * $sectorSize

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

Add-DirEntry 0 "JINN DISK  " 0x08 0 0
Add-DirEntry 1 "README  TXT" 0x20 3 ($readmeBytes.Length)
Add-DirEntry 2 "OLA     ELF" 0x20 4 ($elfBytes.Length)

Write-BytesAt $dataStartByteOffset1 $rootDir
Write-BytesAt ($dataStartByteOffset1 + [long](3 - 2) * $clusterBytes) $readmeBytes
Write-BytesAt ($dataStartByteOffset1 + [long](4 - 2) * $clusterBytes) $elfBytes

# ======================================================================
# 3. FORMATAÇÃO DA PARTIÇÃO 2: Ext2 (/ext2)
# ======================================================================
$part2ByteOffset = [long]$startLba2 * $sectorSize
$ext2BlockSize = 1024
$ext2BlocksCount = 31744 # 63488 setores / 2 = 31744 blocos de 1024 bytes
$ext2InodesCount = 512

# 3.1. Superblock Ext2 (Byte 1024 da partição -> Bloco 1)
$sb = [byte[]]::new(1024)
[System.BitConverter]::GetBytes([uint32]$ext2InodesCount).CopyTo($sb, 0)       # s_inodes_count
[System.BitConverter]::GetBytes([uint32]$ext2BlocksCount).CopyTo($sb, 4)       # s_blocks_count
[System.BitConverter]::GetBytes([uint32]1000).CopyTo($sb, 8)                   # s_r_blocks_count
[System.BitConverter]::GetBytes([uint32]31600).CopyTo($sb, 12)                 # s_free_blocks_count
[System.BitConverter]::GetBytes([uint32]500).CopyTo($sb, 16)                   # s_free_inodes_count
[System.BitConverter]::GetBytes([uint32]1).CopyTo($sb, 20)                     # s_first_data_block = 1
[System.BitConverter]::GetBytes([uint32]0).CopyTo($sb, 24)                     # s_log_block_size = 0 (1024 bytes)
[System.BitConverter]::GetBytes([uint32]0).CopyTo($sb, 28)                     # s_log_frag_size = 0
[System.BitConverter]::GetBytes([uint32]$ext2BlocksCount).CopyTo($sb, 32)      # s_blocks_per_group
[System.BitConverter]::GetBytes([uint32]$ext2BlocksCount).CopyTo($sb, 36)      # s_frags_per_group
[System.BitConverter]::GetBytes([uint32]$ext2InodesCount).CopyTo($sb, 40)      # s_inodes_per_group
[System.BitConverter]::GetBytes([uint16]0xEF53).CopyTo($sb, 56)                # s_magic = 0xEF53
[System.BitConverter]::GetBytes([uint16]1).CopyTo($sb, 58)                     # s_state = 1 (Cleanly unmounted)
[System.BitConverter]::GetBytes([uint16]1).CopyTo($sb, 62)                     # s_minor_rev_level
[System.BitConverter]::GetBytes([uint32]1).CopyTo($sb, 76)                     # s_rev_level = 1 (Dynamic inodes)
[System.BitConverter]::GetBytes([uint16]11).CopyTo($sb, 84)                    # s_first_ino = 11
[System.BitConverter]::GetBytes([uint16]128).CopyTo($sb, 88)                   # s_inode_size = 128
[System.Text.Encoding]::ASCII.GetBytes("JINN_EXT2_VOL  ").CopyTo($sb, 120)     # s_volume_name
Write-BytesAt ($part2ByteOffset + 1024) $sb

# 3.2. Descritor de Grupo de Blocos (Bloco 2 -> Offset 2048 da partição)
$gd = [byte[]]::new(32)
[System.BitConverter]::GetBytes([uint32]3).CopyTo($gd, 0)                      # bg_block_bitmap = bloco 3
[System.BitConverter]::GetBytes([uint32]4).CopyTo($gd, 4)                      # bg_inode_bitmap = bloco 4
[System.BitConverter]::GetBytes([uint32]5).CopyTo($gd, 8)                      # bg_inode_table = bloco 5
[System.BitConverter]::GetBytes([uint16]31600).CopyTo($gd, 12)                # bg_free_blocks_count
[System.BitConverter]::GetBytes([uint16]500).CopyTo($gd, 14)                  # bg_free_inodes_count
[System.BitConverter]::GetBytes([uint16]1).CopyTo($gd, 16)                    # bg_used_dirs_count = 1
Write-BytesAt ($part2ByteOffset + 2048) $gd

# 3.3. Inode Table (Bloco 5 -> Offset 5 * 1024 = 5120 da partição)
$inodeTableOffset = $part2ByteOffset + 5120

# Inode 2: Diretório Raiz / (Offset 5120 + 1 * 128 = 5248)
$inoRoot = [byte[]]::new(128)
[System.BitConverter]::GetBytes([uint16]0x41ED).CopyTo($inoRoot, 0)            # mode: DIR | 0755
[System.BitConverter]::GetBytes([uint16]0).CopyTo($inoRoot, 2)                 # uid: 0
[System.BitConverter]::GetBytes([uint32]1024).CopyTo($inoRoot, 4)              # size: 1024 bytes (1 bloco)
[System.BitConverter]::GetBytes([uint32]1700000000).CopyTo($inoRoot, 8)        # atime
[System.BitConverter]::GetBytes([uint32]1700000000).CopyTo($inoRoot, 12)       # ctime
[System.BitConverter]::GetBytes([uint32]1700000000).CopyTo($inoRoot, 16)       # mtime
[System.BitConverter]::GetBytes([uint16]2).CopyTo($inoRoot, 26)                # links_count = 2 (. e ..)
[System.BitConverter]::GetBytes([uint32]2).CopyTo($inoRoot, 28)                # blocks = 2 setores de 512B
[System.BitConverter]::GetBytes([uint32]70).CopyTo($inoRoot, 40)               # direct_blocks[0] = bloco 70
Write-BytesAt ($inodeTableOffset + 128) $inoRoot

# Inode 11: Arquivo SERVER.TXT (Offset 5120 + 10 * 128 = 6400)
$inoServer = [byte[]]::new(128)
[System.BitConverter]::GetBytes([uint16]0x81A4).CopyTo($inoServer, 0)          # mode: REG | 0644
[System.BitConverter]::GetBytes([uint16]0).CopyTo($inoServer, 2)               # uid: 0
[System.BitConverter]::GetBytes([uint32]$serverBytes.Length).CopyTo($inoServer, 4) # size
[System.BitConverter]::GetBytes([uint32]1700000000).CopyTo($inoServer, 8)      # atime
[System.BitConverter]::GetBytes([uint32]1700000000).CopyTo($inoServer, 12)     # ctime
[System.BitConverter]::GetBytes([uint32]1700000000).CopyTo($inoServer, 16)     # mtime
[System.BitConverter]::GetBytes([uint16]1).CopyTo($inoServer, 26)              # links_count = 1
$secCount = [uint32]([Math]::Max(2, [Math]::Ceiling($serverBytes.Length / 512)))
[System.BitConverter]::GetBytes($secCount).CopyTo($inoServer, 28)              # blocks em setores de 512B
[System.BitConverter]::GetBytes([uint32]71).CopyTo($inoServer, 40)             # direct_blocks[0] = bloco 71
Write-BytesAt ($inodeTableOffset + 10 * 128) $inoServer

# 3.4. Bloco 70: Conteúdo do Diretório Raiz Ext2 (Offset 70 * 1024)
$dirBlock = [byte[]]::new(1024)
# Entrada 1: "." (ino = 2, rec_len = 12, name_len = 1, file_type = 2)
[System.BitConverter]::GetBytes([uint32]2).CopyTo($dirBlock, 0)
[System.BitConverter]::GetBytes([uint16]12).CopyTo($dirBlock, 4)
$dirBlock[6] = 1 # name_len
$dirBlock[7] = 2 # file_type = DIR
[System.Text.Encoding]::ASCII.GetBytes(".").CopyTo($dirBlock, 8)

# Entrada 2: ".." (ino = 2, rec_len = 12, name_len = 2, file_type = 2)
[System.BitConverter]::GetBytes([uint32]2).CopyTo($dirBlock, 12)
[System.BitConverter]::GetBytes([uint16]12).CopyTo($dirBlock, 16)
$dirBlock[18] = 2 # name_len
$dirBlock[19] = 2 # file_type = DIR
[System.Text.Encoding]::ASCII.GetBytes("..").CopyTo($dirBlock, 20)

# Entrada 3: "SERVER.TXT" (ino = 11, rec_len = 1000, name_len = 10, file_type = 1)
$recLen3 = [uint16](1024 - 12 - 12) # 1000 bytes preenche até o final do bloco
[System.BitConverter]::GetBytes([uint32]11).CopyTo($dirBlock, 24)
[System.BitConverter]::GetBytes($recLen3).CopyTo($dirBlock, 28)
$dirBlock[30] = 10 # name_len
$dirBlock[31] = 1  # file_type = REG
[System.Text.Encoding]::ASCII.GetBytes("SERVER.TXT").CopyTo($dirBlock, 32)

Write-BytesAt ($part2ByteOffset + 70 * 1024) $dirBlock

# 3.5. Bloco 71: Dados de SERVER.TXT (Offset 71 * 1024)
Write-BytesAt ($part2ByteOffset + 71 * 1024) $serverBytes

$bw.Flush()
$bw.Close()
$fs.Close()

Write-Host "Disco virtual criado com sucesso em: $targetDisk"
Write-Host "  - Particao 1 (FAT32): LBA $startLba1 (31 MiB) -> Arquivos: README.TXT, OLA.ELF"
Write-Host "  - Particao 2 (Ext2):  LBA $startLba2 (31 MiB) -> Arquivos: SERVER.TXT (Read-Only)"
