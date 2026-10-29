//! Suíte de Testes Automatizados para a Imagem de Disco FAT32 + Ext2 do Jinn OS
//!
//! Valida a integridade estrutural do MBR, da tabela de partições, do BPB FAT32,
//! dos descritores de diretório raiz, da cadeia de clusters na FAT e dos
//! executáveis ELF extraídos (BROWSER.ELF e OLA.ELF), garantindo:
//! 1. Ausência de sobreposição ou compartilhamento indevido de clusters entre arquivos.
//! 2. Terminação correta com marca de fim de arquivo (EOF) na FAT.
//! 3. Correspondência exata de tamanho e integridade byte a byte com os binários de origem.
//! 4. Validade dos cabeçalhos ELF64 e pontos de entrada em espaço de usuário (Ring 3).

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::Command;

fn find_project_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // tools/elf-tests -> project root is ../..
    let root = manifest_dir.parent().and_then(|p| p.parent()).unwrap();
    root.to_path_buf()
}

fn ensure_disk_image() -> PathBuf {
    let project_root = find_project_root();
    let disk_path = project_root.join("disk.img");
    if !disk_path.exists() {
        // Gera via scripts/make_disk.py ou powershell
        let script = project_root.join("scripts").join("make_disk.py");
        let status = Command::new("py")
            .arg(&script)
            .status()
            .or_else(|_| Command::new("python3").arg(&script).status())
            .expect("Falha ao invocar gerador da imagem de disco");
        assert!(status.success(), "Falha ao gerar disk.img");
    }
    assert!(disk_path.exists(), "disk.img não encontrado em {:?}", disk_path);
    disk_path
}

#[test]
fn test_mbr_partitions_layout() {
    let disk_path = ensure_disk_image();
    let mut file = File::open(&disk_path).expect("Não foi possível abrir disk.img");
    let mut mbr = [0u8; 512];
    file.read_exact(&mut mbr).expect("Leitura do MBR falhou");

    // Valida assinatura de boot MBR 0x55, 0xAA
    assert_eq!(mbr[510], 0x55);
    assert_eq!(mbr[511], 0xAA);

    // Partição 1: FAT32
    let p1_offset = 0x1BE;
    let p1_bootable = mbr[p1_offset];
    let p1_type = mbr[p1_offset + 4];
    let p1_lba = u32::from_le_bytes(mbr[p1_offset + 8..p1_offset + 12].try_into().unwrap());
    let p1_sectors = u32::from_le_bytes(mbr[p1_offset + 12..p1_offset + 16].try_into().unwrap());

    assert_eq!(p1_bootable, 0x80, "Partição 1 deve ter flag bootable ativa");
    assert_eq!(p1_type, 0x0C, "Partição 1 deve ser tipo 0x0C (FAT32 LBA)");
    assert_eq!(p1_lba, 2048, "Partição 1 deve iniciar no LBA 2048");
    assert!(p1_sectors > 0, "Partição 1 deve ter setores alocados");

    // Partição 2: Ext2
    let p2_offset = 0x1CE;
    let p2_bootable = mbr[p2_offset];
    let p2_type = mbr[p2_offset + 4];
    let p2_lba = u32::from_le_bytes(mbr[p2_offset + 8..p2_offset + 12].try_into().unwrap());
    let p2_sectors = u32::from_le_bytes(mbr[p2_offset + 12..p2_offset + 16].try_into().unwrap());

    assert_eq!(p2_bootable, 0x00, "Partição 2 não deve ser bootable");
    assert_eq!(p2_type, 0x83, "Partição 2 deve ser tipo 0x83 (Linux native / Ext2)");
    assert!(p2_lba >= p1_lba + p1_sectors, "Partição 2 não pode sobrepor a Partição 1");
    assert_eq!(p2_lba, 65536, "Partição 2 deve iniciar no LBA 65536");
    assert!(p2_sectors > 0, "Partição 2 deve ter setores alocados");
}

