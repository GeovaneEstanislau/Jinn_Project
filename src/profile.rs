//! System Profiles for Jinn Microkernel: Aladdin (Desktop) vs Simbad (Server) vs Ifrit (Industrial)
//! Controls runtime behavior, scheduler timeslice, headless mode, storage write policies, and security profiles.

use crate::drivers::block::{StorageProfile, set_storage_profile};
use crate::net::jinnwall::{FirewallProfile, JINNWALL};

/// Perfis canônicos de manifestação do ecossistema Jinn
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemProfile {
    AladdinDesktop,
    SimbadServer,
    IfritIndustrial,
}

impl SystemProfile {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::AladdinDesktop => "Aladdin (Desktop OS)",
            Self::SimbadServer => "Simbad (Server OS)",
            Self::IfritIndustrial => "Ifrit (Industrial Real-Time OS)",
        }
    }

    pub const fn is_headless(&self) -> bool {
        match self {
            Self::AladdinDesktop => false,
            Self::SimbadServer | Self::IfritIndustrial => true,
        }
    }

    pub const fn scheduler_quantum_ticks(&self) -> usize {
        match self {
            Self::AladdinDesktop => 1,  // ~10ms (responsividade interativa)
            Self::SimbadServer => 5,    // ~50ms (vazão e cache throughput)
            Self::IfritIndustrial => 1, // ~10ms (tempo real)
        }
    }

    pub const fn default_storage_profile(&self) -> StorageProfile {
        match self {
            Self::AladdinDesktop => StorageProfile::Aladdin,
            Self::SimbadServer => StorageProfile::Simbad,
            Self::IfritIndustrial => StorageProfile::TerminalPos,
        }
    }

    pub const fn periodic_flush_interval_ms(&self) -> u64 {
        match self {
            Self::AladdinDesktop => 10_000,
            Self::SimbadServer => 5_000,
            Self::IfritIndustrial => 1_000,
        }
    }
}

static mut ACTIVE_SYSTEM_PROFILE: SystemProfile = SystemProfile::AladdinDesktop;

pub fn get_system_profile() -> SystemProfile {
    unsafe { *core::ptr::addr_of!(ACTIVE_SYSTEM_PROFILE) }
}

pub fn set_system_profile(profile: SystemProfile) {
    unsafe {
        *core::ptr::addr_of_mut!(ACTIVE_SYSTEM_PROFILE) = profile;
    }
    // Alinha a política de armazenamento do kernel com o perfil
    set_storage_profile(profile.default_storage_profile());

    // No modo servidor, assegura perfil de firewall PublicSecure por padrão
    if profile == SystemProfile::SimbadServer {
        unsafe {
            JINNWALL.set_profile(FirewallProfile::PublicSecure);
        }
    }
}
