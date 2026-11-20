//! Jinn OS — Subsistema de Áudio e Isolamento de Falhas (AC'97 / Intel HDA)
//!
//! Implementa:
//! - Detecção e enumeração segura de controladores PCI de Classe 0x04 (Áudio / Multimídia).
//! - Suporte a AC'97 (Classe 0x04, Subclasse 0x01) e Intel HDA (Classe 0x04, Subclasse 0x03).
//! - Gerenciamento atômico de estado de reprodução, volume e modo mudo.
//! - Buffer DMA físico protegido com prevenção de estouro e colisão de memória.
//! - Tratamento de interrupções level-triggered com limpeza de status e mitigação de storm.
//! - Supervisão e degradação resiliente (`DegradationStrategy::AudioMute`) via DriverSupervisor.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use crate::drivers::pci::{self, PciDevice};
use crate::drivers::supervisor::{DegradationStrategy, DriverState};

pub const AUDIO_DEVICE_ID: u32 = 4;
pub const AUDIO_DMA_BUF_SIZE: usize = 16384; // 16 KiB ring buffer

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioControllerType {
    None,
    Ac97,
    IntelHda,
    GenericPci,
}

struct AudioHardwareState {
    ctrl_type: AudioControllerType,
    bus: u8,
    slot: u8,
    func: u8,
    nam_bar: u16,   // AC'97 Native Audio Mixer (BAR0)
    nabm_bar: u16,  // AC'97 Native Audio Bus Master (BAR1)
    mmio_base: u64, // Intel HDA MMIO base (BAR0)
    irq: u8,
}

static AUDIO_HW: crate::memory::IrqSafeSpinLock<AudioHardwareState> = crate::memory::IrqSafeSpinLock::new(AudioHardwareState {
    ctrl_type: AudioControllerType::None,
    bus: 0,
    slot: 0,
    func: 0,
    nam_bar: 0,
    nabm_bar: 0,
    mmio_base: 0,
    irq: 0,
});

static IS_PRESENT: AtomicBool = AtomicBool::new(false);
static IS_MUTED: AtomicBool = AtomicBool::new(false);
static IS_PLAYING: AtomicBool = AtomicBool::new(false);
static VOLUME_PERCENT: AtomicU32 = AtomicU32::new(80);
static CURRENT_IRQ: AtomicU32 = AtomicU32::new(0);
static BUFFER_HEAD: AtomicUsize = AtomicUsize::new(0);
static BUFFER_TAIL: AtomicUsize = AtomicUsize::new(0);
static FAULT_COUNT: AtomicU32 = AtomicU32::new(0);

// Buffer estático de DMA em BSS para streaming PCM seguro sem alocação dinâmica em IRQ
static mut DMA_PCM_BUFFER: [u8; AUDIO_DMA_BUF_SIZE] = [0u8; AUDIO_DMA_BUF_SIZE];

