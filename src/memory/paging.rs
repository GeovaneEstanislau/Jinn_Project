/// Jinn OS — x86_64 4-Level Paging
///
/// Implements the Page Map Level 4 (PML4) hierarchy:
///   PML4 → PDPT → PD → PT → Physical Frame
///
/// Entry flags follow the x86_64 specification.
/// All tables are allocated from the physical frame allocator in `memory.rs`.

use crate::memory;

// ── Entry Flags ───────────────────────────────────────────────────────────────
pub const PRESENT:    u64 = 1 << 0;
pub const WRITABLE:   u64 = 1 << 1;
pub const USER:       u64 = 1 << 2;
pub const HUGE_PAGE:  u64 = 1 << 7;
pub const NO_EXEC:    u64 = 1 << 63;

pub const KERNEL_RW:  u64 = PRESENT | WRITABLE;
pub const KERNEL_RO:  u64 = PRESENT | NO_EXEC;
pub const USER_RW:    u64 = PRESENT | WRITABLE | USER;

const PAGE_MASK: u64 = 0x000F_FFFF_FFFF_F000;
const PAGE_SIZE: usize = 4096;

// ── Page Table Entry ──────────────────────────────────────────────────────────
#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct PageEntry(pub u64);

impl PageEntry {
    #[inline] pub fn new(phys: u64, flags: u64) -> Self { Self((phys & PAGE_MASK) | flags) }
    #[inline] pub fn is_present(self) -> bool           { self.0 & PRESENT != 0 }
    #[inline] pub fn phys_addr(self) -> u64             { self.0 & PAGE_MASK }
    #[inline] pub fn flags(self) -> u64                 { self.0 & !PAGE_MASK }
    #[inline] pub fn clear(&mut self)                   { self.0 = 0; }
}

// ── Page Table (512 entries, one per level) ───────────────────────────────────
#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [PageEntry; 512],
}

impl PageTable {
    pub const fn new() -> Self {
        Self { entries: [PageEntry(0); 512] }
    }

    pub fn zero(&mut self) {
        for e in self.entries.iter_mut() { e.0 = 0; }
    }
}

// ── HHDM helpers ─────────────────────────────────────────────────────────────
/// Convert a physical address to a kernel-accessible virtual address via HHDM.
#[inline]
pub fn phys_to_virt(phys: u64) -> u64 {
    phys + crate::limine::hhdm_offset()
}

/// Convert a kernel virtual address (HHDM) back to physical.
#[inline]
pub fn virt_to_phys(virt: u64) -> u64 {
    virt - crate::limine::hhdm_offset()
}

// ── Allocate a zeroed page table from the physical allocator ──────────────────
fn alloc_table() -> *mut PageTable {
    let frame = memory::alloc_frame().expect("paging: out of physical frames");
    let virt = phys_to_virt(frame as u64) as *mut PageTable;
    unsafe { (*virt).zero(); }
    virt
}

// ── Index extraction from virtual address ─────────────────────────────────────
#[inline] fn pml4_idx(va: u64) -> usize { ((va >> 39) & 0x1FF) as usize }
#[inline] fn pdpt_idx(va: u64) -> usize { ((va >> 30) & 0x1FF) as usize }
#[inline] fn pd_idx  (va: u64) -> usize { ((va >> 21) & 0x1FF) as usize }
#[inline] fn pt_idx  (va: u64) -> usize { ((va >> 12) & 0x1FF) as usize }

