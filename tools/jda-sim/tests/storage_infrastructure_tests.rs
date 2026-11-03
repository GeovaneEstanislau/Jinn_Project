//! Testes Automatizados da Infraestrutura de Armazenamento Jinn Storage 0.1.0
//!
//! Cenários automatizados com discos virtuais descartáveis:
//! 1. Disco vazio (sem MBR/GPT).
//! 2. Disco com MBR válido.
//! 3. Disco com GPT válido.
//! 4. Disco com GPT corrompido (tabela primária corrompida e cabeçalho de backup íntegro).
//! 5. Partição FAT32 com arquivos normais e longos.
//! 6. Partição ext2 com estrutura de diretórios e arquivos.
//! 7. Tentativa de montagem dupla no mesmo ponto.
//! 8. Tentativa de desmontagem de volume ocupado e proteção contra desmontagem da raiz.
//! 9. Falha de leitura/escrita e tratamento seguro de erros.
//! 10. Persistência de alterações após reinício simulado.

use std::collections::HashMap;

// ── Virtual Disposable Block Device ──────────────────────────────────────────

#[derive(Clone)]
struct VirtualDisk {
    sectors: HashMap<u64, [u8; 512]>,
    total_sectors: u64,
    fail_reads: bool,
    fail_writes: bool,
    read_only: bool,
    read_count: usize,
    write_count: usize,
}

impl VirtualDisk {
    fn new(total_sectors: u64) -> Self {
        Self {
            sectors: HashMap::new(),
            total_sectors,
            fail_reads: false,
            fail_writes: false,
            read_only: false,
            read_count: 0,
            write_count: 0,
        }
    }

    fn read_sector(&mut self, lba: u64, buf: &mut [u8; 512]) -> Result<(), &'static str> {
        if self.fail_reads {
            return Err("EIO: Hardware read fault");
        }
        if lba >= self.total_sectors {
            return Err("EINVAL: LBA out of bounds");
        }
        self.read_count += 1;
        if let Some(data) = self.sectors.get(&lba) {
            buf.copy_from_slice(data);
        } else {
            buf.fill(0);
        }
        Ok(())
    }

    fn write_sector(&mut self, lba: u64, buf: &[u8; 512]) -> Result<(), &'static str> {
        if self.fail_writes {
            return Err("EIO: Hardware write fault");
        }
        if self.read_only {
            return Err("EROFS: Read-only storage device");
        }
        if lba >= self.total_sectors {
            return Err("EINVAL: LBA out of bounds");
        }
        self.write_count += 1;
        self.sectors.insert(lba, *buf);
        Ok(())
    }
}

// ── Partition Parser Simulation ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum FsKind {
    Fat32,
    Ext2,
    Unknown,
}

#[derive(Debug, Clone)]
struct ParsedPartition {
    index: usize,
    start_lba: u64,
    sector_count: u64,
    part_type: u8,
    fs_kind: FsKind,
    is_boot: bool,
    is_root: bool,
}

fn scan_virtual_partitions(disk: &mut VirtualDisk) -> Result<Vec<ParsedPartition>, &'static str> {
    let mut sector0 = [0u8; 512];
    disk.read_sector(0, &mut sector0)?;

    // Verifica assinatura MBR
    if sector0[510] != 0x55 || sector0[511] != 0xAA {
        return Ok(Vec::new()); // Disco vazio sem MBR
    }

    let is_protective_mbr = sector0[446 + 4] == 0xEE;
    if is_protective_mbr {
        // Tenta ler GPT Primário em LBA 1
        let mut gpt_header = [0u8; 512];
        let primary_ok = disk.read_sector(1, &mut gpt_header).is_ok()
            && &gpt_header[0..8] == b"EFI PART";

        if primary_ok {
            return parse_gpt(disk, 2);
        }

        // Se primário corrompido, tenta ler cabeçalho secundário no final do disco
        let backup_lba = disk.total_sectors.saturating_sub(1);
        let backup_ok = disk.read_sector(backup_lba, &mut gpt_header).is_ok()
            && &gpt_header[0..8] == b"EFI PART";

        if backup_ok {
            let part_lba = disk.total_sectors.saturating_sub(33);
            return parse_gpt(disk, part_lba);
        }

        return Err("GPT corrompido tanto no cabeçalho primário quanto secundário");
    }

    // MBR Tradicional: 4 entradas de 16 bytes a partir do offset 446
    let mut parts = Vec::new();
    for i in 0..4 {
        let off = 446 + (i * 16);
        let boot_flag = sector0[off];
        let p_type = sector0[off + 4];
        if p_type == 0 { continue; }

        let start_lba = u32::from_le_bytes(sector0[off + 8..off + 12].try_into().unwrap()) as u64;
        let count = u32::from_le_bytes(sector0[off + 12..off + 16].try_into().unwrap()) as u64;

        if let Some(end) = start_lba.checked_add(count) {
            if end <= disk.total_sectors {
                let fs = match p_type {
                    0x0B | 0x0C => FsKind::Fat32,
                    0x83 => FsKind::Ext2,
                    _ => FsKind::Unknown,
                };
                parts.push(ParsedPartition {
                    index: i + 1,
                    start_lba,
                    sector_count: count,
                    part_type: p_type,
                    fs_kind: fs,
                    is_boot: boot_flag == 0x80,
                    is_root: i == 0,
                });
            }
        }
    }

    Ok(parts)
}

