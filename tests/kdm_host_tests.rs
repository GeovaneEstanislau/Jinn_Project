//! Jinn OS — Testes Unitários de Confiabilidade do KDM no Host (Fase 2)

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
#[path = "../src/dma/tests.rs"]
mod tests;

#[test]
fn test_host_kdm_fase1_and_fase2_all_selftests() {
    let (passed, total) = tests::run_dma_manager_selftests();
    assert_eq!(passed, total, "Todos os 16 testes do KDM devem passar no host");
    assert_eq!(passed, 16);
}

#[test]
fn test_host_pool_32bit_strict_ceiling() {
    pool::mock_alloc::set_force_64bit(true);
    let mut manager = pool::DmaPoolManager::new();
    let res = manager.create_pool(1, 0x1001, 4);
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
    let mut manager = pool::DmaPoolManager::new();
    let mut reg = buffer::QuarantineRegistry::new();

    let pool_id = manager.create_pool(99, 0x1001, 4).expect("create_pool");
    let (paddr, _) = manager.alloc_from_pool(pool_id, 99, 2).expect("alloc_from_pool");

    manager.destroy_pool(pool_id, 99, &mut reg).expect("destroy_pool");

    assert!(reg.is_quarantined(paddr));
    assert!(!reg.can_release_to_buddy(paddr));
}

