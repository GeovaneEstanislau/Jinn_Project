#![allow(dead_code)]

/// Limine boot protocol support for Jinn OS.
///
/// Provides requests for:
/// - Base revision (0)
/// - Bootloader Information (name and version)
/// - HHDM (Higher Half Direct Map) offset
/// - Memory Map (physical memory regions, total and usable RAM)
/// - Kernel Address (physical and virtual base)
/// - ACPI RSDP pointer
/// - SMP (CPU core count and bootstrap processor ID)

#[used]
#[link_section = ".requests"]
static LIMINE_BASE_REVISION: [u64; 3] = [
    0xf9562b2d5c95a6c8, // magic[0]
    0x6a7b384944536bdc, // magic[1]
    0,                  // revision = 0 (compatible with all Limine >= v2)
];

// Common magic numbers for all Limine requests
const COMMON_MAGIC_0: u64 = 0xc7b1dd30df4c8b88;
const COMMON_MAGIC_1: u64 = 0x0a82e883a194f07b;

// ── Bootloader Info Request ──────────────────────────────────────────────────
#[repr(C)]
pub struct LimineBootloaderInfoResponse {
    pub revision: u64,
    pub name: *const u8,
    pub version: *const u8,
}

#[repr(C)]
pub struct LimineBootloaderInfoRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *const LimineBootloaderInfoResponse,
}

unsafe impl Sync for LimineBootloaderInfoRequest {}

#[used]
#[link_section = ".requests"]
pub static LIMINE_BOOTLOADER_INFO_REQUEST: LimineBootloaderInfoRequest = LimineBootloaderInfoRequest {
    id: [COMMON_MAGIC_0, COMMON_MAGIC_1, 0xf55038d8e2a1202f, 0x279426fcf5f59740],
    revision: 0,
    response: core::ptr::null(),
};

// ── HHDM (Higher-Half Direct Map) Request ────────────────────────────────────
#[repr(C)]
pub struct LimineHhdmResponse {
    pub revision: u64,
    pub offset: u64,
}

#[repr(C)]
pub struct LimineHhdmRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *const LimineHhdmResponse,
}

unsafe impl Sync for LimineHhdmRequest {}

#[used]
#[link_section = ".requests"]
pub static LIMINE_HHDM_REQUEST: LimineHhdmRequest = LimineHhdmRequest {
    id: [COMMON_MAGIC_0, COMMON_MAGIC_1, 0x48dcf1cb8ad2b852, 0x63984e959a98244b],
    revision: 0,
    response: core::ptr::null(),
};

// ── Memory Map Request ───────────────────────────────────────────────────────
pub const LIMINE_MEMMAP_USABLE: u64 = 0;
pub const LIMINE_MEMMAP_RESERVED: u64 = 1;
pub const LIMINE_MEMMAP_ACPI_RECLAIMABLE: u64 = 2;
pub const LIMINE_MEMMAP_ACPI_NVS: u64 = 3;
pub const LIMINE_MEMMAP_BAD_MEMORY: u64 = 4;
pub const LIMINE_MEMMAP_BOOTLOADER_RECLAIMABLE: u64 = 5;
pub const LIMINE_MEMMAP_KERNEL_AND_MODULES: u64 = 6;
pub const LIMINE_MEMMAP_FRAMEBUFFER: u64 = 7;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct LimineMemmapEntry {
    pub base: u64,
    pub length: u64,
    pub typ: u64,
}

#[repr(C)]
pub struct LimineMemmapResponse {
    pub revision: u64,
    pub entry_count: u64,
    pub entries: *const *const LimineMemmapEntry,
}

#[repr(C)]
pub struct LimineMemmapRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *const LimineMemmapResponse,
}

unsafe impl Sync for LimineMemmapRequest {}

#[used]
#[link_section = ".requests"]
pub static LIMINE_MEMMAP_REQUEST: LimineMemmapRequest = LimineMemmapRequest {
    id: [COMMON_MAGIC_0, COMMON_MAGIC_1, 0x67cf3d9d378a806f, 0xe304acdfc50c3c62],
    revision: 0,
    response: core::ptr::null(),
};

// ── Kernel Address Request ───────────────────────────────────────────────────
#[repr(C)]
pub struct LimineKernelAddressResponse {
    pub revision: u64,
    pub physical_base: u64,
    pub virtual_base: u64,
}

#[repr(C)]
pub struct LimineKernelAddressRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *const LimineKernelAddressResponse,
}

unsafe impl Sync for LimineKernelAddressRequest {}

#[used]
#[link_section = ".requests"]
pub static LIMINE_KERNEL_ADDRESS_REQUEST: LimineKernelAddressRequest = LimineKernelAddressRequest {
    // Executable Address Request ID
    id: [COMMON_MAGIC_0, COMMON_MAGIC_1, 0x71ba76863cc55f63, 0xb2644a48c516a487],
    revision: 0,
    response: core::ptr::null(),
};

// ── ACPI RSDP Request ────────────────────────────────────────────────────────
#[repr(C)]
pub struct LimineRsdpResponse {
    pub revision: u64,
    pub address: u64,
}

#[repr(C)]
pub struct LimineRsdpRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *const LimineRsdpResponse,
}

unsafe impl Sync for LimineRsdpRequest {}

#[used]
#[link_section = ".requests"]
pub static LIMINE_RSDP_REQUEST: LimineRsdpRequest = LimineRsdpRequest {
    id: [COMMON_MAGIC_0, COMMON_MAGIC_1, 0xc5e77b6b397e7b43, 0x27637845accdcf3c],
    revision: 0,
    response: core::ptr::null(),
};