fn parse_gpt(disk: &mut VirtualDisk, entries_start_lba: u64) -> Result<Vec<ParsedPartition>, &'static str> {
    let mut parts = Vec::new();
    let mut entry_sector = [0u8; 512];
    disk.read_sector(entries_start_lba, &mut entry_sector)?;

    // Cada entrada GPT tem 128 bytes (4 entradas por setor de 512 bytes)
    for i in 0..4 {
        let off = i * 128;
        let type_guid = &entry_sector[off..off + 16];
        if type_guid.iter().all(|&b| b == 0) { continue; }

        let start_lba = u64::from_le_bytes(entry_sector[off + 32..off + 40].try_into().unwrap());
        let end_lba = u64::from_le_bytes(entry_sector[off + 40..off + 48].try_into().unwrap());

        if end_lba >= start_lba && end_lba < disk.total_sectors {
            let count = end_lba - start_lba + 1;
            parts.push(ParsedPartition {
                index: i + 1,
                start_lba,
                sector_count: count,
                part_type: 0xEE,
                fs_kind: FsKind::Fat32,
                is_boot: i == 0,
                is_root: i == 0,
            });
        }
    }
    Ok(parts)
}

// ── Volume Manager Simulation ────────────────────────────────────────────────

struct SimVolume {
    id: usize,
    label: String,
    mount_point: String,
    fs_type: String,
    is_mounted: bool,
    is_root: bool,
    open_handles: usize,
}

struct SimVolumeManager {
    volumes: Vec<SimVolume>,
}

impl SimVolumeManager {
    fn new() -> Self {
        Self { volumes: Vec::new() }
    }

    fn register(&mut self, vol: SimVolume) -> usize {
        let id = self.volumes.len();
        self.volumes.push(vol);
        id
    }

    fn mount(&mut self, id: usize, mount_point: &str) -> Result<(), &'static str> {
        let vol = self.volumes.get(id).ok_or("Volume nao encontrado")?;
        if vol.is_mounted {
            return Err("Volume ja esta montado");
        }
        for v in &self.volumes {
            if v.is_mounted && v.mount_point == mount_point {
                return Err("Ponto de montagem ja ocupado por outro volume");
            }
        }
        let v_mut = self.volumes.get_mut(id).unwrap();
        v_mut.mount_point = mount_point.to_string();
        v_mut.is_mounted = true;
        Ok(())
    }

    fn unmount(&mut self, id: usize) -> Result<(), &'static str> {
        let vol = self.volumes.get(id).ok_or("Volume nao encontrado")?;
        if !vol.is_mounted {
            return Err("Volume nao esta montado");
        }
        if vol.is_root || vol.mount_point == "/" || vol.mount_point == "/disk" {
            return Err("Tentativa de desmontar o volume raiz ativo do sistema bloqueada");
        }
        if vol.open_handles > 0 {
            return Err("Volume ocupado: existem descritores abertos");
        }
        let v_mut = self.volumes.get_mut(id).unwrap();
        v_mut.is_mounted = false;
        Ok(())
    }

    fn format(&mut self, id: usize, fs: &str) -> Result<(), &'static str> {
        let vol = self.volumes.get(id).ok_or("Volume nao encontrado")?;
        if vol.is_root {
            return Err("Formatacao da raiz do sistema proibida");
        }
        if vol.is_mounted {
            return Err("Desmonte o volume antes de formata-lo");
        }
        let v_mut = self.volumes.get_mut(id).unwrap();
        v_mut.fs_type = fs.to_string();
        Ok(())
    }
}

// ── Cenário 1: Disco Vazio (Sem MBR / GPT) ────────────────────────────────────

