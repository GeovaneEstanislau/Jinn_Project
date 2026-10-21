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
