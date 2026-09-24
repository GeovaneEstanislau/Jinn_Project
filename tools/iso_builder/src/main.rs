use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const SECTOR_SIZE: usize = 2048;

fn u16_both(v: u16) -> [u8; 4] {
    let lsb = v.to_le_bytes();
    let msb = v.to_be_bytes();
    [lsb[0], lsb[1], msb[0], msb[1]]
}

fn u32_both(v: u32) -> [u8; 8] {
    let lsb = v.to_le_bytes();
    let msb = v.to_be_bytes();
    [lsb[0], lsb[1], lsb[2], lsb[3], msb[0], msb[1], msb[2], msb[3]]
}

struct IsoFile {
    path_in_iso: String, // e.g. "LIMINE_BIOS_CD.BIN;1" or "BOOT/JINN_KERNEL;1"
    disk_path: PathBuf,
    size: usize,
    sector: u32,
}

fn main() -> io::Result<()> {
    println!("=== Jinn OS ISO Builder (ISO9660 + El Torito / Limine) ===");

    let root_dir = Path::new(".").canonicalize()?;
    let iso_output = root_dir.join("jinn.iso");

    // Gather boot components
    let limine_cd = root_dir.join("limine/usr/share/limine/limine-bios-cd.bin");
    let limine_sys = root_dir.join("limine/usr/share/limine/limine-bios.sys");
    let limine_cfg = root_dir.join("iso_root/limine.conf");
    let kernel_elf = root_dir.join("boot/kernel.elf");
    let bootx64_efi = root_dir.join("limine/usr/share/limine/BOOTX64.EFI");

    if !limine_cd.exists() {
        eprintln!("Error: limine-bios-cd.bin not found at {:?}", limine_cd);
        std::process::exit(1);
    }
    if !limine_sys.exists() {
        eprintln!("Error: limine-bios.sys not found at {:?}", limine_sys);
        std::process::exit(1);
    }
    if !kernel_elf.exists() {
        eprintln!("Error: kernel.elf not found at {:?}", kernel_elf);
        std::process::exit(1);
    }

    println!("Found components:");
    println!("  - Bootloader CD  : {:?}", limine_cd);
    println!("  - Bootloader SYS : {:?}", limine_sys);
    println!("  - Limine Config  : {:?}", limine_cfg);
    println!("  - Kernel ELF     : {:?}", kernel_elf);
    if bootx64_efi.exists() {
        println!("  - UEFI Bootloader: {:?}", bootx64_efi);
    }

    let mut files = Vec::new();

    // Add root files
    files.push(IsoFile {
        path_in_iso: "LIMINE_B.BIN;1".into(),
        disk_path: limine_cd.clone(),
        size: fs::metadata(&limine_cd)?.len() as usize,
        sector: 0,
    });
    files.push(IsoFile {
        path_in_iso: "LIMINE_B.SYS;1".into(),
        disk_path: limine_sys.clone(),
        size: fs::metadata(&limine_sys)?.len() as usize,
        sector: 0,
    });
    files.push(IsoFile {
        path_in_iso: "LIMINE.CON;1".into(),
        disk_path: limine_cfg.clone(),
        size: fs::metadata(&limine_cfg)?.len() as usize,
        sector: 0,
    });
    files.push(IsoFile {
        path_in_iso: "BOOT/JINN_KER.;1".into(),
        disk_path: kernel_elf.clone(),
        size: fs::metadata(&kernel_elf)?.len() as usize,
        sector: 0,
    });

    if bootx64_efi.exists() {
        files.push(IsoFile {
            path_in_iso: "EFI/BOOT/BOOTX64.EFI;1".into(),
            disk_path: bootx64_efi.clone(),
            size: fs::metadata(&bootx64_efi)?.len() as usize,
            sector: 0,
        });
    }

    // Sector layout:
    // 0..15: System area (32 KB)
    // 16: Primary Volume Descriptor (PVD)
    // 17: Boot Record Volume Descriptor (El Torito)
    // 18: Volume Descriptor Set Terminator
    // 19: Boot Catalog (El Torito)
    // 20: Type L Path Table
    // 21: Type M Path Table
    // 22: Root Directory
    // 23: BOOT Directory
    // 24: EFI Directory
    // 25: EFI/BOOT Directory
    // 26+: File data sectors

    let pvd_sector = 16u32;
    let boot_desc_sector = 17u32;
    let term_sector = 18u32;
    let boot_catalog_sector = 19u32;
    let path_table_l_sector = 20u32;
    let path_table_m_sector = 21u32;
    let root_dir_sector = 22u32;
    let boot_dir_sector = 23u32;
    let efi_dir_sector = 24u32;
    let efi_boot_dir_sector = 25u32;

    let mut current_file_sector = 26u32;
    for f in &mut files {
        f.sector = current_file_sector;
        let sectors_needed = (f.size + SECTOR_SIZE - 1) / SECTOR_SIZE;
        current_file_sector += sectors_needed as u32;
    }

    let total_sectors = current_file_sector + 1;
    println!("Total ISO size: {} sectors ({} bytes)", total_sectors, total_sectors as usize * SECTOR_SIZE);

    let mut iso = vec![0u8; total_sectors as usize * SECTOR_SIZE];

    // ── Sector 16: Primary Volume Descriptor ─────────────────────────────────
    {
        let offset = pvd_sector as usize * SECTOR_SIZE;
        let pvd = &mut iso[offset..offset + SECTOR_SIZE];
        pvd[0] = 0x01; // Type: PVD
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[6] = 0x01; // Version
        pvd[8..40].copy_from_slice(format!("{:<32}", "LINUX").as_bytes());
        pvd[40..72].copy_from_slice(format!("{:<32}", "JINN").as_bytes());
        pvd[80..88].copy_from_slice(&u32_both(total_sectors));
        pvd[120..124].copy_from_slice(&u16_both(1)); // Set size
        pvd[124..128].copy_from_slice(&u16_both(1)); // Seq number
        pvd[128..132].copy_from_slice(&u16_both(SECTOR_SIZE as u16)); // Block size
        pvd[132..140].copy_from_slice(&u32_both(64)); // Path table size (bytes)
        pvd[140..144].copy_from_slice(&path_table_l_sector.to_le_bytes());
        pvd[148..152].copy_from_slice(&path_table_m_sector.to_be_bytes());

        // Root directory record in PVD (34 bytes)
        write_dir_record(&mut pvd[156..190], root_dir_sector, SECTOR_SIZE as u32, true, b"\x00");

        pvd[190..318].copy_from_slice(format!("{:<128}", "JINN_SET").as_bytes());
        pvd[318..446].copy_from_slice(format!("{:<128}", "JINN_OS_TEAM").as_bytes());
        pvd[446..574].copy_from_slice(format!("{:<128}", "JINN_PREPARER").as_bytes());
        pvd[574..702].copy_from_slice(format!("{:<128}", "JINN_APP").as_bytes());
        pvd[881] = 0x01; // File structure version
    }

    // ── Sector 17: Boot Record Volume Descriptor (El Torito) ─────────────────
    {
        let offset = boot_desc_sector as usize * SECTOR_SIZE;
        let br = &mut iso[offset..offset + SECTOR_SIZE];
        br[0] = 0x00; // Type: Boot Record
        br[1..6].copy_from_slice(b"CD001");
        br[6] = 0x01; // Version
        br[7..39].copy_from_slice(b"EL TORITO SPECIFICATION\0\0\0\0\0\0\0\0");
        br[71..75].copy_from_slice(&boot_catalog_sector.to_le_bytes()); // Boot catalog sector
    }

    // ── Sector 18: Volume Descriptor Set Terminator ──────────────────────────
    {
        let offset = term_sector as usize * SECTOR_SIZE;
        let term = &mut iso[offset..offset + SECTOR_SIZE];
        term[0] = 0xff;
        term[1..6].copy_from_slice(b"CD001");
        term[6] = 0x01;
    }

    // ── Sector 19: Boot Catalog (El Torito) ──────────────────────────────────
    {
        let offset = boot_catalog_sector as usize * SECTOR_SIZE;
        let cat = &mut iso[offset..offset + SECTOR_SIZE];

        // 1. Validation Entry (32 bytes)
        cat[0] = 0x01; // Header ID
        cat[1] = 0x00; // Platform ID: 80x86
        cat[2..4].copy_from_slice(&[0, 0]);
        cat[4..28].copy_from_slice(format!("{:<24}", "Limine Bootloader").as_bytes());
        cat[30] = 0x55;
        cat[31] = 0xaa;

        // Calculate validation checksum: sum of 16 16-bit words must be 0
        let mut sum: u16 = 0;
        for i in 0..16 {
            if i != 14 { // skip checksum word itself
                let w = u16::from_le_bytes([cat[i * 2], cat[i * 2 + 1]]);
                sum = sum.wrapping_add(w);
            }
        }
        let checksum = (0u16).wrapping_sub(sum);
        cat[28..30].copy_from_slice(&checksum.to_le_bytes());

        // 2. Initial / Default Boot Entry (32 bytes)
        let boot_entry = &mut cat[32..64];
        let limine_cd_file = &files[0];
        let boot_sectors_512 = ((limine_cd_file.size + 511) / 512) as u16;

        boot_entry[0] = 0x88; // Bootable
        boot_entry[1] = 0x00; // No emulation
        boot_entry[2..4].copy_from_slice(&0x0000u16.to_le_bytes()); // Load segment (0x07C0 default)
        boot_entry[4] = 0x00; // System type
        boot_entry[5] = 0x00; // Unused
        boot_entry[6..8].copy_from_slice(&boot_sectors_512.to_le_bytes()); // Virtual sector count (512B)
        boot_entry[8..12].copy_from_slice(&limine_cd_file.sector.to_le_bytes()); // Load RBA (2048B sector)
    }

    // ── Sector 20 & 21: Path Tables (Type L and Type M) ──────────────────────
    {
        // Directory 1: Root (parent: 1)
        // Directory 2: BOOT (parent: 1)
        // Directory 3: EFI (parent: 1)
        // Directory 4: BOOT inside EFI (parent: 3)
        let mut path_l = Vec::new();
        let mut path_m = Vec::new();

        // 1. Root
        path_l.extend_from_slice(&[1, 0]); // len=1, ext_attr=0
        path_l.extend_from_slice(&root_dir_sector.to_le_bytes());
        path_l.extend_from_slice(&1u16.to_le_bytes()); // parent=1
        path_l.push(0x00);
        path_l.push(0x00); // padding

        path_m.extend_from_slice(&[1, 0]);
        path_m.extend_from_slice(&root_dir_sector.to_be_bytes());
        path_m.extend_from_slice(&1u16.to_be_bytes());
        path_m.push(0x00);
        path_m.push(0x00);

        // 2. BOOT
        path_l.extend_from_slice(&[4, 0]); // len=4 ("BOOT")
        path_l.extend_from_slice(&boot_dir_sector.to_le_bytes());
        path_l.extend_from_slice(&1u16.to_le_bytes()); // parent=1
        path_l.extend_from_slice(b"BOOT");

        path_m.extend_from_slice(&[4, 0]);
        path_m.extend_from_slice(&boot_dir_sector.to_be_bytes());
        path_m.extend_from_slice(&1u16.to_be_bytes());
        path_m.extend_from_slice(b"BOOT");

        // 3. EFI
        path_l.extend_from_slice(&[3, 0]); // len=3 ("EFI")
        path_l.extend_from_slice(&efi_dir_sector.to_le_bytes());
        path_l.extend_from_slice(&1u16.to_le_bytes()); // parent=1
        path_l.extend_from_slice(b"EFI\0"); // padded

        path_m.extend_from_slice(&[3, 0]);
        path_m.extend_from_slice(&efi_dir_sector.to_be_bytes());
        path_m.extend_from_slice(&1u16.to_be_bytes());
        path_m.extend_from_slice(b"EFI\0");

        // 4. EFI/BOOT
        path_l.extend_from_slice(&[4, 0]); // len=4 ("BOOT")
        path_l.extend_from_slice(&efi_boot_dir_sector.to_le_bytes());
        path_l.extend_from_slice(&3u16.to_le_bytes()); // parent=3 (EFI)
        path_l.extend_from_slice(b"BOOT");

        path_m.extend_from_slice(&[4, 0]);
        path_m.extend_from_slice(&efi_boot_dir_sector.to_be_bytes());
        path_m.extend_from_slice(&3u16.to_be_bytes()); // parent=3 (EFI)
        path_m.extend_from_slice(b"BOOT");

        let off_l = path_table_l_sector as usize * SECTOR_SIZE;
        iso[off_l..off_l + path_l.len()].copy_from_slice(&path_l);

        let off_m = path_table_m_sector as usize * SECTOR_SIZE;
        iso[off_m..off_m + path_m.len()].copy_from_slice(&path_m);
    }

    // ── Sector 22: Root Directory ────────────────────────────────────────────
    {
        let offset = root_dir_sector as usize * SECTOR_SIZE;
        let mut dir = Vec::new();

        // '.' record
        let mut dot = [0u8; 34];
        write_dir_record(&mut dot, root_dir_sector, SECTOR_SIZE as u32, true, b"\x00");
        dir.extend_from_slice(&dot);

        // '..' record
        let mut dotdot = [0u8; 34];
        write_dir_record(&mut dotdot, root_dir_sector, SECTOR_SIZE as u32, true, b"\x01");
        dir.extend_from_slice(&dotdot);

        // BOOT directory record
        let mut boot_rec = [0u8; 38];
        write_dir_record(&mut boot_rec, boot_dir_sector, SECTOR_SIZE as u32, true, b"BOOT");
        dir.extend_from_slice(&boot_rec);

        // EFI directory record
        let mut efi_rec = [0u8; 38];
        write_dir_record(&mut efi_rec, efi_dir_sector, SECTOR_SIZE as u32, true, b"EFI");
        dir.extend_from_slice(&efi_rec);

        // Files in root (LIMINE_B.BIN, LIMINE_B.SYS, LIMINE.CON)
        for f in &files {
            if !f.path_in_iso.contains('/') {
                let name = f.path_in_iso.as_bytes();
                let rec_len = (33 + name.len() + 1) & !1;
                let mut rec = vec![0u8; rec_len];
                write_dir_record(&mut rec, f.sector, f.size as u32, false, name);
                dir.extend_from_slice(&rec);
            }
        }

        iso[offset..offset + dir.len()].copy_from_slice(&dir);
    }

    // ── Sector 23: BOOT Directory ────────────────────────────────────────────
    {
        let offset = boot_dir_sector as usize * SECTOR_SIZE;
        let mut dir = Vec::new();

        // '.' record
        let mut dot = [0u8; 34];
        write_dir_record(&mut dot, boot_dir_sector, SECTOR_SIZE as u32, true, b"\x00");
        dir.extend_from_slice(&dot);

        // '..' record
        let mut dotdot = [0u8; 34];
        write_dir_record(&mut dotdot, root_dir_sector, SECTOR_SIZE as u32, true, b"\x01");
        dir.extend_from_slice(&dotdot);

        // Files in BOOT (JINN_KER)
        for f in &files {
            if f.path_in_iso.starts_with("BOOT/") {
                let name = f.path_in_iso.strip_prefix("BOOT/").unwrap().as_bytes();
                let rec_len = (33 + name.len() + 1) & !1;
                let mut rec = vec![0u8; rec_len];
                write_dir_record(&mut rec, f.sector, f.size as u32, false, name);
                dir.extend_from_slice(&rec);
            }
        }

        iso[offset..offset + dir.len()].copy_from_slice(&dir);
    }

    // ── Sector 24: EFI Directory ─────────────────────────────────────────────
    {
        let offset = efi_dir_sector as usize * SECTOR_SIZE;
        let mut dir = Vec::new();

        let mut dot = [0u8; 34];
        write_dir_record(&mut dot, efi_dir_sector, SECTOR_SIZE as u32, true, b"\x00");
        dir.extend_from_slice(&dot);

        let mut dotdot = [0u8; 34];
        write_dir_record(&mut dotdot, root_dir_sector, SECTOR_SIZE as u32, true, b"\x01");
        dir.extend_from_slice(&dotdot);

        let mut boot_rec = [0u8; 38];
        write_dir_record(&mut boot_rec, efi_boot_dir_sector, SECTOR_SIZE as u32, true, b"BOOT");
        dir.extend_from_slice(&boot_rec);

        iso[offset..offset + dir.len()].copy_from_slice(&dir);
    }

    // ── Sector 25: EFI/BOOT Directory ────────────────────────────────────────
    {
        let offset = efi_boot_dir_sector as usize * SECTOR_SIZE;
        let mut dir = Vec::new();

        let mut dot = [0u8; 34];
        write_dir_record(&mut dot, efi_boot_dir_sector, SECTOR_SIZE as u32, true, b"\x00");
        dir.extend_from_slice(&dot);

        let mut dotdot = [0u8; 34];
        write_dir_record(&mut dotdot, efi_dir_sector, SECTOR_SIZE as u32, true, b"\x01");
        dir.extend_from_slice(&dotdot);

        for f in &files {
            if f.path_in_iso.starts_with("EFI/BOOT/") {
                let name = f.path_in_iso.strip_prefix("EFI/BOOT/").unwrap().as_bytes();
                let rec_len = (33 + name.len() + 1) & !1;
                let mut rec = vec![0u8; rec_len];
                write_dir_record(&mut rec, f.sector, f.size as u32, false, name);
                dir.extend_from_slice(&rec);
            }
        }

        iso[offset..offset + dir.len()].copy_from_slice(&dir);
    }

    // ── Copy File Data ───────────────────────────────────────────────────────
    for f in &files {
        let offset = f.sector as usize * SECTOR_SIZE;
        let data = fs::read(&f.disk_path)?;
        iso[offset..offset + data.len()].copy_from_slice(&data);
        println!("  Packed {:<25} -> Sector {:<4} ({} bytes)", f.path_in_iso, f.sector, f.size);
    }

    // Write final ISO
    let mut out_file = File::create(&iso_output)?;
    out_file.write_all(&iso)?;
    println!("\n✅ ISO image generated successfully at {:?}", iso_output);

    Ok(())
}

fn write_dir_record(buf: &mut [u8], extent_sector: u32, size_bytes: u32, is_dir: bool, name: &[u8]) {
    let rec_len = (33 + name.len() + 1) & !1;
    assert!(buf.len() >= rec_len);

    buf[0] = rec_len as u8;
    buf[1] = 0; // Extended attribute length
    buf[2..10].copy_from_slice(&u32_both(extent_sector));
    buf[10..18].copy_from_slice(&u32_both(size_bytes));

    // Date/time: [year-1900, month, day, hour, min, sec, gmt_offset]
    buf[18] = 126; // 2026
    buf[19] = 8;   // August
    buf[20] = 24;  // 24
    buf[21] = 22;
    buf[22] = 30;
    buf[23] = 0;
    buf[24] = 0;

    buf[25] = if is_dir { 0x02 } else { 0x00 }; // File flags
    buf[26] = 0; // File unit size
    buf[27] = 0; // Interleave gap
    buf[28..32].copy_from_slice(&u16_both(1)); // Volume sequence
    buf[32] = name.len() as u8;
    buf[33..33 + name.len()].copy_from_slice(name);
}