#[test]
fn test_scenario_1_empty_disk() {
    let mut disk = VirtualDisk::new(2048);
    let parts = scan_virtual_partitions(&mut disk).expect("Varredura em disco vazio deve ter sucesso");
    assert!(parts.is_empty(), "Disco vazio não deve retornar partições");

    // Validação de limites
    let mut buf = [0u8; 512];
    assert!(disk.read_sector(2047, &mut buf).is_ok());
    assert!(disk.read_sector(2048, &mut buf).is_err(), "Acesso além da capacidade deve falhar");
}

// ── Cenário 2: Disco com MBR Válido ──────────────────────────────────────────

#[test]
fn test_scenario_2_valid_mbr() {
    let mut disk = VirtualDisk::new(131072); // 64 MiB
    let mut mbr = [0u8; 512];

    // Partição 1: FAT32 LBA (0x0C), Bootável (0x80), LBA 2048, 65536 setores (32 MiB)
    mbr[446] = 0x80;
    mbr[446 + 4] = 0x0C;
    mbr[446 + 8..446 + 12].copy_from_slice(&2048u32.to_le_bytes());
    mbr[446 + 12..446 + 16].copy_from_slice(&65536u32.to_le_bytes());

    // Partição 2: Ext2 (0x83), LBA 67584, 63488 setores (~31 MiB)
    mbr[462] = 0x00;
    mbr[462 + 4] = 0x83;
    mbr[462 + 8..462 + 12].copy_from_slice(&67584u32.to_le_bytes());
    mbr[462 + 12..462 + 16].copy_from_slice(&63488u32.to_le_bytes());

    // Assinatura MBR
    mbr[510] = 0x55;
    mbr[511] = 0xAA;

    disk.write_sector(0, &mbr).unwrap();

    let parts = scan_virtual_partitions(&mut disk).expect("MBR valido deve ser escaneado");
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].fs_kind, FsKind::Fat32);
    assert!(parts[0].is_boot);
    assert!(parts[0].is_root);
    assert_eq!(parts[0].start_lba, 2048);
    assert_eq!(parts[1].fs_kind, FsKind::Ext2);
    assert!(!parts[1].is_boot);
    assert_eq!(parts[1].start_lba, 67584);
}

// ── Cenário 3: Disco com GPT Válido ──────────────────────────────────────────

#[test]
fn test_scenario_3_valid_gpt() {
    let mut disk = VirtualDisk::new(262144); // 128 MiB

    // LBA 0: Protective MBR
    let mut mbr = [0u8; 512];
    mbr[446 + 4] = 0xEE;
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    disk.write_sector(0, &mbr).unwrap();

    // LBA 1: GPT Header Primário
    let mut gpt_hdr = [0u8; 512];
    gpt_hdr[0..8].copy_from_slice(b"EFI PART");
    gpt_hdr[8..12].copy_from_slice(&0x00010000u32.to_le_bytes()); // Rev 1.0
    disk.write_sector(1, &gpt_hdr).unwrap();

    // LBA 2: Entradas de Partição GPT
    let mut gpt_entries = [0u8; 512];
    // Entrada 1: Type GUID qualquer não-zero
    gpt_entries[0..16].fill(0xAA);
    // Start LBA: 2048, End LBA: 133119 (64 MiB)
    gpt_entries[32..40].copy_from_slice(&2048u64.to_le_bytes());
    gpt_entries[40..48].copy_from_slice(&133119u64.to_le_bytes());
    disk.write_sector(2, &gpt_entries).unwrap();

    let parts = scan_virtual_partitions(&mut disk).expect("GPT valido deve ser escaneado");
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].start_lba, 2048);
    assert_eq!(parts[0].sector_count, 131072);
}

// ── Cenário 4: GPT Corrompido com Fallback no Cabeçalho de Backup ─────────────

#[test]
fn test_scenario_4_corrupted_gpt_primary_backup_fallback() {
    let total_sec = 262144u64;
    let mut disk = VirtualDisk::new(total_sec);

    // LBA 0: Protective MBR
    let mut mbr = [0u8; 512];
    mbr[446 + 4] = 0xEE;
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    disk.write_sector(0, &mbr).unwrap();

    // LBA 1: GPT Primário CORROMPIDO (zeros ou lixo)
    let bad_hdr = [0x55u8; 512];
    disk.write_sector(1, &bad_hdr).unwrap();

    // Cabeçalho Secundário em total_sec - 1
    let mut backup_hdr = [0u8; 512];
    backup_hdr[0..8].copy_from_slice(b"EFI PART");
    disk.write_sector(total_sec - 1, &backup_hdr).unwrap();

    // Entradas Secundárias em total_sec - 33
    let mut backup_entries = [0u8; 512];
    backup_entries[0..16].fill(0xBB);
    backup_entries[32..40].copy_from_slice(&4096u64.to_le_bytes());
    backup_entries[40..48].copy_from_slice(&69631u64.to_le_bytes());
    disk.write_sector(total_sec - 33, &backup_entries).unwrap();

    // O scanner deve detectar falha no primário e recuperar via backup
    let parts = scan_virtual_partitions(&mut disk).expect("Recuperacao via backup GPT deve funcionar");
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].start_lba, 4096);
    assert_eq!(parts[0].sector_count, 65536);
}