#[derive(Debug)]
#[allow(dead_code)]
struct Fat32Bpb {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    sectors_per_fat: u32,
    root_cluster: u32,
    part_byte_offset: u64,
    fat1_offset: u64,
    fat2_offset: u64,
    data_area_offset: u64,
    cluster_size: u32,
}

impl Fat32Bpb {
    fn read(file: &mut File, part_lba: u32) -> Self {
        let part_byte_offset = (part_lba as u64) * 512;
        file.seek(SeekFrom::Start(part_byte_offset)).unwrap();
        let mut bpb = [0u8; 512];
        file.read_exact(&mut bpb).unwrap();

        assert_eq!(bpb[510], 0x55);
        assert_eq!(bpb[511], 0xAA);

        let bytes_per_sector = u16::from_le_bytes(bpb[11..13].try_into().unwrap());
        let sectors_per_cluster = bpb[13];
        let reserved_sectors = u16::from_le_bytes(bpb[14..16].try_into().unwrap());
        let num_fats = bpb[16];
        let sectors_per_fat = u32::from_le_bytes(bpb[36..40].try_into().unwrap());
        let root_cluster = u32::from_le_bytes(bpb[44..48].try_into().unwrap());

        assert_eq!(bytes_per_sector, 512);
        assert_eq!(sectors_per_cluster, 8);
        assert_eq!(num_fats, 2);
        assert_eq!(root_cluster, 2);

        let cluster_size = (bytes_per_sector as u32) * (sectors_per_cluster as u32);
        let fat1_offset = part_byte_offset + (reserved_sectors as u64) * (bytes_per_sector as u64);
        let fat2_offset = fat1_offset + (sectors_per_fat as u64) * (bytes_per_sector as u64);
        let data_area_offset = fat1_offset + (num_fats as u64) * (sectors_per_fat as u64) * (bytes_per_sector as u64);

        Fat32Bpb {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            sectors_per_fat,
            root_cluster,
            part_byte_offset,
            fat1_offset,
            fat2_offset,
            data_area_offset,
            cluster_size,
        }
    }

    fn cluster_offset(&self, cluster: u32) -> u64 {
        assert!(cluster >= 2, "Cluster inválido < 2: {}", cluster);
        self.data_area_offset + ((cluster - 2) as u64) * (self.cluster_size as u64)
    }

    fn read_fat_entry(&self, file: &mut File, cluster: u32) -> u32 {
        let entry_offset = self.fat1_offset + (cluster as u64) * 4;
        file.seek(SeekFrom::Start(entry_offset)).unwrap();
        let mut buf = [0u8; 4];
        file.read_exact(&mut buf).unwrap();
        u32::from_le_bytes(buf) & 0x0FFF_FFFF
    }

