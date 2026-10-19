#!/usr/bin/env python3
"""
scripts/make_disk.py - Gera um disco virtual RAW (disk.img) de 32 MiB formatado em FAT32 com MBR
"""

import os
import struct

def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.abspath(os.path.join(script_dir, ".."))
    disk_path = os.path.join(project_root, "disk.img")
    app_elf_path = os.path.join(project_root, "iso_root", "apps", "ola-mundo.elf")

    print(f"Criando disco virtual FAT32 em: {disk_path}")

    total_sectors = 65536
    sector_size = 512
    disk_size = total_sectors * sector_size
    start_lba = 2048
    part_sectors = total_sectors - start_lba
    sectors_per_fat = 256
    cluster_size = 8 * sector_size  # 4096 bytes

    # Lê o binário de usuário ola-mundo.elf
    elf_bytes = b""
    if os.path.exists(app_elf_path):
        with open(app_elf_path, "rb") as f:
            elf_bytes = f.read()
        print(f"Binario de usuario encontrado: {app_elf_path} ({len(elf_bytes)} bytes)")
    else:
        elf_bytes = b"\x00" * 512
        print("Aviso: ola-mundo.elf nao encontrado. Usando payload dummy.")

    readme_text = (
        "=====================================================\n"
        "  Jinn OS - Sistema de Arquivos FAT32 (Modo PIO IDE)\n"
        "=====================================================\n\n"
        "Parabens! A particao FAT32 foi montada com sucesso em /disk.\n"
        "Este arquivo README.TXT foi lido diretamente dos clusters\n"
        "do disco rigido virtual usando o driver de VFS.\n\n"
        "Executaveis disponiveis nesta particao:\n"
        "  - /disk/ola.elf (Processo de espaco de usuario Ring 3)\n\n"
        "Experimente executar no shell do Jinn:\n"
        "  jinn[K]> ls /disk\n"
        "  jinn[K]> cat /disk/readme.txt\n"
        "  jinn[K]> spawn /disk/ola.elf\n"
        "  jinn[K]> ps\n"
    ).encode("ascii")

    with open(disk_path, "wb") as f:
        # Prealoca tamanho total
        f.truncate(disk_size)

        # 1. MBR (LBA 0)
        mbr = bytearray(512)
        # Partição 1 no offset 446 (0x1BE)
        p_offset = 446
        mbr[p_offset + 0] = 0x80  # Bootable
        mbr[p_offset + 1] = 0x00
        mbr[p_offset + 2] = 0x01
        mbr[p_offset + 3] = 0x01
        mbr[p_offset + 4] = 0x0C  # FAT32 LBA
        mbr[p_offset + 5] = 0x00
        mbr[p_offset + 6] = 0x00
        mbr[p_offset + 7] = 0x00
        struct.pack_into("<I", mbr, p_offset + 8, start_lba)
        struct.pack_into("<I", mbr, p_offset + 12, part_sectors)
        mbr[510] = 0x55
        mbr[511] = 0xAA

        f.seek(0)
        f.write(mbr)

        # 2. Boot Sector / BPB (LBA 2048)
        part_offset = start_lba * sector_size
        bpb = bytearray(512)
        bpb[0:3] = b"\xEB\x58\x90"
        bpb[3:11] = b"MSWIN4.1"
        struct.pack_into("<H", bpb, 11, 512)  # BytsPerSec
        bpb[13] = 8                           # SecPerClus
        struct.pack_into("<H", bpb, 14, 32)   # RsvdSecCnt
        bpb[16] = 2                           # NumFATs
        struct.pack_into("<H", bpb, 17, 0)    # RootEntCnt
        struct.pack_into("<H", bpb, 19, 0)    # TotSec16
        bpb[21] = 0xF8                        # Media
        struct.pack_into("<H", bpb, 22, 0)    # FATSz16
        struct.pack_into("<H", bpb, 24, 63)   # SecPerTrk
        struct.pack_into("<H", bpb, 26, 255)  # NumHeads
        struct.pack_into("<I", bpb, 28, start_lba)     # HiddSec
        struct.pack_into("<I", bpb, 32, part_sectors)  # TotSec32
        struct.pack_into("<I", bpb, 36, sectors_per_fat)  # FATSz32
        struct.pack_into("<H", bpb, 40, 0)    # ExtFlags
        struct.pack_into("<H", bpb, 42, 0)    # FSVer
        struct.pack_into("<I", bpb, 44, 2)    # RootClus = 2
        struct.pack_into("<H", bpb, 48, 1)    # FSInfo = 1
        struct.pack_into("<H", bpb, 50, 6)    # BkBootSec = 6
        bpb[64] = 0x80                        # DrvNum
        bpb[66] = 0x29                        # BootSig
        struct.pack_into("<I", bpb, 67, 0x12345678)
        bpb[71:82] = b"JINN DISK  "
        bpb[82:90] = b"FAT32   "
        bpb[510] = 0x55
        bpb[511] = 0xAA

        f.seek(part_offset)
        f.write(bpb)
        f.seek(part_offset + 6 * sector_size)
        f.write(bpb)

        # 3. FSInfo (LBA 2049)
        fsinfo = bytearray(512)
        struct.pack_into("<I", fsinfo, 0, 0x41615252)
        struct.pack_into("<I", fsinfo, 484, 0x61417272)
        struct.pack_into("<I", fsinfo, 488, 0xFFFFFFFF)
        struct.pack_into("<I", fsinfo, 492, 5)
        fsinfo[510] = 0x55
        fsinfo[511] = 0xAA

        f.seek(part_offset + sector_size)
        f.write(fsinfo)

        # 4. Tabelas FAT 1 e FAT 2 (Setor 32)
        fat_table = bytearray(sectors_per_fat * sector_size)
        struct.pack_into("<I", fat_table, 0, 0x0FFFFFF8)  # Clus 0
        struct.pack_into("<I", fat_table, 4, 0x0FFFFFFF)  # Clus 1
        struct.pack_into("<I", fat_table, 8, 0x0FFFFFFF)  # Clus 2 (Root)
        struct.pack_into("<I", fat_table, 12, 0x0FFFFFFF) # Clus 3 (README)

        elf_clusters_needed = max(1, (len(elf_bytes) + cluster_size - 1) // cluster_size)
        for i in range(elf_clusters_needed):
            curr_c = 4 + i
            val = 0x0FFFFFFF if i == elf_clusters_needed - 1 else curr_c + 1
            struct.pack_into("<I", fat_table, curr_c * 4, val)

        fat1_offset = part_offset + 32 * sector_size
        fat2_offset = part_offset + (32 + sectors_per_fat) * sector_size
        f.seek(fat1_offset)
        f.write(fat_table)
        f.seek(fat2_offset)
        f.write(fat_table)

        # 5. Diretório Raiz (Cluster 2)
        data_start_offset = part_offset + (32 + 2 * sectors_per_fat) * sector_size
        root_dir = bytearray(cluster_size)

        def add_entry(index, name11, attr, first_clus, size):
            off = index * 32
            root_dir[off:off+11] = name11.encode("ascii")
            root_dir[off+11] = attr
            clus_hi = (first_clus >> 16) & 0xFFFF
            clus_lo = first_clus & 0xFFFF
            struct.pack_into("<H", root_dir, off + 20, clus_hi)
            struct.pack_into("<H", root_dir, off + 26, clus_lo)
            struct.pack_into("<I", root_dir, off + 28, size)

        add_entry(0, "JINN DISK  ", 0x08, 0, 0)
        add_entry(1, "README  TXT", 0x20, 3, len(readme_text))
        add_entry(2, "OLA     ELF", 0x20, 4, len(elf_bytes))

        f.seek(data_start_offset)
        f.write(root_dir)

        # Cluster 3: README.TXT
        f.seek(data_start_offset + (3 - 2) * cluster_size)
        f.write(readme_text)

        # Cluster 4: OLA.ELF
        f.seek(data_start_offset + (4 - 2) * cluster_size)
        f.write(elf_bytes)

    print(f"Disco virtual criado com sucesso em: {disk_path}")
    print(f"  - Particao FAT32: LBA 2048 (31 MiB)")
    print(f"  - Arquivos: README.TXT ({len(readme_text)} bytes), OLA.ELF ({len(elf_bytes)} bytes)")

if __name__ == "__main__":
    main()
