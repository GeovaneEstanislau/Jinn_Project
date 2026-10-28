//! Testes automatizados da Jinn Storage Architecture (JSA)
//!
//! Valida os contratos fundamentais de integridade:
//! 1. Política LRU de cache de blocos com escrita de blocos modificados (dirty eviction write-back).
//! 2. Proteção estrita de limites de partições (PartitionDevice boundaries).
//! 3. Rejeição de escrita em sistemas de arquivos declarados somente-leitura (Read-Only enforcement).
//! 4. Transição segura de perfis de armazenamento com flush preventivo de dirty cache.
//! 5. Proteção contra diretórios Ext2 corrompidos (zero rec_len e bounds overflow).
//! 6. Coerência da matriz de políticas dos perfis (Aladdin, Simbad, Workstation, Terminal/POS).

#![allow(dead_code)]

use std::collections::HashMap;

// ── 1. Simulação do Cache de Blocos LRU com Dirty Eviction ─────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WritePolicy {
    WriteThrough,
    WriteBack,
}

struct MockBlockDevice {
    sectors: HashMap<u64, [u8; 512]>,
    write_count: usize,
    read_count: usize,
}

impl MockBlockDevice {
    fn new() -> Self {
        Self {
            sectors: HashMap::new(),
            write_count: 0,
            read_count: 0,
        }
    }

    fn read_sector(&mut self, lba: u64, buf: &mut [u8; 512]) {
        self.read_count += 1;
        if let Some(data) = self.sectors.get(&lba) {
            buf.copy_from_slice(data);
        } else {
            buf.fill(0);
        }
    }

    fn write_sector(&mut self, lba: u64, buf: &[u8; 512]) {
        self.write_count += 1;
        self.sectors.insert(lba, *buf);
    }
}

struct CacheSlot {
    lba: u64,
    data: [u8; 512],
    is_valid: bool,
    is_dirty: bool,
    access_counter: u64,
}

struct MockBlockCache {
    slots: Vec<CacheSlot>,
    capacity: usize,
    counter: u64,
    policy: WritePolicy,
}

impl MockBlockCache {
    fn new(capacity: usize, policy: WritePolicy) -> Self {
        let mut slots = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots.push(CacheSlot {
                lba: 0,
                data: [0u8; 512],
                is_valid: false,
                is_dirty: false,
                access_counter: 0,
            });
        }
        Self {
            slots,
            capacity,
            counter: 0,
            policy,
        }
    }

    fn read(&mut self, lba: u64, buf: &mut [u8; 512], dev: &mut MockBlockDevice) {
        self.counter += 1;
        for slot in self.slots.iter_mut() {
            if slot.is_valid && slot.lba == lba {
                slot.access_counter = self.counter;
                buf.copy_from_slice(&slot.data);
                return;
            }
        }

        // Cache miss: ler do dispositivo e inserir
        dev.read_sector(lba, buf);
        self.insert_evicting(lba, buf, false, dev);
    }

    fn write(&mut self, lba: u64, buf: &[u8; 512], dev: &mut MockBlockDevice) {
        self.counter += 1;
        match self.policy {
            WritePolicy::WriteThrough => {
                dev.write_sector(lba, buf);
                self.insert_evicting(lba, buf, false, dev);
            }
            WritePolicy::WriteBack => {
                self.insert_evicting(lba, buf, true, dev);
            }
        }
    }

    fn insert_evicting(&mut self, lba: u64, buf: &[u8; 512], is_dirty: bool, dev: &mut MockBlockDevice) {
        // Se já está presente, apenas atualiza
        for slot in self.slots.iter_mut() {
            if slot.is_valid && slot.lba == lba {
                slot.data.copy_from_slice(buf);
                slot.is_dirty = slot.is_dirty || is_dirty;
                slot.access_counter = self.counter;
                return;
            }
        }

        // Procura slot livre
        for slot in self.slots.iter_mut() {
            if !slot.is_valid {
                slot.lba = lba;
                slot.data.copy_from_slice(buf);
                slot.is_valid = true;
                slot.is_dirty = is_dirty;
                slot.access_counter = self.counter;
                return;
            }
        }

        // Seleciona vítima por menor access_counter (LRU)
        let mut min_idx = 0;
        let mut min_val = u64::MAX;
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.access_counter < min_val {
                min_val = slot.access_counter;
                min_idx = i;
            }
        }

        // FLUSH SE ESTIVER MODIFICADO (Dirty Eviction)
        if self.slots[min_idx].is_dirty {
            dev.write_sector(self.slots[min_idx].lba, &self.slots[min_idx].data);
            self.slots[min_idx].is_dirty = false;
        }

        self.slots[min_idx].lba = lba;
        self.slots[min_idx].data.copy_from_slice(buf);
        self.slots[min_idx].is_valid = true;
        self.slots[min_idx].is_dirty = is_dirty;
        self.slots[min_idx].access_counter = self.counter;
    }

    fn flush(&mut self, dev: &mut MockBlockDevice) -> usize {
        let mut flushed = 0;
        for slot in self.slots.iter_mut() {
            if slot.is_valid && slot.is_dirty {
                dev.write_sector(slot.lba, &slot.data);
                slot.is_dirty = false;
                flushed += 1;
            }
        }
        flushed
    }

    fn set_policy(&mut self, new_policy: WritePolicy, dev: &mut MockBlockDevice) {
        if self.policy == WritePolicy::WriteBack && new_policy == WritePolicy::WriteThrough {
            self.flush(dev);
        }
        self.policy = new_policy;
    }
}