    fn trace_chain(&self, file: &mut File, start_cluster: u32) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut curr = start_cluster;
        while curr < 0x0FFF_FFF8 {
            chain.push(curr);
            let next = self.read_fat_entry(file, curr);
            if next == 0 || next == curr {
                panic!("Loop infinito ou cluster nulo na cadeia FAT para cluster {}", curr);
            }
            if chain.len() > 100_000 {
                panic!("Cadeia FAT excedeu limite razoável");
            }
            curr = next;
        }
        chain
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct DirEntry {
    name: String,
    attr: u8,
    first_cluster: u32,
    size: u32,
}

fn read_root_entries(file: &mut File, bpb: &Fat32Bpb) -> Vec<DirEntry> {
    let root_offset = bpb.cluster_offset(bpb.root_cluster);
    file.seek(SeekFrom::Start(root_offset)).unwrap();
    let mut dir_buf = vec![0u8; bpb.cluster_size as usize];
    file.read_exact(&mut dir_buf).unwrap();

    let mut entries = Vec::new();
    for chunk in dir_buf.chunks_exact(32) {
        if chunk[0] == 0x00 {
            break; // Fim do diretório
        }
        if chunk[0] == 0xE5 || chunk[11] == 0x0F {
            continue; // Deletado ou LFN
        }

        let name_bytes = &chunk[0..11];
        let name_str = String::from_utf8_lossy(name_bytes).trim().to_string();
        let attr = chunk[11];
        let fst_clus_hi = u16::from_le_bytes([chunk[20], chunk[21]]) as u32;
        let fst_clus_lo = u16::from_le_bytes([chunk[26], chunk[27]]) as u32;
        let first_cluster = (fst_clus_hi << 16) | fst_clus_lo;
        let size = u32::from_le_bytes(chunk[28..32].try_into().unwrap());

        entries.push(DirEntry {
            name: name_str,
            attr,
            first_cluster,
            size,
        });
    }
    entries
}

#[test]
fn test_fat32_root_directory_and_cluster_invariants() {
    let disk_path = ensure_disk_image();
    let mut file = File::open(&disk_path).expect("Não foi possível abrir disk.img");
    let bpb = Fat32Bpb::read(&mut file, 2048);

    let entries = read_root_entries(&mut file, &bpb);

    // Valida que os arquivos obrigatórios estão presentes
    let readme_entry = entries.iter().find(|e| e.name.starts_with("README")).expect("README.TXT ausente");
    let ola_entry = entries.iter().find(|e| e.name.starts_with("OLA")).expect("OLA.ELF ausente");
    let browser_entry = entries.iter().find(|e| e.name.starts_with("BROWSER")).expect("BROWSER.ELF ausente");

    // 1. Invariante de Clusters Iniciais Distintos
    assert_ne!(
        ola_entry.first_cluster, browser_entry.first_cluster,
        "CRÍTICO: OLA.ELF e BROWSER.ELF apontam para o mesmo cluster inicial!"
    );
    assert_ne!(readme_entry.first_cluster, ola_entry.first_cluster);
    assert_ne!(readme_entry.first_cluster, browser_entry.first_cluster);

    // 2. Rastreio e Invariante de Não-Sobreposição de Clusters (FAT Chains Disjuntas)
    let ola_chain = bpb.trace_chain(&mut file, ola_entry.first_cluster);
    let browser_chain = bpb.trace_chain(&mut file, browser_entry.first_cluster);
    let readme_chain = bpb.trace_chain(&mut file, readme_entry.first_cluster);

    let ola_set: HashSet<u32> = ola_chain.iter().cloned().collect();
    let browser_set: HashSet<u32> = browser_chain.iter().cloned().collect();
    let readme_set: HashSet<u32> = readme_chain.iter().cloned().collect();

    let intersection_ola_browser: Vec<_> = ola_set.intersection(&browser_set).collect();
    assert!(
        intersection_ola_browser.is_empty(),
        "CRÍTICO: Sobreposição de clusters detectada entre OLA.ELF e BROWSER.ELF: {:?}",
        intersection_ola_browser
    );

    let intersection_readme_browser: Vec<_> = readme_set.intersection(&browser_set).collect();
    assert!(intersection_readme_browser.is_empty(), "Sobreposição com README.TXT");

    // 3. Validação de capacidade da cadeia vs tamanho registrado
    let cluster_size = bpb.cluster_size;
    let expected_ola_clusters = (ola_entry.size + cluster_size - 1) / cluster_size;
    let expected_browser_clusters = (browser_entry.size + cluster_size - 1) / cluster_size;

    assert_eq!(
        ola_chain.len() as u32,
        expected_ola_clusters,
        "Cadeia de clusters do OLA.ELF incompatível com o tamanho"
    );
    assert_eq!(
        browser_chain.len() as u32,
        expected_browser_clusters,
        "Cadeia de clusters do BROWSER.ELF incompatível com o tamanho"
    );
}

#[test]
fn test_extracted_executables_integrity_and_elf_contracts() {
    let disk_path = ensure_disk_image();
    let mut file = File::open(&disk_path).expect("Não foi possível abrir disk.img");
    let bpb = Fat32Bpb::read(&mut file, 2048);
    let entries = read_root_entries(&mut file, &bpb);

    let project_root = find_project_root();
    let source_ola_path = project_root.join("iso_root").join("apps").join("ola-mundo.elf");
    let source_browser_path = project_root.join("iso_root").join("apps").join("browser.elf");

    for (entry_name, source_path) in [("OLA", source_ola_path), ("BROWSER", source_browser_path)] {
        let entry = entries.iter().find(|e| e.name.starts_with(entry_name)).expect("Entrada não encontrada");
        let chain = bpb.trace_chain(&mut file, entry.first_cluster);

        let mut extracted_data = Vec::with_capacity(entry.size as usize);
        for &cluster in &chain {
            let clus_offset = bpb.cluster_offset(cluster);
            file.seek(SeekFrom::Start(clus_offset)).unwrap();
            let mut buf = vec![0u8; bpb.cluster_size as usize];
            file.read_exact(&mut buf).unwrap();
            extracted_data.extend_from_slice(&buf);
        }
        extracted_data.truncate(entry.size as usize);

        // 1. Valida ELF64 Header
        assert!(extracted_data.len() >= 64, "Executável menor que o cabeçalho ELF64");
        assert_eq!(&extracted_data[0..4], b"\x7fELF", "Magic ELF inválido em {}", entry_name);
        assert_eq!(extracted_data[4], 2, "Classe ELF deve ser 64-bit (ELFCLASS64)");
        assert_eq!(extracted_data[5], 1, "Endianness deve ser Little-Endian");
        assert_eq!(extracted_data[6], 1, "Versão ELF deve ser 1");

        let e_type = u16::from_le_bytes([extracted_data[16], extracted_data[17]]);
        let e_machine = u16::from_le_bytes([extracted_data[18], extracted_data[19]]);
        assert_eq!(e_type, 2, "Tipo ELF deve ser ET_EXEC (2)");
        assert_eq!(e_machine, 62, "Arquitetura deve ser x86_64 (62)");

        let e_entry = u64::from_le_bytes(extracted_data[24..32].try_into().unwrap());
        assert!(
            e_entry >= 0x0001_0000 && e_entry < 0x0000_8000_0000_0000,
            "Ponto de entrada 0x{:X} fora do espaço canônico Ring 3",
            e_entry
        );

        // 2. Se o arquivo de origem existir, compara byte a byte
        if source_path.exists() {
            let mut source_file = File::open(&source_path).expect("Falha ao abrir binário de origem");
            let mut source_data = Vec::new();
            source_file.read_to_end(&mut source_data).unwrap();

            assert_eq!(
                extracted_data.len(),
                source_data.len(),
                "Tamanho extraído de {} difere do arquivo de origem",
                entry_name
            );
            assert_eq!(
                extracted_data, source_data,
                "Dados extraídos de {} não correspondem exatamente ao binário de origem!",
                entry_name
            );
        }
    }
}

#[test]
fn test_ext2_partition_superblock() {
    let disk_path = ensure_disk_image();
    let mut file = File::open(&disk_path).expect("Não foi possível abrir disk.img");
    let part2_offset = 65536 * 512; // LBA 65536
    let sb_offset = part2_offset + 1024; // Superblock fica no byte 1024 da partição

    file.seek(SeekFrom::Start(sb_offset)).unwrap();
    let mut sb = [0u8; 1024];
    file.read_exact(&mut sb).unwrap();

    let magic = u16::from_le_bytes([sb[56], sb[57]]);
    assert_eq!(magic, 0xEF53, "Magic Ext2 inválido (esperado 0xEF53, obtido 0x{:04X})", magic);

    let state = u16::from_le_bytes([sb[58], sb[59]]);
    assert_eq!(state, 1, "Estado Ext2 deve ser 1 (Cleanly unmounted)");

    let rev_level = u32::from_le_bytes(sb[76..80].try_into().unwrap());
    assert_eq!(rev_level, 1, "Ext2 rev_level deve ser 1 (Dynamic rev)");

    let volume_name = String::from_utf8_lossy(&sb[120..136]);
    assert!(volume_name.starts_with("JINN_EXT2_VOL"), "Nome do volume Ext2 inesperado: {}", volume_name);
}