// ── Walk / Create a single entry, allocating intermediate tables ──────────────
unsafe fn walk_or_create(table: *mut PageTable, idx: usize, flags: u64) -> *mut PageTable {
    let entry = &mut (*table).entries[idx];
    if !entry.is_present() {
        let new_table = alloc_table();
        let phys = virt_to_phys(new_table as u64);
        *entry = PageEntry::new(phys, flags | PRESENT | WRITABLE);
    }
    phys_to_virt(entry.phys_addr()) as *mut PageTable
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Map a single 4 KiB virtual page to a physical frame.
///
/// # Safety
/// * `pml4` must point to a valid, 4 KiB aligned PML4 table.
/// * Caller must flush the TLB (`invlpg virt`) after mapping.
pub unsafe fn map_page(pml4: *mut PageTable, virt: u64, phys: u64, flags: u64) {
    let table_flags = KERNEL_RW | (flags & USER);
    let pdpt = walk_or_create(pml4, pml4_idx(virt), table_flags);
    let pd   = walk_or_create(pdpt, pdpt_idx(virt), table_flags);
    let pt   = walk_or_create(pd,   pd_idx(virt),   table_flags);

    let entry = &mut (*pt).entries[pt_idx(virt)];
    *entry = PageEntry::new(phys, flags | PRESENT);

    // TLB flush for this page
    core::arch::asm!("invlpg [{0}]", in(reg) virt, options(nostack));
}

/// Grants Ring 3 access to the mapped page containing `virt`.
///
/// Kernel mappings cloned from Limine may use 2 MiB pages, so a huge-page
/// entry is promoted as a whole when encountered at the PD level.
pub unsafe fn make_page_user(pml4: *mut PageTable, virt: u64) {
    let pml4_entry = &mut (*pml4).entries[pml4_idx(virt)];
    if !pml4_entry.is_present() { return; }
    pml4_entry.0 |= USER;
    let pdpt = phys_to_virt(pml4_entry.phys_addr()) as *mut PageTable;

    let pdpt_entry = &mut (*pdpt).entries[pdpt_idx(virt)];
    if !pdpt_entry.is_present() { return; }
    pdpt_entry.0 |= USER;
    let pd = phys_to_virt(pdpt_entry.phys_addr()) as *mut PageTable;

    let pd_entry = &mut (*pd).entries[pd_idx(virt)];
    if !pd_entry.is_present() { return; }
    pd_entry.0 |= USER;
    if pd_entry.0 & HUGE_PAGE != 0 { return; }

    let pt = phys_to_virt(pd_entry.phys_addr()) as *mut PageTable;
    (*pt).entries[pt_idx(virt)].0 |= USER;
    core::arch::asm!("invlpg [{0}]", in(reg) virt, options(nostack));
}

/// Unmap a virtual page. Does NOT free the physical frame.
///
/// # Safety
/// * `pml4` must be the active PML4.
pub unsafe fn unmap_page(pml4: *mut PageTable, virt: u64) {
    let pdpt_e = &(*pml4).entries[pml4_idx(virt)];
    if !pdpt_e.is_present() { return; }
    let pdpt = phys_to_virt(pdpt_e.phys_addr()) as *mut PageTable;

    let pd_e = &(*pdpt).entries[pdpt_idx(virt)];
    if !pd_e.is_present() { return; }
    let pd = phys_to_virt(pd_e.phys_addr()) as *mut PageTable;

    let pt_e = &(*pd).entries[pd_idx(virt)];
    if !pt_e.is_present() { return; }
    let pt = phys_to_virt(pt_e.phys_addr()) as *mut PageTable;

    (*pt).entries[pt_idx(virt)].clear();
    core::arch::asm!("invlpg [{0}]", in(reg) virt, options(nostack));
}

/// Read the current CR3 (physical address of active PML4).
pub fn current_cr3() -> u64 {
    let cr3: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack)); }
    cr3
}

/// Load a new PML4 into CR3.
///
/// # Safety
/// * The new PML4 must correctly identity-map or HHDM-map all kernel code/data.
pub unsafe fn load_cr3(pml4_phys: u64) {
    core::arch::asm!("mov cr3, {}", in(reg) pml4_phys, options(nomem, nostack));
}

/// Creates a new PML4 table for a user process.
/// It clones the higher half of the current (kernel) PML4, which contains
/// the mappings for the kernel code, data, heap, and HHDM.
/// Returns the physical address of the new PML4.
pub fn clone_kernel_pml4() -> u64 {
    unsafe {
        let current_pml4 = phys_to_virt(current_cr3()) as *const PageTable;
        let new_table_virt = alloc_table();
        
        // Em x86_64, os endereços do kernel ficam na metade superior (higher half).
        // Copiamos as entradas 256..512 (índices 256 a 511) do PML4 atual para o novo.
        for i in 256..512 {
            (*new_table_virt).entries[i] = (*current_pml4).entries[i];
        }
        
        virt_to_phys(new_table_virt as u64)
    }
}

/// Destroys a user PML4 and all user-space pages mapped in its lower half.
/// Only iterates through entries 0..256 (the user half) to avoid freeing
/// kernel pages, which are shared across all address spaces.
pub unsafe fn free_user_pml4(pml4_phys: u64) {
    let pml4 = phys_to_virt(pml4_phys) as *mut PageTable;
    
    // Free lower half (user space, indices 0..256)
    for i in 0..256 {
        let pml4_e = &(*pml4).entries[i];
        if pml4_e.is_present() {
            let pdpt = phys_to_virt(pml4_e.phys_addr()) as *mut PageTable;
            for j in 0..512 {
                let pdpt_e = &(*pdpt).entries[j];
                if pdpt_e.is_present() {
                    let pd = phys_to_virt(pdpt_e.phys_addr()) as *mut PageTable;
                    for k in 0..512 {
                        let pd_e = &(*pd).entries[k];
                        if pd_e.is_present() {
                            if pd_e.flags() & HUGE_PAGE == 0 {
                                let pt = phys_to_virt(pd_e.phys_addr()) as *mut PageTable;
                                for l in 0..512 {
                                    let pt_e = &(*pt).entries[l];
                                    if pt_e.is_present() {
                                        crate::memory::free_frame(pt_e.phys_addr());
                                    }
                                }
                                crate::memory::free_frame(pd_e.phys_addr());
                            } else {
                                // Se por acaso mapeamos HUGE_PAGE no user space, liberamos o frame gigante.
                                crate::memory::free_frame(pd_e.phys_addr());
                            }
                        }
                    }
                    crate::memory::free_frame(pdpt_e.phys_addr());
                }
            }
            crate::memory::free_frame(pml4_e.phys_addr());
        }
    }
    
    // Libera a própria página do PML4
    crate::memory::free_frame(pml4_phys);
}