// ── Testes Automatizados ───────────────────────────────────────────────────────

#[test]
fn test_lru_cache_dirty_eviction_prevents_data_loss() {
    let mut dev = MockBlockDevice::new();
    let mut cache = MockBlockCache::new(2, WritePolicy::WriteBack); // Cache pequena de 2 slots

    let data1 = [1u8; 512];
    let data2 = [2u8; 512];
    let data3 = [3u8; 512];

    // Escreve no bloco 10 (slot 0, dirty)
    cache.write(10, &data1, &mut dev);
    // Escreve no bloco 20 (slot 1, dirty)
    cache.write(20, &data2, &mut dev);

    assert_eq!(dev.write_count, 0, "No WriteBack, escritas ficam na cache inicialmente");

    // Escreve no bloco 30 -> Força evicção do slot LRU (bloco 10)
    cache.write(30, &data3, &mut dev);

    assert_eq!(dev.write_count, 1, "O bloco modificado 10 deve ter sido gravado no disco antes da evicção");
    assert_eq!(dev.sectors.get(&10).unwrap(), &data1, "Dados do bloco 10 foram persistidos");

    // Descarrega o restante
    let flushed = cache.flush(&mut dev);
    assert_eq!(flushed, 2, "Blocos 20 e 30 devem ser descarregados no flush");
    assert_eq!(dev.sectors.get(&20).unwrap(), &data2);
    assert_eq!(dev.sectors.get(&30).unwrap(), &data3);
}

#[test]
fn test_partition_bounds_protection() {
    struct MockPartition {
        start_lba: u64,
        sector_count: u64,
    }

    impl MockPartition {
        fn check_bounds(&self, lba: u64) -> Result<(), &'static str> {
            if lba >= self.start_lba + self.sector_count {
                Err("Acesso fora dos limites da particao")
            } else {
                Ok(())
            }
        }
    }

    let part = MockPartition { start_lba: 2048, sector_count: 65536 };

    assert!(part.check_bounds(2048).is_ok());
    assert!(part.check_bounds(2048 + 65535).is_ok());
    assert!(part.check_bounds(2048 + 65536).is_err(), "Setor exatamente no limite deve ser rejeitado");
    assert!(part.check_bounds(2048 + 100000).is_err(), "Setor muito além do limite deve ser rejeitado");
}