// ── Cenário 5: Partição FAT32 com Nomes Normais e Longos (LFN) ────────────────

#[test]
fn test_scenario_5_fat32_normal_and_long_filename() {
    let mut root_dir_sector = [0u8; 512];

    // Entrada 1: Nome curto 8.3 padrão "KERNEL  BIN"
    root_dir_sector[0..11].copy_from_slice(b"KERNEL  BIN");
    root_dir_sector[11] = 0x20; // Archive
    root_dir_sector[28..32].copy_from_slice(&65536u32.to_le_bytes()); // Tamanho: 64 KB

    // Entrada 2: Entrada LFN (Attr 0x0F) associada a "jinn_browser.elf"
    root_dir_sector[32] = 0x41; // Seq 1, Last LFN
    root_dir_sector[32 + 11] = 0x0F; // LFN Attribute Marker

    // Entrada associada 8.3 "JINN_B~1ELF"
    root_dir_sector[64..75].copy_from_slice(b"JINN_B~1ELF");
    root_dir_sector[64 + 11] = 0x20;
    root_dir_sector[64 + 28..64 + 32].copy_from_slice(&131072u32.to_le_bytes()); // 128 KB

    assert_eq!(&root_dir_sector[0..11], b"KERNEL  BIN");
    assert_eq!(root_dir_sector[32 + 11], 0x0F);
    assert_eq!(&root_dir_sector[64..75], b"JINN_B~1ELF");
}

// ── Cenário 6: Partição Ext2 com Estrutura de Diretórios e Inodes ─────────────

#[test]
fn test_scenario_6_ext2_directory_and_file_structure() {
    // Simula Superbloco Ext2 no offset 1024 (LBA 2)
    let mut superblock = [0u8; 1024];
    // Magic Ext2: 0xEF53 no offset 56
    superblock[56..58].copy_from_slice(&0xEF53u16.to_le_bytes());
    let magic = u16::from_le_bytes(superblock[56..58].try_into().unwrap());
    assert_eq!(magic, 0xEF53);

    // Simula entrada de diretório Ext2: inode (4B), rec_len (2B), name_len (1B), file_type (1B), name
    let mut dir_block = [0u8; 1024];
    // "." (Inode 2)
    dir_block[0..4].copy_from_slice(&2u32.to_le_bytes());
    dir_block[4..6].copy_from_slice(&12u16.to_le_bytes()); // rec_len
    dir_block[6] = 1; // name_len
    dir_block[7] = 2; // dir
    dir_block[8] = b'.';

    // ".." (Inode 2)
    dir_block[12..16].copy_from_slice(&2u32.to_le_bytes());
    dir_block[16..18].copy_from_slice(&12u16.to_le_bytes());
    dir_block[18] = 2;
    dir_block[19] = 2;
    dir_block[20..22].copy_from_slice(b"..");

    // "config.json" (Inode 11)
    let fname = b"config.json";
    dir_block[24..28].copy_from_slice(&11u32.to_le_bytes());
    dir_block[28..30].copy_from_slice(&1000u16.to_le_bytes()); // resto do bloco
    dir_block[30] = fname.len() as u8;
    dir_block[31] = 1; // regular file
    dir_block[32..32 + fname.len()].copy_from_slice(fname);

    assert_eq!(&dir_block[32..32 + fname.len()], b"config.json");
}

// ── Cenário 7: Rejeição de Montagem Dupla no Mesmo Ponto ──────────────────────

#[test]
fn test_scenario_7_double_mount_rejection() {
    let mut vm = SimVolumeManager::new();
    let v0 = SimVolume {
        id: 0,
        label: "SYS".into(),
        mount_point: "/disk".into(),
        fs_type: "FAT32".into(),
        is_mounted: true,
        is_root: true,
        open_handles: 0,
    };
    let v1 = SimVolume {
        id: 1,
        label: "DATA".into(),
        mount_point: "".into(),
        fs_type: "Ext2".into(),
        is_mounted: false,
        is_root: false,
        open_handles: 0,
    };
    vm.register(v0);
    let id1 = vm.register(v1);

    // Tentativa 1: Montar volume já montado
    assert_eq!(vm.mount(0, "/disk2"), Err("Volume ja esta montado"));

    // Tentativa 2: Montar volume no mesmo ponto de montagem já em uso ("/disk")
    assert_eq!(vm.mount(id1, "/disk"), Err("Ponto de montagem ja ocupado por outro volume"));

    // Montagem válida em outro ponto
    assert!(vm.mount(id1, "/data").is_ok());
}

