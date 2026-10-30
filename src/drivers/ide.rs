#![allow(dead_code)]
/// Jinn OS — Driver de Disco ATA / IDE em Modo PIO (Primary Bus)
///
/// Implementa comunicação direta com o controlador de disco IDE/ATA via
/// portas de I/O em modo PIO (Programmed Input/Output).
///
/// Portas do Barramento Primário (Primary IDE Bus):
///   - 0x1F0: Porta de Dados (16 bits)
///   - 0x1F1: Features / Error
///   - 0x1F2: Contagem de Setores
///   - 0x1F3: LBA Low (bits 0-7)
///   - 0x1F4: LBA Mid (bits 8-15)
///   - 0x1F5: LBA High (bits 16-23)
///   - 0x1F6: Drive / Head / LBA bits 24-27
///   - 0x1F7: Comando / Status
///   - 0x3F6: Status Alternativo / Controle do Dispositivo

const IDE_DATA:         u16 = 0x1F0;
const IDE_FEATURES:     u16 = 0x1F1;
const IDE_SECTOR_COUNT: u16 = 0x1F2;
const IDE_LBA_LOW:      u16 = 0x1F3;
const IDE_LBA_MID:      u16 = 0x1F4;
const IDE_LBA_HIGH:     u16 = 0x1F5;
const IDE_DRIVE_HEAD:   u16 = 0x1F6;
const IDE_STATUS_CMD:   u16 = 0x1F7;
const IDE_CONTROL:      u16 = 0x3F6;

// Bits de Status (porta 0x1F7)
const STATUS_ERR: u8 = 1 << 0; // Erro na operação
const STATUS_DRQ: u8 = 1 << 3; // Data Request (pronto para transferência)
const STATUS_SRV: u8 = 1 << 4; // Service
const STATUS_DF:  u8 = 1 << 5; // Falha no dispositivo
const STATUS_RDY: u8 = 1 << 6; // Drive pronto
const STATUS_BSY: u8 = 1 << 7; // Ocupado executando comando

static IDE_LOCK: crate::memory::SpinLock<()> = crate::memory::SpinLock::new(());

// Comandos ATA
const CMD_READ_SECTORS:  u8 = 0x20;
const CMD_WRITE_SECTORS: u8 = 0x30;
const CMD_IDENTIFY:      u8 = 0xEC;
const CMD_IDENTIFY_PACKET: u8 = 0xA1; // ATAPI (CD-ROM)

// ── Funções de I/O de Baixo Nível ─────────────────────────────────────────────

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack));
    val
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack));
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    core::arch::asm!("in ax, dx", out("ax") val, in("dx") port, options(nomem, nostack));
    val
}

#[inline]
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack));
}

/// Aguarda aproximadamente 400ns lendo 4 vezes a porta de status alternativo.
/// Obrigatório pelo padrão ATA após selecionar o drive.
#[inline]
unsafe fn ide_delay() {
    inb(IDE_CONTROL);
    inb(IDE_CONTROL);
    inb(IDE_CONTROL);
    inb(IDE_CONTROL);
}

/// Aguarda o drive desocupar (BSY = 0) com timeout.
unsafe fn wait_not_busy() -> Result<u8, &'static str> {
    let mut timeout = 100_000usize;
    while timeout > 0 {
        let status = inb(IDE_STATUS_CMD);
        if (status & STATUS_BSY) == 0 {
            return Ok(status);
        }
        core::hint::spin_loop();
        timeout -= 1;
    }
    Err("Timeout aguardando BSY=0 no drive IDE")
}

/// Aguarda o drive solicitar dados (DRQ = 1) com timeout.
unsafe fn wait_drq() -> Result<u8, &'static str> {
    let mut timeout = 100_000usize;
    while timeout > 0 {
        let status = inb(IDE_STATUS_CMD);
        if (status & STATUS_ERR) != 0 {
            return Err("Erro reportado pelo controlador de disco (STATUS_ERR)");
        }
        if (status & STATUS_DF) != 0 {
            return Err("Falha reportada pelo disco (STATUS_DF)");
        }
        if (status & STATUS_BSY) == 0 && (status & STATUS_DRQ) != 0 {
            return Ok(status);
        }
        core::hint::spin_loop();
        timeout -= 1;
    }
    Err("Timeout aguardando DRQ=1 no drive IDE")
}