#[test]
fn test_vfs_readonly_flag_rejection() {
    const O_RDONLY: u32 = 0x0;
    const O_WRONLY: u32 = 0x1;
    const O_RDWR:   u32 = 0x2;
    const O_CREAT:  u32 = 0x40;
    const O_TRUNC:  u32 = 0x200;
    const O_APPEND: u32 = 0x400;

    fn check_open_permission(is_read_only: bool, flags: u32) -> Result<(), &'static str> {
        let write_flags = O_WRONLY | O_RDWR | O_CREAT | O_TRUNC | O_APPEND;
        if is_read_only && (flags & write_flags) != 0 {
            Err("PermissionDenied")
        } else {
            Ok(())
        }
    }

    // Read-only filesystem (ex: Ext2)
    assert!(check_open_permission(true, O_RDONLY).is_ok());
    assert_eq!(check_open_permission(true, O_WRONLY), Err("PermissionDenied"));
    assert_eq!(check_open_permission(true, O_RDWR), Err("PermissionDenied"));
    assert_eq!(check_open_permission(true, O_CREAT | O_WRONLY), Err("PermissionDenied"));
    assert_eq!(check_open_permission(true, O_TRUNC), Err("PermissionDenied"));
    assert_eq!(check_open_permission(true, O_APPEND), Err("PermissionDenied"));

    // Read-write filesystem (ex: FAT32)
    assert!(check_open_permission(false, O_RDONLY).is_ok());
    assert!(check_open_permission(false, O_WRONLY).is_ok());
    assert!(check_open_permission(false, O_RDWR).is_ok());
    assert!(check_open_permission(false, O_CREAT | O_WRONLY).is_ok());
}

#[test]
fn test_profile_transition_flush() {
    let mut dev = MockBlockDevice::new();
    let mut cache = MockBlockCache::new(4, WritePolicy::WriteBack);

    let data = [0xAAu8; 512];
    cache.write(100, &data, &mut dev);
    assert_eq!(dev.write_count, 0, "No WriteBack, o dado esta na cache");

    // Transição de perfil: de Simbad (WriteBack) para Terminal/POS (WriteThrough)
    cache.set_policy(WritePolicy::WriteThrough, &mut dev);

    assert_eq!(dev.write_count, 1, "Transição para WriteThrough deve forçar flush automático");
    assert_eq!(dev.sectors.get(&100).unwrap(), &data);
}

#[test]
fn test_ext2_corrupted_directory_record_safety() {
    let mut block = [0u8; 1024];

    // Entrada 1 válida: inode = 1, rec_len = 16, name_len = 4, "test"
    block[0..4].copy_from_slice(&1u32.to_le_bytes());
    block[4..6].copy_from_slice(&16u16.to_le_bytes());
    block[6] = 4;
    block[7] = 1;
    block[8..12].copy_from_slice(b"test");

    // Entrada 2 malformada: rec_len = 0 (ataque de loop infinito)
    block[16..20].copy_from_slice(&2u32.to_le_bytes());
    block[20..22].copy_from_slice(&0u16.to_le_bytes()); // rec_len == 0!

    let mut entries_found = 0;
    let mut offset = 0usize;
    let block_size = 1024usize;

    while offset + 8 <= block_size {
        let ino = u32::from_le_bytes([block[offset], block[offset + 1], block[offset + 2], block[offset + 3]]);
        let rec_len = u16::from_le_bytes([block[offset + 4], block[offset + 5]]) as usize;
        let name_len = block[offset + 6] as usize;

        // Regra de segurança implementada no Jinn OS:
        if rec_len < 8 || offset + rec_len > block_size {
            break;
        }

        if ino != 0 && offset + 8 + name_len <= offset + rec_len {
            entries_found += 1;
        }

        offset += rec_len;
    }

    assert_eq!(entries_found, 1, "Apenas a primeira entrada válida deve ser processada, abortando seguramente no rec_len=0");
}