/// Inicializa e audita o subsistema de áudio no barramento PCI
pub fn init() {
    crate::serial_write(b"[audio] Inicializando subsistema de audio e isolamento de falhas...\n");

    // Registra o driver no supervisor de drivers com política de degradação graciosa
    let _ = crate::drivers::supervisor::DRIVER_SUPERVISOR.lock().register_driver(
        AUDIO_DEVICE_ID,
        "AC97/HDA Audio",
        DegradationStrategy::AudioMute,
    );

    let dev_count = pci::device_count();
    let dev_slice = pci::devices();
    let mut detected_dev: Option<PciDevice> = None;

    for i in 0..dev_count {
        if let Some(dev) = dev_slice.get(i).copied().flatten() {
            if dev.class_code == 0x04 {
                detected_dev = Some(dev);
                break;
            }
        }
    }

    let dev = match detected_dev {
        Some(d) => d,
        None => {
            crate::serial_write(b"[audio] Nenhum dispositivo PCI de audio detectado (Modo Silencioso).\n");
            IS_PRESENT.store(false, Ordering::SeqCst);
            IS_MUTED.store(true, Ordering::SeqCst);
            return;
        }
    };

    let ctrl_type = if dev.subclass == 0x01 {
        AudioControllerType::Ac97
    } else if dev.subclass == 0x03 {
        AudioControllerType::IntelHda
    } else {
        AudioControllerType::GenericPci
    };

    crate::serial_write(b"[audio] Controlador detectado: ");
    match ctrl_type {
        AudioControllerType::Ac97 => crate::serial_write(b"AC'97 Audio Controller\n"),
        AudioControllerType::IntelHda => crate::serial_write(b"Intel High Definition Audio (HDA)\n"),
        _ => crate::serial_write(b"Generic Multimedia Audio\n"),
    }

    // Habilita Bus Master (DMA), Espaço de Memória e Espaço de I/O
    pci::enable_bus_mastering(dev.bus, dev.slot, dev.func);

    let nam_bar = (dev.bar0 & 0xFFFE) as u16;
    let nabm_bar = (dev.bar1 & 0xFFFE) as u16;
    let mmio_base = (dev.bar0 & !0x0F) as u64;
    let irq = dev.irq_line;

    {
        let mut hw = AUDIO_HW.lock();
        hw.ctrl_type = ctrl_type;
        hw.bus = dev.bus;
        hw.slot = dev.slot;
        hw.func = dev.func;
        hw.nam_bar = nam_bar;
        hw.nabm_bar = nabm_bar;
        hw.mmio_base = mmio_base;
        hw.irq = irq;
    }

    if irq > 0 && irq < 16 {
        CURRENT_IRQ.store(irq as u32, Ordering::SeqCst);
        // Desmascara linha IRQ no PIC legado
        crate::interrupts::pic::unmask_irq(irq);
    }

    // Configuração inicial de volume e hardware
    if ctrl_type == AudioControllerType::Ac97 && nabm_bar != 0 && nam_bar != 0 {
        unsafe {
            // AC'97 Master Volume register (0x02): bits 0..5 Attenuation Left, 8..13 Attenuation Right
            // 0x0000 = 0 dB (volume maximo), 0x8000 = Mute
            outw(nam_bar + 0x02, 0x0808); // Volume inicial confortavel (-12 dB)
            // PCM Out Volume register (0x18)
            outw(nam_bar + 0x18, 0x0808);
        }
    }

    IS_PRESENT.store(true, Ordering::SeqCst);
    IS_MUTED.store(false, Ordering::SeqCst);
    IS_PLAYING.store(false, Ordering::SeqCst);
    VOLUME_PERCENT.store(80, Ordering::SeqCst);

    crate::drivers::supervisor::DRIVER_SUPERVISOR.lock().record_heartbeat(AUDIO_DEVICE_ID, crate::timer::ticks());
    crate::serial_write(b"[audio] Subsistema de audio pronto e operacional.\n");
}

/// Envia dados PCM brutos para a fila de streaming de áudio
pub fn play_pcm(data: &[u8]) -> Result<(), ()> {
    if !IS_PRESENT.load(Ordering::Acquire) || IS_MUTED.load(Ordering::Acquire) {
        return Ok(()); // Silencioso / degradação ativa
    }

    if data.is_empty() {
        return Ok(());
    }

    let len = data.len().min(AUDIO_DMA_BUF_SIZE);
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), core::ptr::addr_of_mut!(DMA_PCM_BUFFER) as *mut u8, len);
    }

    BUFFER_HEAD.store(0, Ordering::Release);
    BUFFER_TAIL.store(len, Ordering::Release);
    IS_PLAYING.store(true, Ordering::Release);

    // Inicia reprodução no controlador de hardware se aplicável
    let hw = AUDIO_HW.lock();
    if hw.ctrl_type == AudioControllerType::Ac97 && hw.nabm_bar != 0 {
        unsafe {
            // PO_CR (PCM Out Control Register, offset 0x1B): Bit 0 = Run/Pause
            let cr = inb(hw.nabm_bar + 0x1B);
            outb(hw.nabm_bar + 0x1B, cr | 0x01);
        }
    }

    crate::drivers::supervisor::DRIVER_SUPERVISOR.lock().record_heartbeat(AUDIO_DEVICE_ID, crate::timer::ticks());
    Ok(())
}

/// Interrompe a reprodução ativa
pub fn stop() {
    IS_PLAYING.store(false, Ordering::Release);
    let hw = AUDIO_HW.lock();
    if hw.ctrl_type == AudioControllerType::Ac97 && hw.nabm_bar != 0 {
        unsafe {
            let cr = inb(hw.nabm_bar + 0x1B);
            outb(hw.nabm_bar + 0x1B, cr & !0x01);
        }
    }
}

/// Define o volume geral do áudio (0..=100)
pub fn set_volume(percent: u32) {
    let p = percent.min(100);
    VOLUME_PERCENT.store(p, Ordering::Release);

    let hw = AUDIO_HW.lock();
    if hw.ctrl_type == AudioControllerType::Ac97 && hw.nam_bar != 0 {
        // Mapeia 100% -> 0x00, 0% -> 0x1F (attenuation)
        let att = ((100 - p) * 31 / 100) as u16;
        let val = (att << 8) | att;
        unsafe {
            outw(hw.nam_bar + 0x02, val);
            outw(hw.nam_bar + 0x18, val);
        }
    }
}

/// Obtém o volume atual em porcentagem (0..=100)
pub fn get_volume() -> u32 {
    VOLUME_PERCENT.load(Ordering::Acquire)
}

