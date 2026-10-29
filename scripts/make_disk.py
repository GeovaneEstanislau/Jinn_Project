#!/usr/bin/env python3
"""
scripts/make_disk.py - Gera um disco virtual RAW (disk.img) de 64 MiB formatado com MBR Dual-Partition (FAT32 + Ext2)
Alinhado rigorosamente com scripts/make_disk.ps1
"""

import os
import struct
import sys

def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.abspath(os.path.join(script_dir, ".."))
    disk_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(project_root, "disk.img")
    ola_elf_path = os.path.join(project_root, "iso_root", "apps", "ola-mundo.elf")
    browser_elf_path = os.path.join(project_root, "iso_root", "apps", "browser.elf")

    print(f"Criando disco virtual Dual-Partition (FAT32 + Ext2) em: {disk_path}")

    # Tamanho do disco: 64 MiB = 131072 setores de 512 bytes = 67108864 bytes
    total_sectors = 131072
    sector_size = 512
    disk_size = total_sectors * sector_size

    # Partição 1: FAT32
    start_lba1 = 2048
    part1_sectors = 63488  # 31 MiB
    sectors_per_fat = 256
    cluster_size = 8 * sector_size  # 4096 bytes

    # Partição 2: Ext2
    start_lba2 = 65536
    part2_sectors = 63488  # 31 MiB
    ext2_block_size = 1024
    ext2_blocks_count = 31744  # 63488 setores / 2 = 31744 blocos de 1024 bytes
    ext2_inodes_count = 512

    # Lê binário de usuário ola-mundo.elf
    ola_bytes = b""
    if os.path.exists(ola_elf_path):
        with open(ola_elf_path, "rb") as f:
            ola_bytes = f.read()
        print(f"Binario ola-mundo encontrado: {ola_elf_path} ({len(ola_bytes)} bytes)")
    else:
        ola_bytes = b"\x00" * 512
        print("Aviso: ola-mundo.elf nao encontrado. Usando payload dummy.")

    # Lê binário de usuário browser.elf
    browser_bytes = b""
    if os.path.exists(browser_elf_path):
        with open(browser_elf_path, "rb") as f:
            browser_bytes = f.read()
        print(f"Binario browser encontrado: {browser_elf_path} ({len(browser_bytes)} bytes)")
    elif len(ola_bytes) > 0:
        browser_bytes = ola_bytes
        print(f"Aviso: browser.elf nao encontrado. Usando ola-mundo.elf ({len(browser_bytes)} bytes)")
    else:
        browser_bytes = b"\x00" * 512
        print("Aviso: browser.elf nao encontrado. Usando payload dummy.")

    readme_text = (
        "=====================================================\n"
        "  Jinn OS - Sistema de Arquivos FAT32 (Modo PIO IDE)\n"
        "=====================================================\n\n"
        "Parabens! A particao FAT32 foi montada com sucesso em /disk.\n"
        "Este arquivo README.TXT foi lido diretamente dos clusters\n"
        "do disco rigido virtual usando o driver de VFS.\n\n"
        "Executaveis disponiveis nesta particao:\n"
        "  - /disk/ola.elf (Processo de espaco de usuario Ring 3)\n"
        "  - /disk/browser.elf (Jinn Browser Engine v0.2.0)\n\n"
        "Experimente executar no shell do Jinn:\n"
        "  jinn[K]> ls /disk\n"
        "  jinn[K]> cat /disk/readme.txt\n"
        "  jinn[K]> spawn /disk/ola.elf\n"
        "  jinn[K]> spawn /disk/browser.elf\n"
        "  jinn[K]> ps"
    ).encode("ascii")

    server_text = (
        "=====================================================\n"
        "  Jinn OS - Sistema de Arquivos Ext2 (Linux Native)\n"
        "=====================================================\n\n"
        "Volume Ext2 montado com sucesso em /ext2!\n"
        "Este arquivo SERVER.TXT foi lido diretamente dos\n"
        "inodes e blocos do sistema de arquivos Ext2.\n\n"
        "Modo: Somente Leitura (Read-Only)\n"
        "Integridade: Verificacao estrita de Inodes e blocos\n"
        "Perfil recomendado: Simbad (Server) / Workstation\n\n"
        "Experimente executar no shell ou terminal:\n"
        "  jinn[K]> ls /ext2\n"
        "  jinn[K]> cat /ext2/server.txt\n"
        "  jinn[K]> fsinfo /ext2"
    ).encode("ascii")

    index_html = (
        b"<!DOCTYPE html>\n<html>\n<head><title>Jinn OS Portal</title></head>\n"
        b"<body>\n  <h1>Jinn Browser 0.2.0</h1>\n"
        b"  <p>Bem-vindo ao Jinn Browser rodando nativamente sobre o microkernel Jinn OS e Aladdin Desktop Compositor.</p>\n"
        b"  <p><a href=\"about.htm\">Sobre o Sistema</a></p>\n</body>\n</html>"
    )
    about_html = (
        b"<!DOCTYPE html>\n<html>\n<head><title>Sobre o Jinn</title></head>\n"
        b"<body>\n  <h1>Sobre o Jinn OS</h1>\n"
        b"  <p>Microkernel x86_64 com isolamento Ring 3, IPC sincrono e assincrono e composicao visual acelerada pelo Aladdin.</p>\n"
        b"  <p><a href=\"index.htm\">Voltar ao Inicio</a></p>\n</body>\n</html>"
    )

    with open(disk_path, "wb") as f:
        # Prealoca tamanho total (64 MiB)
        f.truncate(disk_size)

        # ------------------------------------------------------------------
        # 1. MBR (LBA 0) com 2 Partições
        # ------------------------------------------------------------------
        mbr = bytearray(512)
        # Partição 1: FAT32 LBA no offset 446 (0x1BE)
        p1 = 446
        mbr[p1 + 0] = 0x80  # Bootable
        mbr[p1 + 1] = 0x00
        mbr[p1 + 2] = 0x01
        mbr[p1 + 3] = 0x01
        mbr[p1 + 4] = 0x0C  # FAT32 LBA
        mbr[p1 + 5] = 0x00
        mbr[p1 + 6] = 0x00
        mbr[p1 + 7] = 0x00
        struct.pack_into("<I", mbr, p1 + 8, start_lba1)
        struct.pack_into("<I", mbr, p1 + 12, part1_sectors)

        # Partição 2: Linux Native (Ext2) no offset 462 (0x1CE)
        p2 = 462
        mbr[p2 + 0] = 0x00  # Non-bootable
        mbr[p2 + 1] = 0x00
        mbr[p2 + 2] = 0x01
        mbr[p2 + 3] = 0x01
        mbr[p2 + 4] = 0x83  # Linux Native Ext2
        mbr[p2 + 5] = 0x00
        mbr[p2 + 6] = 0x00
        mbr[p2 + 7] = 0x00
        struct.pack_into("<I", mbr, p2 + 8, start_lba2)
        struct.pack_into("<I", mbr, p2 + 12, part2_sectors)

        mbr[510] = 0x55
        mbr[511] = 0xAA
        f.seek(0)
        f.write(mbr)

        # ------------------------------------------------------------------
        # 2. FORMATAÇÃO DA PARTIÇÃO 1: FAT32 (/disk)
        # ------------------------------------------------------------------
        part1_offset = start_lba1 * sector_size
        bpb = bytearray(512)
        bpb[0:3] = b"\xEB\x58\x90"
        bpb[3:11] = b"MSWIN4.1"
        struct.pack_into("<H", bpb, 11, 512)            # BytsPerSec
        bpb[13] = 8                                     # SecPerClus = 8 (4096 bytes)
        struct.pack_into("<H", bpb, 14, 32)             # RsvdSecCnt = 32
        bpb[16] = 2                                     # NumFATs = 2
        struct.pack_into("<H", bpb, 17, 0)              # RootEntCnt = 0 (FAT32)
        struct.pack_into("<H", bpb, 19, 0)              # TotSec16 = 0
        bpb[21] = 0xF8                                  # Media = Fixed disk
        struct.pack_into("<H", bpb, 22, 0)              # FATSz16 = 0
        struct.pack_into("<H", bpb, 24, 63)             # SecPerTrk = 63
        struct.pack_into("<H", bpb, 26, 255)            # NumHeads = 255
        struct.pack_into("<I", bpb, 28, start_lba1)     # HiddSec = 2048
        struct.pack_into("<I", bpb, 32, part1_sectors)  # TotSec32 = 63488
        struct.pack_into("<I", bpb, 36, sectors_per_fat)# FATSz32 = 256
        struct.pack_into("<H", bpb, 40, 0)              # ExtFlags = 0
        struct.pack_into("<H", bpb, 42, 0)              # FSVer = 0
        struct.pack_into("<I", bpb, 44, 2)              # RootClus = 2
        struct.pack_into("<H", bpb, 48, 1)              # FSInfo = 1
        struct.pack_into("<H", bpb, 50, 6)              # BkBootSec = 6
        bpb[64] = 0x80                                  # DrvNum = 0x80
        bpb[66] = 0x29                                  # BootSig = 0x29
        struct.pack_into("<I", bpb, 67, 0x12345678)     # VolID
        bpb[71:82] = b"JINN DISK  "
        bpb[82:90] = b"FAT32   "
        bpb[510] = 0x55
        bpb[511] = 0xAA

        f.seek(part1_offset)
        f.write(bpb)
        f.seek(part1_offset + 6 * sector_size)
        f.write(bpb)

        # Cálculo de clusters distintos sem sobreposição
        ola_clusters_needed = max(1, (len(ola_bytes) + cluster_size - 1) // cluster_size)
        browser_clusters_needed = max(1, (len(browser_bytes) + cluster_size - 1) // cluster_size)

        ola_start_clus = 4
        browser_start_clus = ola_start_clus + ola_clusters_needed
        index_clus = browser_start_clus + browser_clusters_needed
        about_clus = index_clus + 1
        next_free_clus = about_clus + 1

        # FSInfo Sector (Setor 1 da partição FAT32)
        fsinfo = bytearray(512)
        struct.pack_into("<I", fsinfo, 0, 0x41615252)    # LeadSig "RRaA"
        struct.pack_into("<I", fsinfo, 484, 0x61417272)  # StrucSig "rrAa"
        struct.pack_into("<I", fsinfo, 488, 0xFFFFFFFF)  # FreeCount
        struct.pack_into("<I", fsinfo, 492, next_free_clus)
        fsinfo[510] = 0x55
        fsinfo[511] = 0xAA
        f.seek(part1_offset + 1 * sector_size)
        f.write(fsinfo)

        # Tabelas FAT 1 e FAT 2 (Setor 32 da partição)
        fat_table = bytearray(sectors_per_fat * sector_size)
        def set_fat(c, val):
            struct.pack_into("<I", fat_table, c * 4, val)

        set_fat(0, 0x0FFFFFF8)
        set_fat(1, 0x0FFFFFFF)
        set_fat(2, 0x0FFFFFFF)  # Cluster 2 (Root)
        set_fat(3, 0x0FFFFFFF)  # Cluster 3 (README)

        # Cadeia OLA.ELF
        for i in range(ola_clusters_needed):
            curr_c = ola_start_clus + i
            val = 0x0FFFFFFF if i == ola_clusters_needed - 1 else curr_c + 1
            set_fat(curr_c, val)

        # Cadeia BROWSER.ELF
        for i in range(browser_clusters_needed):
            curr_c = browser_start_clus + i
            val = 0x0FFFFFFF if i == browser_clusters_needed - 1 else curr_c + 1
            set_fat(curr_c, val)

        set_fat(index_clus, 0x0FFFFFFF)
        set_fat(about_clus, 0x0FFFFFFF)

        fat1_offset = part1_offset + 32 * sector_size
        fat2_offset = part1_offset + (32 + sectors_per_fat) * sector_size
        f.seek(fat1_offset)
        f.write(fat_table)
        f.seek(fat2_offset)
        f.write(fat_table)

        # Área de Dados FAT32 (Cluster 2 = Diretório Raiz)
        data_start_offset1 = part1_offset + (32 + 2 * sectors_per_fat) * sector_size
        root_dir = bytearray(cluster_size)

        def add_dir_entry(index, name11, attr, first_clus, size):
            off = index * 32
            root_dir[off:off+11] = name11.encode("ascii")
            root_dir[off+11] = attr
            clus_hi = (first_clus >> 16) & 0xFFFF
            clus_lo = first_clus & 0xFFFF
            struct.pack_into("<H", root_dir, off + 20, clus_hi)
            struct.pack_into("<H", root_dir, off + 26, clus_lo)
            struct.pack_into("<I", root_dir, off + 28, size)

        add_dir_entry(0, "JINN DISK  ", 0x08, 0, 0)
        add_dir_entry(1, "README  TXT", 0x20, 3, len(readme_text))
        add_dir_entry(2, "OLA     ELF", 0x20, ola_start_clus, len(ola_bytes))
        add_dir_entry(3, "INDEX   HTM", 0x20, index_clus, len(index_html))
        add_dir_entry(4, "ABOUT   HTM", 0x20, about_clus, len(about_html))
        add_dir_entry(5, "BROWSER ELF", 0x20, browser_start_clus, len(browser_bytes))

        f.seek(data_start_offset1)
        f.write(root_dir)

        # Cluster 3: README.TXT
        f.seek(data_start_offset1 + (3 - 2) * cluster_size)
        f.write(readme_text)

        # Clusters OLA.ELF
        f.seek(data_start_offset1 + (ola_start_clus - 2) * cluster_size)
        f.write(ola_bytes)

        # Clusters BROWSER.ELF
        f.seek(data_start_offset1 + (browser_start_clus - 2) * cluster_size)
        f.write(browser_bytes)

        # Cluster index_clus: INDEX.HTM
        f.seek(data_start_offset1 + (index_clus - 2) * cluster_size)
        f.write(index_html)

        # Cluster about_clus: ABOUT.HTM
        f.seek(data_start_offset1 + (about_clus - 2) * cluster_size)
        f.write(about_html)

        # ------------------------------------------------------------------
        # 3. FORMATAÇÃO DA PARTIÇÃO 2: Ext2 (/ext2)
        # ------------------------------------------------------------------
        part2_offset = start_lba2 * sector_size

        # 3.1. Superblock Ext2 (Byte 1024 da partição -> Bloco 1)
        sb = bytearray(1024)
        struct.pack_into("<I", sb, 0, ext2_inodes_count)    # s_inodes_count
        struct.pack_into("<I", sb, 4, ext2_blocks_count)    # s_blocks_count
        struct.pack_into("<I", sb, 8, 1000)                 # s_r_blocks_count
        struct.pack_into("<I", sb, 12, 31600)               # s_free_blocks_count
        struct.pack_into("<I", sb, 16, 500)                 # s_free_inodes_count
        struct.pack_into("<I", sb, 20, 1)                   # s_first_data_block = 1
        struct.pack_into("<I", sb, 24, 0)                   # s_log_block_size = 0 (1024 bytes)
        struct.pack_into("<I", sb, 28, 0)                   # s_log_frag_size = 0
        struct.pack_into("<I", sb, 32, 31744)               # s_blocks_per_group
        struct.pack_into("<I", sb, 36, 31744)               # s_frags_per_group
        struct.pack_into("<I", sb, 40, ext2_inodes_count)   # s_inodes_per_group
        struct.pack_into("<H", sb, 56, 0xEF53)              # s_magic = 0xEF53 (Ext2)
        struct.pack_into("<H", sb, 58, 1)                   # s_state = CLEAN (1)
        struct.pack_into("<H", sb, 60, 1)                   # s_errors = CONTINUE (1)
        struct.pack_into("<I", sb, 76, 1)                   # s_rev_level = 1 (Dynamic rev)
        struct.pack_into("<H", sb, 84, 11)                  # s_first_ino = 11 (Primeiro inode livre padrão)
        struct.pack_into("<H", sb, 88, 128)                 # s_inode_size = 128
        sb[120:136] = b"JINN_EXT2_VOL  "
        f.seek(part2_offset + 1024)
        f.write(sb)

        # 3.2. Descritor de Grupo de Blocos (Bloco 2 -> Offset 2048 da partição)
        gd = bytearray(32)
        struct.pack_into("<I", gd, 0, 3)                    # bg_block_bitmap = bloco 3
        struct.pack_into("<I", gd, 4, 4)                    # bg_inode_bitmap = bloco 4
        struct.pack_into("<I", gd, 8, 5)                    # bg_inode_table = bloco 5
        struct.pack_into("<H", gd, 12, 31600)               # bg_free_blocks_count
        struct.pack_into("<H", gd, 14, 500)                 # bg_free_inodes_count
        struct.pack_into("<H", gd, 16, 1)                   # bg_used_dirs_count = 1
        f.seek(part2_offset + 2048)
        f.write(gd)

        # 3.3. Inode Table (Bloco 5 -> Offset 5 * 1024 = 5120 da partição)
        inode_table_offset = part2_offset + 5120

        # Inode 2: Diretório Raiz / (Offset 5120 + 1 * 128 = 5248)
        ino_root = bytearray(128)
        struct.pack_into("<H", ino_root, 0, 0x41ED)         # mode: DIR | 0755
        struct.pack_into("<H", ino_root, 2, 0)              # uid: 0
        struct.pack_into("<I", ino_root, 4, 1024)           # size: 1024 bytes (1 bloco)
        struct.pack_into("<I", ino_root, 8, 1700000000)     # atime
        struct.pack_into("<I", ino_root, 12, 1700000000)    # ctime
        struct.pack_into("<I", ino_root, 16, 1700000000)    # mtime
        struct.pack_into("<H", ino_root, 26, 2)             # links_count = 2 (. e ..)
        struct.pack_into("<I", ino_root, 28, 2)             # blocks = 2 setores de 512B
        struct.pack_into("<I", ino_root, 40, 70)            # direct_blocks[0] = bloco 70
        f.seek(inode_table_offset + 128)
        f.write(ino_root)

        # Inode 11: Arquivo SERVER.TXT (Offset 5120 + 10 * 128 = 6400)
        ino_server = bytearray(128)
        struct.pack_into("<H", ino_server, 0, 0x81A4)       # mode: REG | 0644
        struct.pack_into("<H", ino_server, 2, 0)            # uid: 0
        struct.pack_into("<I", ino_server, 4, len(server_text)) # size
        struct.pack_into("<I", ino_server, 8, 1700000000)   # atime
        struct.pack_into("<I", ino_server, 12, 1700000000)  # ctime
        struct.pack_into("<I", ino_server, 16, 1700000000)  # mtime
        struct.pack_into("<H", ino_server, 26, 1)           # links_count = 1
        sec_count = max(2, (len(server_text) + 511) // 512)
        struct.pack_into("<I", ino_server, 28, sec_count)   # blocks em setores de 512B
        struct.pack_into("<I", ino_server, 40, 71)          # direct_blocks[0] = bloco 71
        f.seek(inode_table_offset + 10 * 128)
        f.write(ino_server)

        # 3.4. Bloco 70: Conteúdo do Diretório Raiz Ext2 (Offset 70 * 1024)
        dir_block = bytearray(1024)
        # Entrada 1: "." (ino = 2, rec_len = 12, name_len = 1, file_type = 2)
        struct.pack_into("<I", dir_block, 0, 2)
        struct.pack_into("<H", dir_block, 4, 12)
        dir_block[6] = 1
        dir_block[7] = 2
        dir_block[8:9] = b"."

        # Entrada 2: ".." (ino = 2, rec_len = 12, name_len = 2, file_type = 2)
        struct.pack_into("<I", dir_block, 12, 2)
        struct.pack_into("<H", dir_block, 16, 12)
        dir_block[18] = 2
        dir_block[19] = 2
        dir_block[20:22] = b".."

        # Entrada 3: "SERVER.TXT" (ino = 11, rec_len = 1000, name_len = 10, file_type = 1)
        rec_len3 = 1024 - 12 - 12
        struct.pack_into("<I", dir_block, 24, 11)
        struct.pack_into("<H", dir_block, 28, rec_len3)
        dir_block[30] = 10
        dir_block[31] = 1
        dir_block[32:42] = b"SERVER.TXT"

        f.seek(part2_offset + 70 * 1024)
        f.write(dir_block)

        # 3.5. Bloco 71: Dados de SERVER.TXT (Offset 71 * 1024)
        f.seek(part2_offset + 71 * 1024)
        f.write(server_text)

    print(f"Disco virtual criado com sucesso em: {disk_path}")
    print(f"  - Particao 1 (FAT32): LBA {start_lba1} (31 MiB)")
    print(f"    * README.TXT: cluster 3 ({len(readme_text)} bytes)")
    print(f"    * OLA.ELF: cluster {ola_start_clus}..{ola_start_clus + ola_clusters_needed - 1} ({len(ola_bytes)} bytes)")
    print(f"    * BROWSER.ELF: cluster {browser_start_clus}..{browser_start_clus + browser_clusters_needed - 1} ({len(browser_bytes)} bytes)")
    print(f"    * INDEX.HTM: cluster {index_clus} ({len(index_html)} bytes)")
    print(f"    * ABOUT.HTM: cluster {about_clus} ({len(about_html)} bytes)")
    print(f"  - Particao 2 (Ext2):  LBA {start_lba2} (31 MiB) -> SERVER.TXT ({len(server_text)} bytes, Read-Only)")

if __name__ == "__main__":
    main()