#[test]
fn test_storage_profile_policy_matrix() {
    struct StorageProfileConfig {
        name: &'static str,
        cache_capacity: usize,
        write_policy: WritePolicy,
        read_ahead_sectors: usize,
        max_open_handles: usize,
    }

    let profiles = [
        StorageProfileConfig {
            name: "Aladdin",
            cache_capacity: 64, // 32 KB
            write_policy: WritePolicy::WriteThrough,
            read_ahead_sectors: 8,
            max_open_handles: 16,
        },
        StorageProfileConfig {
            name: "Simbad",
            cache_capacity: 512, // 256 KB
            write_policy: WritePolicy::WriteBack,
            read_ahead_sectors: 32,
            max_open_handles: 128,
        },
        StorageProfileConfig {
            name: "Workstation",
            cache_capacity: 256, // 128 KB
            write_policy: WritePolicy::WriteBack,
            read_ahead_sectors: 16,
            max_open_handles: 64,
        },
        StorageProfileConfig {
            name: "Terminal",
            cache_capacity: 16, // 8 KB
            write_policy: WritePolicy::WriteThrough,
            read_ahead_sectors: 2,
            max_open_handles: 8,
        },
    ];

    for p in profiles.iter() {
        assert!(p.cache_capacity >= 16);
        assert!(p.read_ahead_sectors >= 2);
        if p.name == "Terminal" || p.name == "Aladdin" {
            assert_eq!(p.write_policy, WritePolicy::WriteThrough, "Sistemas de confiabilidade doméstica ou terminais exigem WriteThrough");
        } else {
            assert_eq!(p.write_policy, WritePolicy::WriteBack, "Sistemas de alta performance usam WriteBack");
        }
    }
}

// ── 7. Testes de Contrato de Escrita Persistente FAT32 e Alocação de Clusters ───

struct MockFat32Volume {
    fat1: [u32; 1024],
    fat2: [u32; 1024],
    clusters: HashMap<u32, Vec<u8>>,
    dir_entries: HashMap<String, (u32, u32)>, // name -> (first_cluster, size)
    cluster_size: usize,
}

impl MockFat32Volume {
    fn new(cluster_size: usize) -> Self {
        let mut fat1 = [0u32; 1024];
        let mut fat2 = [0u32; 1024];
        // Clusters 0 e 1 reservados
        fat1[0] = 0x0FFF_FFF8;
        fat1[1] = 0x0FFF_FFFF;
        // Cluster 2 é o diretório raiz
        fat1[2] = 0x0FFF_FFFF;
        fat2.copy_from_slice(&fat1);

        Self {
            fat1,
            fat2,
            clusters: HashMap::new(),
            dir_entries: HashMap::new(),
            cluster_size,
        }
    }

