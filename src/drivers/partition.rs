/// Jinn OS — Gerenciador de Partições e Volumes (MBR e GPT)
///
/// Implementa a decodificação segura de tabelas de partição MBR clássicas e GPT modernas,
/// além de fornecer a abstração `PartitionDevice` que impõe verificação estrita de limites
/// (boundary checking) para impedir que leituras e escritas ultrapassem os limites da partição.

use crate::drivers::block::{BlockDevice, DEFAULT_SECTOR_SIZE};

pub const MAX_PARTITIONS: usize = 8;
pub const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";

/// Descrição padronizada de uma partição em disco
#[derive(Debug, Clone, Copy)]
pub struct PartitionInfo {
    pub index: usize,
    pub name: [u8; 36],
    pub name_len: usize,
    pub start_lba: u64,
    pub sector_count: u64,
    pub partition_type: u8,
    pub is_bootable: bool,
    pub is_gpt: bool,
}

impl PartitionInfo {
    pub const fn empty() -> Self {
        Self {
            index: 0,
            name: [0u8; 36],
            name_len: 0,
            start_lba: 0,
            sector_count: 0,
            partition_type: 0,
            is_bootable: false,
            is_gpt: false,
        }
    }

    pub fn size_mb(&self) -> u64 {
        (self.sector_count * DEFAULT_SECTOR_SIZE as u64) / (1024 * 1024)
    }

    pub fn type_str(&self) -> &'static str {
        match self.partition_type {
            0x00 => "Vazia",
            0x01 => "FAT12",
            0x04 | 0x06 => "FAT16",
            0x07 => "NTFS / exFAT",
            0x0B | 0x0C => "FAT32",
            0x82 => "Linux Swap",
            0x83 => "Linux Native (ext4/btrfs)",
            0xEE => "GPT Protective MBR",
            0xEF => "EFI System Partition",
            _ => "Desconhecida",
        }
    }
}

/// Tabela consolidada de partições detectadas em uma unidade
#[derive(Debug, Clone, Copy)]
pub struct PartitionTable {
    pub partitions: [Option<PartitionInfo>; MAX_PARTITIONS],
    pub count: usize,
    pub is_gpt: bool,
}

impl PartitionTable {
    pub const fn new() -> Self {
        Self {
            partitions: [None; MAX_PARTITIONS],
            count: 0,
            is_gpt: false,
        }
    }

    pub fn add(&mut self, part: PartitionInfo) -> bool {
        if self.count < MAX_PARTITIONS {
            self.partitions[self.count] = Some(part);
            self.count += 1;
            true
        } else {
            false
        }
    }
}

// ── Dispositivo de Blocos Enclausurado na Partição (PartitionDevice) ──────────

/// Encapsula uma partição de um `BlockDevice` subjacente, garantindo isolamento absoluto de limites.
/// Qualquer leitura ou escrita que tente acessar setores fora de `0..sector_count` é rejeitada.
pub struct PartitionDevice<'a, D: BlockDevice> {
    pub inner: &'a D,
    pub name: &'static str,
    pub start_lba: u64,
    pub sector_count: u64,
    pub is_read_only: bool,
}

impl<'a, D: BlockDevice> PartitionDevice<'a, D> {
    pub fn new(inner: &'a D, name: &'static str, start_lba: u64, sector_count: u64, is_read_only: bool) -> Result<Self, &'static str> {
        let total = inner.total_blocks();
        if start_lba + sector_count > total {
            return Err("Particao ultrapassa o limite fisico total do dispositivo de blocos");
        }
        Ok(Self {
            inner,
            name,
            start_lba,
            sector_count,
            is_read_only,
        })
    }
}

impl<'a, D: BlockDevice> BlockDevice for PartitionDevice<'a, D> {
    fn name(&self) -> &'static str {
        self.name
    }

    fn block_size(&self) -> usize {
        self.inner.block_size()
    }

    fn total_blocks(&self) -> u64 {
        self.sector_count
    }

    fn read_blocks(&self, start_block: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
        if start_block + (count as u64) > self.sector_count {
            return Err("Tentativa de leitura alem dos limites desta particao");
        }
        self.inner.read_blocks(self.start_lba + start_block, count, buf)
    }

    fn write_blocks(&self, start_block: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
        if self.is_read_only {
            return Err("Operacao negada: Particao configurada como somente-leitura");
        }
        if start_block + (count as u64) > self.sector_count {
            return Err("Tentativa de escrita alem dos limites desta particao (protecao ativada)");
        }
        self.inner.write_blocks(self.start_lba + start_block, count, buf)
    }

    fn flush(&self) -> Result<(), &'static str> {
        self.inner.flush()
    }
}

// ── Varredura e Decodificação de Tabelas de Partição ──────────────────────────