// ── Estruturas de Dados ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct DriveInfo {
    pub drive: u8,
    pub is_atapi: bool,
    pub model: [u8; 40],
    pub serial: [u8; 20],
    pub sectors_lba28: u32,
    pub size_mb: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct PartitionEntry {
    pub bootable: bool,
    pub partition_type: u8,
    pub start_lba: u32,
    pub sector_count: u32,
    pub size_mb: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct MbrInfo {
    pub is_valid: bool,
    pub partitions: [Option<PartitionEntry>; 4],
}

// ── Operações Principais do Driver ───────────────────────────────────────────

/// Identifica um disco no barramento primário (0 = Master, 1 = Slave).
pub fn identify(drive: u8) -> Result<DriveInfo, &'static str> {
    let _guard = IDE_LOCK.lock();
    unsafe {
        // Seleciona o drive (Master = 0xA0, Slave = 0xB0)
        let drive_sel = 0xA0 | ((drive & 1) << 4);
        outb(IDE_DRIVE_HEAD, drive_sel);
        ide_delay();

        // Zera registros LBA e Sector Count
        outb(IDE_SECTOR_COUNT, 0);
        outb(IDE_LBA_LOW, 0);
        outb(IDE_LBA_MID, 0);
        outb(IDE_LBA_HIGH, 0);

        // Envia comando IDENTIFY (0xEC)
        outb(IDE_STATUS_CMD, CMD_IDENTIFY);
        ide_delay();

        let status = inb(IDE_STATUS_CMD);
        if status == 0 {
            return Err("Nenhum dispositivo presente na porta IDE");
        }

        // Aguarda BSY=0
        let status = wait_not_busy()?;

        // Verifica se é um dispositivo ATAPI (CD-ROM/DVD)
        let mid = inb(IDE_LBA_MID);
        let high = inb(IDE_LBA_HIGH);
        let is_atapi = (mid == 0x14 && high == 0xEB) || (mid == 0x69 && high == 0x96);

        if is_atapi {
            // Para dispositivos ATAPI, usa o comando IDENTIFY_PACKET (0xA1)
            outb(IDE_STATUS_CMD, CMD_IDENTIFY_PACKET);
            ide_delay();
            wait_not_busy()?;
        } else if (status & STATUS_ERR) != 0 {
            return Err("Erro no comando IDENTIFY ATA");
        }

        // Aguarda o buffer de dados estar pronto (DRQ)
        wait_drq()?;

        // Lê 256 palavras de 16 bits (512 bytes)
        let mut id_buf = [0u16; 256];
        for word in id_buf.iter_mut() {
            *word = inw(IDE_DATA);
        }

        // Extrai número serial (palavras 10..=19, com bytes invertidos)
        let mut serial = [b' '; 20];
        for i in 0..10 {
            let w = id_buf[10 + i];
            serial[i * 2]     = (w >> 8) as u8;
            serial[i * 2 + 1] = (w & 0xFF) as u8;
        }

        // Extrai modelo do drive (palavras 27..=46, com bytes invertidos)
        let mut model = [b' '; 40];
        for i in 0..20 {
            let w = id_buf[27 + i];
            model[i * 2]     = (w >> 8) as u8;
            model[i * 2 + 1] = (w & 0xFF) as u8;
        }

        // Setores totais LBA28 (palavras 60 e 61)
        let sectors_lba28 = (id_buf[60] as u32) | ((id_buf[61] as u32) << 16);
        let size_mb = sectors_lba28 / 2048; // 512 bytes por setor => 2048 setores = 1 MB

        Ok(DriveInfo {
            drive,
            is_atapi,
            model,
            serial,
            sectors_lba28,
            size_mb,
        })
    }
}

/// Lê um setor físico de 512 bytes por endereço LBA28.
pub fn read_sector(drive: u8, lba: u32, buf: &mut [u8; 512]) -> Result<(), &'static str> {
    if lba >= 0x0FFF_FFFF {
        return Err("Endereço LBA excede o limite LBA28 (28 bits)");
    }

    let _guard = IDE_LOCK.lock();
    unsafe {
        wait_not_busy()?;

        // Configura modo LBA (bit 6 = 1) + bits 24..27 do LBA
        let drive_head = 0xE0 | ((drive & 1) << 4) | (((lba >> 24) & 0x0F) as u8);
        outb(IDE_DRIVE_HEAD, drive_head);
        ide_delay();

        outb(IDE_FEATURES, 0);      // Sem features extras
        outb(IDE_SECTOR_COUNT, 1);  // Lê exatamente 1 setor
        outb(IDE_LBA_LOW,  (lba & 0xFF) as u8);
        outb(IDE_LBA_MID,  ((lba >> 8) & 0xFF) as u8);
        outb(IDE_LBA_HIGH, ((lba >> 16) & 0xFF) as u8);

        // Dispara comando READ SECTORS (0x20)
        outb(IDE_STATUS_CMD, CMD_READ_SECTORS);
        ide_delay();

        // Aguarda transferência de dados
        wait_drq()?;

        // Transfere 256 palavras (512 bytes) da porta de dados
        for i in 0..256 {
            let word = inw(IDE_DATA);
            buf[i * 2]     = (word & 0xFF) as u8;
            buf[i * 2 + 1] = (word >> 8) as u8;
        }

        Ok(())
    }
}