/// Ativa ou desativa o mudo
pub fn set_mute(mute: bool) {
    IS_MUTED.store(mute, Ordering::Release);
    let hw = AUDIO_HW.lock();
    if hw.ctrl_type == AudioControllerType::Ac97 && hw.nam_bar != 0 {
        let cur = unsafe { inw(hw.nam_bar + 0x02) };
        let new_val = if mute { cur | 0x8000 } else { cur & !0x8000 };
        unsafe {
            outw(hw.nam_bar + 0x02, new_val);
        }
    }
}

/// Verifica se o áudio está mudo
pub fn is_muted() -> bool {
    IS_MUTED.load(Ordering::Acquire)
}

/// Verifica se a linha IRQ pertence ao controlador de áudio PCI ativo
pub fn is_audio_irq(irq: u8) -> bool {
    if !IS_PRESENT.load(Ordering::Relaxed) {
        return false;
    }
    let dev_irq = CURRENT_IRQ.load(Ordering::Relaxed) as u8;
    dev_irq != 0 && dev_irq == irq
}

/// Trata interrupção de hardware disparada pelo controlador de áudio
pub fn handle_irq(irq: u8) -> bool {
    if !IS_PRESENT.load(Ordering::Relaxed) {
        return false;
    }

    let dev_irq = CURRENT_IRQ.load(Ordering::Relaxed) as u8;
    if dev_irq == 0 || dev_irq != irq {
        return false;
    }

    let hw = AUDIO_HW.lock();
    match hw.ctrl_type {
        AudioControllerType::Ac97 if hw.nabm_bar != 0 => {
            unsafe {
                // PO_SR (PCM Out Status Register, offset 0x16): Bit 0 = DMA Controller Halted, Bit 2 = Buffer Completion
                let sr = inw(hw.nabm_bar + 0x16);
                if (sr & 0x1C) != 0 {
                    // Limpa bits de status escrevendo 1s (W1C)
                    outw(hw.nabm_bar + 0x16, sr & 0x1C);
                    if (sr & 0x02) != 0 {
                        // Buffer completion: avança ponteiro
                        let cur_tail = BUFFER_TAIL.load(Ordering::Relaxed);
                        BUFFER_HEAD.store(cur_tail, Ordering::Release);
                        IS_PLAYING.store(false, Ordering::Release);
                    }
                    crate::drivers::supervisor::DRIVER_SUPERVISOR.lock().record_heartbeat(AUDIO_DEVICE_ID, crate::timer::ticks());
                    return true;
                }
            }
        }
        AudioControllerType::IntelHda if hw.mmio_base != 0 => {
            // Intel HDA INTSTS register: offset 0x20
            let intsts_ptr = (hw.mmio_base + 0x20) as *mut u32;
            unsafe {
                let status = core::ptr::read_volatile(intsts_ptr);
                if (status & 0x8000_0000) != 0 {
                    // Global interrupt pending: limpa escrevendo bits
                    core::ptr::write_volatile(intsts_ptr, status);
                    crate::drivers::supervisor::DRIVER_SUPERVISOR.lock().record_heartbeat(AUDIO_DEVICE_ID, crate::timer::ticks());
                    return true;
                }
            }
        }
        _ => {}
    }

    false
}

/// Trata interrupção espúria ou tempestade na linha IRQ compartilhada
pub fn handle_storm_fault(irq: u8) {
    let dev_irq = CURRENT_IRQ.load(Ordering::Relaxed) as u8;
    if dev_irq != 0 && dev_irq == irq {
        let count = FAULT_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
        if count >= 3 {
            crate::serial_write(b"[audio] Tempestade de interrupcoes detectada! Isolando controlador de audio...\n");
            handle_fault("Audio PCI interrupt storm");
        }
    }
}

/// Executa degradação controlada (AudioMute) sem afetar o restante do sistema
pub fn handle_fault(reason: &str) {
    crate::serial_write(b"[audio] Falha de audio isolada: ");
    crate::serial_write(reason.as_bytes());
    crate::serial_write(b"\n");

    IS_MUTED.store(true, Ordering::SeqCst);
    IS_PLAYING.store(false, Ordering::SeqCst);

    let irq = CURRENT_IRQ.load(Ordering::SeqCst) as u8;
    if irq > 0 && irq < 16 {
        crate::interrupts::pic::mask_irq(irq);
    }

    let state = crate::drivers::supervisor::DRIVER_SUPERVISOR.lock().handle_driver_fault(AUDIO_DEVICE_ID, reason);
    if state == DriverState::Disabled {
        crate::serial_write(b"[audio] Circuit breaker disparado: driver de audio desativado permanentemente.\n");
    }
}

// ── I/O Port Helpers ──────────────────────────────────────────────────────────

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack));
    val
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    core::arch::asm!("in ax, dx", out("ax") val, in("dx") port, options(nomem, nostack));
    val
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack));
}

#[inline]
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack));
}