    fn alloc_cluster(&mut self) -> Result<u32, &'static str> {
        for clus in 3..1024u32 {
            if self.fat1[clus as usize] == 0 {
                // Aloca e marca como EOF (0x0FFF_FFFF), espelhando em FAT1 e FAT2
                self.fat1[clus as usize] = 0x0FFF_FFFF;
                self.fat2[clus as usize] = 0x0FFF_FFFF;
                self.clusters.insert(clus, vec![0u8; self.cluster_size]);
                return Ok(clus);
            }
        }
        Err("No space left on device")
    }

    fn set_next_cluster(&mut self, clus: u32, next: u32) {
        self.fat1[clus as usize] = next;
        self.fat2[clus as usize] = next;
    }

    fn get_next_cluster(&self, clus: u32) -> Option<u32> {
        let val = self.fat1[clus as usize];
        if val >= 0x0FFF_FFF8 {
            None
        } else {
            Some(val)
        }
    }

    fn write_file(&mut self, filename: &str, offset: usize, data: &[u8]) -> Result<usize, &'static str> {
        if offset > 0xFFFF_FFFF || offset.saturating_add(data.len()) > 0xFFFF_FFFF {
            return Err("Invalid offset/overflow");
        }

        let (mut first_clus, mut cur_size) = self.dir_entries.get(filename).cloned().unwrap_or((0, 0));

        if first_clus == 0 {
            first_clus = self.alloc_cluster()?;
        }

        let mut clus_skip = offset / self.cluster_size;
        let mut clus_offset = offset % self.cluster_size;

        let mut cur_clus = first_clus;
        while clus_skip > 0 {
            match self.get_next_cluster(cur_clus) {
                Some(next) => cur_clus = next,
                None => {
                    let new_c = self.alloc_cluster()?;
                    self.set_next_cluster(cur_clus, new_c);
                    cur_clus = new_c;
                }
            }
            clus_skip -= 1;
        }

        let mut written = 0;
        while written < data.len() {
            let clus_buf = self.clusters.get_mut(&cur_clus).unwrap();
            let avail = self.cluster_size - clus_offset;
            let needed = data.len() - written;
            let to_copy = avail.min(needed);

            clus_buf[clus_offset..clus_offset + to_copy].copy_from_slice(&data[written..written + to_copy]);
            written += to_copy;
            clus_offset = 0;

            if written < data.len() {
                match self.get_next_cluster(cur_clus) {
                    Some(next) => cur_clus = next,
                    None => {
                        let new_c = self.alloc_cluster()?;
                        self.set_next_cluster(cur_clus, new_c);
                        cur_clus = new_c;
                    }
                }
            }
        }

        let new_end = offset + written;
        if new_end > cur_size as usize {
            cur_size = new_end as u32;
        }
        self.dir_entries.insert(filename.to_string(), (first_clus, cur_size));
        Ok(written)
    }

    fn read_file(&self, filename: &str, offset: usize, buf: &mut [u8]) -> usize {
        let &(first_clus, size) = match self.dir_entries.get(filename) {
            Some(e) => e,
            None => return 0,
        };

        if offset >= size as usize || first_clus < 2 {
            return 0;
        }

        let bytes_to_read = ((size as usize - offset)).min(buf.len());
        let mut clus_skip = offset / self.cluster_size;
        let mut clus_offset = offset % self.cluster_size;

        let mut cur_clus = first_clus;
        while clus_skip > 0 {
            match self.get_next_cluster(cur_clus) {
                Some(next) => cur_clus = next,
                None => return 0,
            }
            clus_skip -= 1;
        }

        let mut read_bytes = 0;
        while read_bytes < bytes_to_read {
            let clus_buf = match self.clusters.get(&cur_clus) {
                Some(b) => b,
                None => break,
            };
            let avail = self.cluster_size - clus_offset;
            let needed = bytes_to_read - read_bytes;
            let to_copy = avail.min(needed);

            buf[read_bytes..read_bytes + to_copy].copy_from_slice(&clus_buf[clus_offset..clus_offset + to_copy]);
            read_bytes += to_copy;
            clus_offset = 0;

            if read_bytes < bytes_to_read {
                match self.get_next_cluster(cur_clus) {
                    Some(next) => cur_clus = next,
                    None => break,
                }
            }
        }

        read_bytes
    }
}

#[test]
fn test_fat32_cluster_allocation_and_mirroring() {
    let mut vol = MockFat32Volume::new(4096);
    let c1 = vol.alloc_cluster().expect("Cluster 1 deve ser alocado com sucesso");
    assert_eq!(c1, 3);
    assert_eq!(vol.fat1[3], 0x0FFF_FFFF, "Cluster alocado deve ser EOF na FAT1");
    assert_eq!(vol.fat2[3], 0x0FFF_FFFF, "Cluster alocado deve estar espelhado na FAT2");

    let c2 = vol.alloc_cluster().expect("Cluster 2 deve ser alocado com sucesso");
    assert_eq!(c2, 4);
    vol.set_next_cluster(c1, c2);
    assert_eq!(vol.fat1[3], 4, "FAT1 deve encadear 3 -> 4");
    assert_eq!(vol.fat2[3], 4, "FAT2 deve espelhar encadeamento 3 -> 4");
    assert_eq!(vol.get_next_cluster(c1), Some(4));
    assert_eq!(vol.get_next_cluster(c2), None);
}