/// Escreve um setor físico de 512 bytes por endereço LBA28.
pub fn write_sector(drive: u8, lba: u32, buf: &[u8; 512]) -> Result<(), &'static str> {
    if lba >= 0x0FFF_FFFF {
        return Err("Endereço LBA excede o limite LBA28 (28 bits)");
    }

    let _guard = IDE_LOCK.lock();
    unsafe {
        wait_not_busy()?;

        // Configura modo LBA (bit 6 = 1) + bits 24..27 do LBA
        let drive_head = 0xE0 | ((drive & 1) << 4) | (((lba >> 24) & 0x0F) as u8);
        outb(IDE_DRIVE_HEAD, drive_head);
        ide_delay();

        outb(IDE_FEATURES, 0);      // Sem features extras
        outb(IDE_SECTOR_COUNT, 1);  // Escreve exatamente 1 setor
        outb(IDE_LBA_LOW,  (lba & 0xFF) as u8);
        outb(IDE_LBA_MID,  ((lba >> 8) & 0xFF) as u8);
        outb(IDE_LBA_HIGH, ((lba >> 16) & 0xFF) as u8);

        // Dispara comando WRITE SECTORS (0x30)
        outb(IDE_STATUS_CMD, CMD_WRITE_SECTORS);
        ide_delay();

        // Aguarda a controladora solicitar os dados (DRQ = 1)
        wait_drq()?;

        // Transfere 256 palavras de 16 bits (512 bytes) para a porta de dados
        for i in 0..256 {
            let low = buf[i * 2] as u16;
            let high = buf[i * 2 + 1] as u16;
            let word = low | (high << 8);
            outw(IDE_DATA, word);
        }

        // Aguarda a gravação física na mídia (BSY = 0)
        wait_not_busy()?;

        // Valida se houve erro na controladora
        let status = inb(IDE_STATUS_CMD);
        if (status & STATUS_ERR) != 0 || (status & STATUS_DF) != 0 {
            return Err("Falha reportada pela controladora ao gravar setor no disco IDE");
        }

        Ok(())
    }
}

/// Lê e decodifica a tabela de partições do MBR (LBA 0).
pub fn read_mbr(drive: u8) -> Result<MbrInfo, &'static str> {
    let mut sector = [0u8; 512];
    read_sector(drive, 0, &mut sector)?;

    // Verifica assinatura de boot 0x55AA no final do setor
    let is_valid = sector[510] == 0x55 && sector[511] == 0xAA;
    if !is_valid {
        return Ok(MbrInfo {
            is_valid: false,
            partitions: [None; 4],
        });
    }

    let mut partitions = [None; 4];
    for i in 0..4 {
        let offset = 446 + (i * 16);
        let bootable = sector[offset] == 0x80;
        let ptype = sector[offset + 4];
        if ptype == 0 {
            continue; // Partição vazia/não particionada
        }

        let start_lba = (sector[offset + 8] as u32)
            | ((sector[offset + 9] as u32) << 8)
            | ((sector[offset + 10] as u32) << 16)
            | ((sector[offset + 11] as u32) << 24);

        let count = (sector[offset + 12] as u32)
            | ((sector[offset + 13] as u32) << 8)
            | ((sector[offset + 14] as u32) << 16)
            | ((sector[offset + 15] as u32) << 24);

        partitions[i] = Some(PartitionEntry {
            bootable,
            partition_type: ptype,
            start_lba,
            sector_count: count,
            size_mb: count / 2048,
        });
    }

    Ok(MbrInfo {
        is_valid: true,
        partitions,
    })
}

/// Retorna o nome amigável do tipo de partição MBR.
pub fn partition_type_name(ptype: u8) -> &'static str {
    match ptype {
        0x00 => "Vazia",
        0x01 => "FAT12",
        0x04 | 0x06 => "FAT16",
        0x07 => "NTFS / exFAT",
        0x0B | 0x0C => "FAT32",
        0x82 => "Linux Swap",
        0x83 => "Linux Native (Ext2/3/4)",
        0xEE => "GPT Protective",
        0xEF => "EFI System Partition",
        _    => "Desconhecida",
    }
}

/// Inicializa o subsistema IDE/ATA e verifica a integridade do barramento.
pub fn init() {
    unsafe {
        // Habilita interrupções e desativa software reset no controlador
        outb(IDE_CONTROL, 0x00);
    }
}