// ── Framebuffer Request ──────────────────────────────────────────────────────
#[derive(Clone, Copy)]
#[repr(C)]
pub struct LimineFramebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    pub unused: [u8; 7],
    pub edid_size: u64,
    pub edid: *const u8,
}

#[repr(C)]
pub struct LimineFramebufferResponse {
    pub revision: u64,
    pub framebuffer_count: u64,
    pub framebuffers: *const *const LimineFramebuffer,
}

#[repr(C)]
pub struct LimineFramebufferRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *const LimineFramebufferResponse,
}

unsafe impl Sync for LimineFramebufferRequest {}

#[used]
#[link_section = ".requests"]
pub static LIMINE_FRAMEBUFFER_REQUEST: LimineFramebufferRequest = LimineFramebufferRequest {
    id: [COMMON_MAGIC_0, COMMON_MAGIC_1, 0x9d5827dcd881dd75, 0xa3148604f6fab11b],
    revision: 0,
    response: core::ptr::null(),
};

// ── Safe Query Functions ────────────────────────────────────────────────────
// All response pointer reads use read_volatile so the compiler cannot
// constant-fold them to null() even with opt-level=3 + lto=true.

pub fn hhdm_offset() -> u64 {
    unsafe {
        let resp = core::ptr::read_volatile(&LIMINE_HHDM_REQUEST.response);
        if !resp.is_null() {
            (*resp).offset
        } else {
            0xffff800000000000 // standard Limine HHDM base (x86-64)
        }
    }
}

pub fn framebuffer() -> Option<&'static mut LimineFramebuffer> {
    unsafe {
        let resp = core::ptr::read_volatile(&LIMINE_FRAMEBUFFER_REQUEST.response);
        if resp.is_null() {
            return None;
        }
        let count = core::ptr::read_volatile(&(*resp).framebuffer_count);
        if count == 0 {
            return None;
        }
        let fbs_ptr = core::ptr::read_volatile(&(*resp).framebuffers);
        if fbs_ptr.is_null() {
            return None;
        }
        let fb_ptr = core::ptr::read_volatile(fbs_ptr) as *mut LimineFramebuffer;
        if fb_ptr.is_null() {
            return None;
        }
        Some(&mut *fb_ptr)
    }
}

pub fn bootloader_info() -> Option<(&'static str, &'static str)> {
    unsafe {
        let resp = core::ptr::read_volatile(&LIMINE_BOOTLOADER_INFO_REQUEST.response);
        if resp.is_null() {
            return None;
        }
        let name_ptr = (*resp).name;
        let ver_ptr = (*resp).version;
        if name_ptr.is_null() || ver_ptr.is_null() {
            return None;
        }
        Some((cstr_to_str(name_ptr), cstr_to_str(ver_ptr)))
    }
}

pub fn kernel_address() -> Option<(u64, u64)> {
    unsafe {
        let resp = core::ptr::read_volatile(&LIMINE_KERNEL_ADDRESS_REQUEST.response);
        if resp.is_null() {
            None
        } else {
            Some(((*resp).physical_base, (*resp).virtual_base))
        }
    }
}

pub fn rsdp_address() -> Option<u64> {
    unsafe {
        let resp = core::ptr::read_volatile(&LIMINE_RSDP_REQUEST.response);
        if resp.is_null() || (*resp).address == 0 {
            None
        } else {
            Some((*resp).address)
        }
    }
}

/// Returns (total_physical_ram, usable_physical_ram) in bytes.
pub fn memory_totals() -> (u64, u64) {
    unsafe {
        let resp = core::ptr::read_volatile(&LIMINE_MEMMAP_REQUEST.response);
        if resp.is_null() {
            return (0, 0);
        }
        let entries_ptr = core::ptr::read_volatile(&(*resp).entries);
        if entries_ptr.is_null() {
            return (0, 0);
        }
        let count = core::ptr::read_volatile(&(*resp).entry_count) as usize;
        if count == 0 || count > 256 {
            return (0, 0);
        }
        let mut total = 0u64;
        let mut usable = 0u64;
        for i in 0..count {
            let entry_ptr = *entries_ptr.add(i);
            if !entry_ptr.is_null() {
                let entry = *entry_ptr;
                total += entry.length;
                if entry.typ == LIMINE_MEMMAP_USABLE {
                    usable += entry.length;
                }
            }
        }
        (total, usable)
    }
}

pub fn memmap_entries_count() -> usize {
    unsafe {
        let resp = core::ptr::read_volatile(&LIMINE_MEMMAP_REQUEST.response);
        if resp.is_null() {
            0
        } else {
            core::ptr::read_volatile(&(*resp).entry_count) as usize
        }
    }
}

pub fn get_memmap_entry(index: usize) -> Option<LimineMemmapEntry> {
    unsafe {
        let resp = core::ptr::read_volatile(&LIMINE_MEMMAP_REQUEST.response);
        if resp.is_null() {
            return None;
        }
        let entries_ptr = core::ptr::read_volatile(&(*resp).entries);
        if entries_ptr.is_null() {
            return None;
        }
        let count = core::ptr::read_volatile(&(*resp).entry_count) as usize;
        if index >= count {
            return None;
        }
        let entry_ptr = *entries_ptr.add(index);
        if entry_ptr.is_null() {
            None
        } else {
            Some(*entry_ptr)
        }
    }
}

unsafe fn cstr_to_str(ptr: *const u8) -> &'static str {
    let mut len = 0;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = core::slice::from_raw_parts(ptr, len);
    core::str::from_utf8_unchecked(slice)
}