/// Escaneia um dispositivo de blocos procurando por tabelas MBR ou GPT válidas.
pub fn scan_partitions<D: BlockDevice>(dev: &D) -> Result<PartitionTable, &'static str> {
    let mut table = PartitionTable::new();
    let mut sector0 = [0u8; DEFAULT_SECTOR_SIZE];

    dev.read_blocks(0, 1, &mut sector0)?;

    // Valida assinatura padrão de partição (0x55AA) no fim do setor 0
    if sector0[510] != 0x55 || sector0[511] != 0xAA {
        return Ok(table); // Disco sem partição inicializada
    }

    // 1. Verifica se é um GPT Protective MBR (tipo 0xEE na primeira entrada)
    let p1_type = sector0[446 + 4];
    if p1_type == 0xEE {
        // Tenta decodificar o cabeçalho GPT no LBA 1
        let mut gpt_header = [0u8; DEFAULT_SECTOR_SIZE];
        if dev.read_blocks(1, 1, &mut gpt_header).is_ok() && &gpt_header[..8] == GPT_SIGNATURE {
            table.is_gpt = true;
            parse_gpt(dev, &gpt_header, &mut table)?;
            return Ok(table);
        }
    }

    // 2. Decodifica MBR clássico (4 entradas de 16 bytes a partir do offset 446)
    for i in 0..4 {
        let offset = 446 + (i * 16);
        let boot_flag = sector0[offset];
        let p_type = sector0[offset + 4];
        let start_lba = (sector0[offset + 8] as u32)
            | ((sector0[offset + 9] as u32) << 8)
            | ((sector0[offset + 10] as u32) << 16)
            | ((sector0[offset + 11] as u32) << 24);
        let sector_count = (sector0[offset + 12] as u32)
            | ((sector0[offset + 13] as u32) << 8)
            | ((sector0[offset + 14] as u32) << 16)
            | ((sector0[offset + 15] as u32) << 24);

        if p_type != 0 && sector_count > 0 {
            let mut name = [0u8; 36];
            let label = b"Particao MBR ";
            name[..label.len()].copy_from_slice(label);
            name[label.len()] = b'1' + (i as u8);
            let name_len = label.len() + 1;

            table.add(PartitionInfo {
                index: i + 1,
                name,
                name_len,
                start_lba: start_lba as u64,
                sector_count: sector_count as u64,
                partition_type: p_type,
                is_bootable: boot_flag == 0x80,
                is_gpt: false,
            });
        }
    }

    Ok(table)
}

/// Decodifica entradas de partição GPT a partir do LBA indicado no cabeçalho
fn parse_gpt<D: BlockDevice>(dev: &D, header: &[u8; DEFAULT_SECTOR_SIZE], table: &mut PartitionTable) -> Result<(), &'static str> {
    let entry_lba = (header[72] as u64)
        | ((header[73] as u64) << 8)
        | ((header[74] as u64) << 16)
        | ((header[75] as u64) << 24)
        | ((header[76] as u64) << 32)
        | ((header[77] as u64) << 40)
        | ((header[78] as u64) << 48)
        | ((header[79] as u64) << 56);

    let num_entries = (header[80] as u32)
        | ((header[81] as u32) << 8)
        | ((header[82] as u32) << 16)
        | ((header[83] as u32) << 24);

    let entry_size = (header[84] as u32)
        | ((header[85] as u32) << 8)
        | ((header[86] as u32) << 16)
        | ((header[87] as u32) << 24);

    if entry_size < 128 || entry_lba == 0 {
        return Err("Estrutura do cabecalho GPT inconsistente");
    }

    let mut entry_buf = [0u8; DEFAULT_SECTOR_SIZE];
    let entries_per_sector = (DEFAULT_SECTOR_SIZE / entry_size as usize).max(1);
    let sectors_to_read = ((num_entries as usize + entries_per_sector - 1) / entries_per_sector).min(8);

    for s in 0..sectors_to_read {
        dev.read_blocks(entry_lba + s as u64, 1, &mut entry_buf)?;

        for e in 0..entries_per_sector {
            let offset = e * (entry_size as usize);
            if offset + 128 > DEFAULT_SECTOR_SIZE { break; }

            // Verifica se o Type GUID não é nulo
            let mut is_null_guid = true;
            for b in 0..16 {
                if entry_buf[offset + b] != 0 {
                    is_null_guid = false;
                    break;
                }
            }
            if is_null_guid { continue; }

            let start_lba = (entry_buf[offset + 32] as u64)
                | ((entry_buf[offset + 33] as u64) << 8)
                | ((entry_buf[offset + 34] as u64) << 16)
                | ((entry_buf[offset + 35] as u64) << 24)
                | ((entry_buf[offset + 36] as u64) << 32)
                | ((entry_buf[offset + 37] as u64) << 40)
                | ((entry_buf[offset + 38] as u64) << 48)
                | ((entry_buf[offset + 39] as u64) << 56);

            let end_lba = (entry_buf[offset + 40] as u64)
                | ((entry_buf[offset + 41] as u64) << 8)
                | ((entry_buf[offset + 42] as u64) << 16)
                | ((entry_buf[offset + 43] as u64) << 24)
                | ((entry_buf[offset + 44] as u64) << 32)
                | ((entry_buf[offset + 45] as u64) << 40)
                | ((entry_buf[offset + 46] as u64) << 48)
                | ((entry_buf[offset + 47] as u64) << 56);

            if end_lba >= start_lba {
                let sector_count = end_lba - start_lba + 1;
                let mut name = [0u8; 36];
                let label = b"Particao GPT ";
                name[..label.len()].copy_from_slice(label);
                name[label.len()] = b'1' + (table.count as u8);
                let name_len = label.len() + 1;

                table.add(PartitionInfo {
                    index: table.count + 1,
                    name,
                    name_len,
                    start_lba,
                    sector_count,
                    partition_type: 0xEE,
                    is_bootable: false,
                    is_gpt: true,
                });
            }
        }
    }

    Ok(())
}