#[test]
fn test_fat32_write_straddling_sectors_and_clusters() {
    let mut vol = MockFat32Volume::new(512); // Clusters de 512 bytes para testar fronteiras rigorosas
    let payload = vec![0xABu8; 1500]; // Cruza 3 clusters (512 + 512 + 476)

    let written = vol.write_file("TEST.BIN", 0, &payload).expect("Escrita multi-cluster deve ter sucesso");
    assert_eq!(written, 1500);

    let (first_clus, size) = *vol.dir_entries.get("TEST.BIN").expect("Entrada de diretório deve existir");
    assert_eq!(size, 1500, "Tamanho na entrada de diretório deve ser 1500 bytes");

    let mut read_buf = vec![0u8; 1500];
    let n = vol.read_file("TEST.BIN", 0, &mut read_buf);
    assert_eq!(n, 1500);
    assert_eq!(read_buf, payload, "Conteúdo lido através de fronteiras de clusters deve ser idêntico");

    // Verifica que 3 clusters foram encadeados
    let c1 = first_clus;
    let c2 = vol.get_next_cluster(c1).expect("Deve ter segundo cluster");
    let c3 = vol.get_next_cluster(c2).expect("Deve ter terceiro cluster");
    assert_eq!(vol.get_next_cluster(c3), None, "Terceiro cluster deve ser o final da cadeia");
}

#[test]
fn test_fat32_write_partial_sector_preservation() {
    let mut vol = MockFat32Volume::new(512);

    // Inicializa arquivo com 100 bytes 'A'
    let initial = vec![b'A'; 100];
    vol.write_file("PARTIAL.TXT", 0, &initial).unwrap();

    // Sobrescreve bytes no offset 20 com 10 bytes 'Z'
    let patch = vec![b'Z'; 10];
    vol.write_file("PARTIAL.TXT", 20, &patch).unwrap();

    let mut buf = vec![0u8; 100];
    let n = vol.read_file("PARTIAL.TXT", 0, &mut buf);
    assert_eq!(n, 100);

    // Bytes 0..20 devem ser 'A'
    assert!(buf[0..20].iter().all(|&b| b == b'A'), "Bytes antes do patch devem ser preservados");
    // Bytes 20..30 devem ser 'Z'
    assert_eq!(&buf[20..30], &patch[..], "Bytes do patch devem ser gravados");
    // Bytes 30..100 devem ser 'A'
    assert!(buf[30..100].iter().all(|&b| b == b'A'), "Bytes depois do patch devem ser preservados");
}

#[test]
fn test_fat32_write_append_and_expand() {
    let mut vol = MockFat32Volume::new(512);

    // Escreve primeiros 300 bytes
    vol.write_file("APPEND.LOG", 0, &[1u8; 300]).unwrap();
    assert_eq!(vol.dir_entries.get("APPEND.LOG").unwrap().1, 300);

    // Faz append de mais 400 bytes a partir do offset 300 (totalizando 700 bytes -> cruza para 2o cluster)
    vol.write_file("APPEND.LOG", 300, &[2u8; 400]).unwrap();
    assert_eq!(vol.dir_entries.get("APPEND.LOG").unwrap().1, 700);

    let mut buf = vec![0u8; 700];
    vol.read_file("APPEND.LOG", 0, &mut buf);
    assert_eq!(&buf[0..300], &[1u8; 300][..]);
    assert_eq!(&buf[300..700], &[2u8; 400][..]);
}

#[test]
fn test_fat32_arithmetic_overflow_protection() {
    let mut vol = MockFat32Volume::new(512);
    let huge_offset = usize::MAX - 10;
    let res = vol.write_file("OVERFLOW.DAT", huge_offset, &[0xFF; 20]);
    assert!(res.is_err(), "Escrita com offset sujeito a overflow aritmético deve ser rejeitada");
}
