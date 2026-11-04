//! Periodic Storage Flush Daemon (jinn-flusher)
//! Periodically executes cache write-back flushing to disk to enforce crash consistency
//! and bounded recovery windows without blocking interactive foreground threads.

pub struct StorageFlusher {
    pub last_flush_tick: u64,
    pub total_flushes: u64,
    pub total_sectors_flushed: u64,
    pub is_enabled: bool,
}

impl StorageFlusher {
    pub const fn new() -> Self {
        Self {
            last_flush_tick: 0,
            total_flushes: 0,
            total_sectors_flushed: 0,
            is_enabled: true,
        }
    }

    /// Executado periodicamente pelo temporizador ou laço do servidor
    pub fn poll(&mut self, current_tick: u64, interval_ticks: u64) -> bool {
        if !self.is_enabled {
            return false;
        }

        if current_tick >= self.last_flush_tick.saturating_add(interval_ticks) {
            self.force_flush(current_tick);
            true
        } else {
            false
        }
    }

    /// Força a sincronização imediata de todos os setores sujos no cache primário
    pub fn force_flush(&mut self, current_tick: u64) {
        let flushed_sectors = match crate::drivers::block::flush_primary_cache() {
            Ok(n) => n,
            Err(_) => 0,
        };
        self.last_flush_tick = current_tick;
        self.total_flushes = self.total_flushes.saturating_add(1);
        self.total_sectors_flushed = self.total_sectors_flushed.saturating_add(flushed_sectors as u64);

        if flushed_sectors > 0 {
            let supervisor = crate::supervisor::get_supervisor();
            supervisor.log("INFO", "FLUSHER", b"Dirty cache flushed to disk");
        }
    }
}

pub static mut FLUSHER: StorageFlusher = StorageFlusher::new();

pub fn get_flusher() -> &'static mut StorageFlusher {
    unsafe { &mut *core::ptr::addr_of_mut!(FLUSHER) }
}