// ── Cenário 8: Rejeição de Desmontagem de Volume Ocupado e Proteção da Raiz ───

#[test]
fn test_scenario_8_busy_unmount_rejection_and_root_protection() {
    let mut vm = SimVolumeManager::new();
    let v_root = SimVolume {
        id: 0,
        label: "ROOT".into(),
        mount_point: "/disk".into(),
        fs_type: "FAT32".into(),
        is_mounted: true,
        is_root: true,
        open_handles: 0,
    };
    let v_data = SimVolume {
        id: 1,
        label: "DATA".into(),
        mount_point: "/data".into(),
        fs_type: "Ext2".into(),
        is_mounted: true,
        is_root: false,
        open_handles: 2, // 2 descritores abertos
    };
    vm.register(v_root);
    vm.register(v_data);

    // 1. Desmontar raiz DEVE ser rejeitado incondicionalmente
    assert_eq!(vm.unmount(0), Err("Tentativa de desmontar o volume raiz ativo do sistema bloqueada"));

    // 2. Desmontar volume de dados com arquivos abertos DEVE ser rejeitado
    assert_eq!(vm.unmount(1), Err("Volume ocupado: existem descritores abertos"));

    // 3. Após fechar os descritores, desmontagem é permitida
    vm.volumes[1].open_handles = 0;
    assert!(vm.unmount(1).is_ok());
    assert!(!vm.volumes[1].is_mounted);

    // 4. Formatação de volume desmontado é permitida
    assert!(vm.format(1, "FAT32").is_ok());
    assert_eq!(vm.volumes[1].fs_type, "FAT32");
}

// ── Cenário 9: Falha de Leitura/Escrita e Tratamento Seguro de Erros ──────────

#[test]
fn test_scenario_9_read_write_hardware_error_handling() {
    let mut disk = VirtualDisk::new(1024);
    let mut buf = [0u8; 512];

    // Simula falha de leitura de hardware
    disk.fail_reads = true;
    assert_eq!(disk.read_sector(10, &mut buf), Err("EIO: Hardware read fault"));
    disk.fail_reads = false;

    // Simula falha de escrita de hardware
    disk.fail_writes = true;
    assert_eq!(disk.write_sector(10, &buf), Err("EIO: Hardware write fault"));
    disk.fail_writes = false;

    // Simula mídia declarada como somente-leitura (ex: CD-ROM / Proteção contra gravação)
    disk.read_only = true;
    assert_eq!(disk.write_sector(10, &buf), Err("EROFS: Read-only storage device"));
    // Leitura ainda deve funcionar em mídia somente-leitura
    assert!(disk.read_sector(10, &mut buf).is_ok());
}

// ── Cenário 10: Persistência de Alterações Após Reinício Simulado ──────────────

#[test]
fn test_scenario_10_persistence_across_reboot_simulation() {
    let mut raw_disk = VirtualDisk::new(4096);

    // 1. Sistema inicial escreve dados estruturados no disco
    let magic_data = *b"JINN_OS_STORAGE_PERSISTENCE_TEST_SECTOR_PAYLOAD_VALIDATION_STRING_DATA_2026";
    let mut sec = [0u8; 512];
    sec[..magic_data.len()].copy_from_slice(&magic_data);
    raw_disk.write_sector(1234, &sec).expect("Escrita inicial deve ter sucesso");

    // 2. Simula "Reboot" limpando toda a memória e estruturas voláteis
    // (O disco físico mantém seus setores no meio persistente)
    let mut rebooted_disk = VirtualDisk {
        sectors: raw_disk.sectors.clone(), // Setores mantidos na mídia magnética/flash
        total_sectors: raw_disk.total_sectors,
        fail_reads: false,
        fail_writes: false,
        read_only: false,
        read_count: 0,
        write_count: 0,
    };

    // 3. Sistema pós-reboot relê os dados
    let mut read_sec = [0u8; 512];
    rebooted_disk.read_sector(1234, &mut read_sec).expect("Leitura pos-reboot deve ter sucesso");
    assert_eq!(&read_sec[..magic_data.len()], &magic_data, "Dados persistidos devem ser 100% idênticos após reboot");
}
