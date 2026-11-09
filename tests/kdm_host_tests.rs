//! Jinn OS — Testes Unitários de Confiabilidade do KDM no Host (Fases 1, 2 e 3)

#[path = "../src/dma/types.rs"]
mod types;
#[path = "../src/dma/token.rs"]
mod token;
#[path = "../src/dma/state.rs"]
mod state;
#[path = "../src/dma/buffer.rs"]
mod buffer;
#[path = "../src/dma/pool.rs"]
mod pool;
#[path = "../src/dma/bounce.rs"]
mod bounce;
#[path = "../src/dma/manager.rs"]
mod manager;
#[path = "../src/dma/tests.rs"]
mod tests;

#[test]
fn test_host_kdm_fases_1_2_3_all_selftests() {
    let (passed, total) = tests::run_dma_manager_selftests();
    assert_eq!(passed, total, "Todos os 23 testes do KDM devem passar no host");
    assert_eq!(passed, 23);
}

#[test]
fn test_host_pool_32bit_strict_ceiling() {
    pool::mock_alloc::set_force_64bit(true);
    let mut dma_mgr = pool::DmaPoolManager::new();
    let res = dma_mgr.create_pool(1, 0x1001, 4);
    assert_eq!(res, Err(types::DmaError::UnsupportedPlatform), "Frames acima de 4 GiB devem ser estritamente rejeitados");
    pool::mock_alloc::set_force_64bit(false);
}

#[test]
fn test_host_bounce_buffer_roundtrip_integrity() {
    let paddr = pool::alloc_physical_frames(0).expect("Alloc mock frame");
    let mut buf = buffer::DmaBuffer::empty();
    buf.capacity = types::PAGE_SIZE;
    buf.direction = types::DmaDirection::Bidirectional;
    buf.state = state::DmaBufferState::Mapped;
    buf.bounce_buffer_phys = paddr;

    let tx_data = b"JINN_DMA_SAFE_TRANSMISSION_2026";
    let hw_addr = bounce::BounceBufferMediator::prepare_tx_slice(&mut buf, tx_data, 0).expect("prepare_tx");
    assert_eq!(hw_addr, paddr);
    assert_eq!(buf.state, state::DmaBufferState::InTransfer);

    let mut rx_data = [0u8; 31];
    let rx_len = bounce::BounceBufferMediator::complete_rx_slice(&mut buf, &mut rx_data, 0).expect("complete_rx");
    assert_eq!(rx_len, 31);
    assert_eq!(&rx_data, tx_data);
    assert_eq!(buf.state, state::DmaBufferState::Mapped);
}

#[test]
fn test_host_pool_quota_and_quarantine_on_leak() {
    let mut dma_mgr = pool::DmaPoolManager::new();
    let mut reg = buffer::QuarantineRegistry::new();

    let pool_id = dma_mgr.create_pool(99, 0x1001, 4).expect("create_pool");
    let (paddr, _) = dma_mgr.alloc_from_pool(pool_id, 99, 2).expect("alloc_from_pool");

    dma_mgr.destroy_pool(pool_id, 99, &mut reg).expect("destroy_pool");

    assert!(reg.is_quarantined(paddr));
    assert!(!reg.can_release_to_buddy(paddr));
}

#[test]
fn test_host_buffer_registry_lifecycle_and_security() {
    let mut pool_mgr = pool::DmaPoolManager::new();
    let mut reg = manager::DmaBufferRegistry::new();
    let mut qreg = buffer::QuarantineRegistry::new();

    let pid = 200;
    let pool_id = pool_mgr.create_pool(pid, 0x5001, 4).expect("create_pool");

    let handle = reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 2, types::DmaDirection::Bidirectional)
        .expect("alloc_buffer");

    assert_eq!(reg.active_count(), 1);

    // Verificação de posse e integridade
    let entry = reg.get_entry(handle, pid).expect("get_entry");
    assert_eq!(entry.page_count, 2);
    assert_eq!(entry.buffer.state, state::DmaBufferState::Allocated);

    // Tentativa de acesso não autorizado por outro PID
    let err = reg.get_entry(handle, 201);
    assert_eq!(err.unwrap_err(), types::DmaError::PermissionDenied);

    // Liberação e ciclo de reciclagem
    reg.release_buffer(&mut pool_mgr, handle, pid).expect("release_buffer");
    assert_eq!(reg.active_count(), 0);

    // Tentativa de reutilizar handle antigo
    let stale_err = reg.get_entry(handle, pid);
    assert_eq!(stale_err.unwrap_err(), types::DmaError::InvalidHandle);

    pool_mgr.destroy_pool(pool_id, pid, &mut qreg).expect("destroy_pool");
    assert_eq!(qreg.count(), 0);
}

#[test]
fn test_host_buffer_registry_unconfirmed_reset_quarantine() {
    let mut pool_mgr = pool::DmaPoolManager::new();
    let mut reg = manager::DmaBufferRegistry::new();
    let mut qreg = buffer::QuarantineRegistry::new();

    let pid = 300;
    let pool_id = pool_mgr.create_pool(pid, 0x5002, 4).expect("create_pool");
    let handle = reg.alloc_buffer(&mut pool_mgr, pid, pool_id, 2, types::DmaDirection::DeviceReadsMemory)
        .expect("alloc_buffer");

    let entry = reg.get_entry_mut(handle, pid).expect("get_entry_mut");
    let paddr = entry.buffer.phys_addr;

    entry.buffer.revoke_and_quarantine(&mut qreg, "Host test unconfirmed reset")
        .expect("revoke_and_quarantine");

    assert_eq!(entry.buffer.state, state::DmaBufferState::Quarantined);
    assert!(qreg.is_quarantined(paddr));
    assert!(!qreg.can_release_to_buddy(paddr));
}

