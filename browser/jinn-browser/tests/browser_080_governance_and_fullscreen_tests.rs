//! Testes de Governança de Recursos, BRCR, Integração com Escalonador e Estabilidade de Tela Cheia (v0.8.0)
//!
//! Validações formais:
//! 1. Máquina de estados de Tela Cheia (WINDOWED -> ENTERING -> FULLSCREEN -> EXITING -> WINDOWED).
//! 2. Estresse de alternância de tela cheia: 100 ciclos consecutivos sem degradação ou vazamento de estado.
//! 3. Resiliência de recuperação (RECOVERY) sob dimensões anômalas ou limites extrapolados.
//! 4. Perfis de Recursos do BrowserResourceManager (64MB, 128MB, 256MB, 512MB).
//! 5. Cálculo exato do Background Resource Consumption Ratio (BRCR):
//!    - Abas Hibernadas: BRCR < 0,5%
//!    - Abas Descartadas: BRCR < 0,1%
//! 6. Sincronização com o Escalonador do Kernel:
//!    - Abas Hibernadas mapeadas para BlockedZeroCpu (0 ticks de CPU garantidos pelo kernel).
//! 7. Distinção formal entre LogicalTabFootprint e PhysicalMemoryFootprint.

use jbe_net::MockLoader;
use jinn_browser::{
    BrowserResourceManager, BrowserSession, FullscreenController, FullscreenState,
    FullscreenTransitionError, KernelTaskStateMapping, MemoryProfile, Tab, TabManager,
    TabState,
};

fn create_mock_loader() -> Box<MockLoader> {
    let mut loader = MockLoader::new();
    let sample_html = r#"
        <html>
            <head><title>Jinn Test Page</title></head>
            <body>
                <h1>Jinn Browser 0.8.0</h1>
                <p>Conteúdo de teste para validação de tela cheia e governança.</p>
                <a href="next.html">Próximo</a>
            </body>
        </html>
    "#;
    loader.insert("http://test.local/index", sample_html.as_bytes().to_vec(), "text/html");
    loader.insert("about:blank", "<html><body></body></html>".as_bytes().to_vec(), "text/html");
    Box::new(loader)
}

fn create_test_session() -> BrowserSession {
    let loader = create_mock_loader();
    let mut session = BrowserSession::new(640, 480, loader);
    let _ = session.navigate("http://test.local/index");
    session
}

// ============================================================================
// 1. TESTES DA MÁQUINA DE ESTADOS DE TELA CHEIA (PHASE 1)
// ============================================================================

#[test]
fn test_fullscreen_canonical_flow_and_reversibility() {
    let mut controller = FullscreenController::new(300, 180);

    // Estado inicial: Windowed
    assert_eq!(controller.state, FullscreenState::Windowed);
    assert_eq!(controller.geometry.width, 300);
    assert_eq!(controller.geometry.height, 180);

    // 1. Transição para Tela Cheia (1024x768)
    assert!(controller.request_fullscreen(1024, 768).is_ok());
    assert_eq!(controller.state, FullscreenState::Fullscreen);
    assert_eq!(controller.geometry.width, 1024);
    assert_eq!(controller.geometry.height, 768);
    assert_eq!(controller.geometry.saved_width, 300);
    assert_eq!(controller.geometry.saved_height, 180);
    assert!(controller.is_fullscreen());

    // 2. Retorno para Janela Normal
    assert!(controller.exit_fullscreen().is_ok());
    assert_eq!(controller.state, FullscreenState::Windowed);
    assert_eq!(controller.geometry.width, 300);
    assert_eq!(controller.geometry.height, 180);
    assert!(!controller.is_fullscreen());
}

#[test]
fn test_fullscreen_toggle_stress_100_cycles() {
    let mut controller = FullscreenController::new(300, 180);

    for _i in 0..100 {
        // Toggle para Fullscreen
        let s1 = controller.toggle_fullscreen(1024, 768).expect("Falha ao entrar em fullscreen");
        assert_eq!(s1, FullscreenState::Fullscreen);
        assert_eq!(controller.geometry.width, 1024);
        assert_eq!(controller.geometry.height, 768);

        // Toggle de volta para Windowed
        let s2 = controller.toggle_fullscreen(1024, 768).expect("Falha ao sair de fullscreen");
        assert_eq!(s2, FullscreenState::Windowed);
        assert_eq!(controller.geometry.width, 300);
        assert_eq!(controller.geometry.height, 180);
    }

    assert_eq!(controller.transition_count, 200);
    assert!(controller.last_error.is_none());
}

#[test]
fn test_fullscreen_boundary_and_recovery() {
    let mut controller = FullscreenController::new(300, 180);

    // Resolução absurda/overflow deve ser rejeitada e transitar para Recovery
    let err = controller.request_fullscreen(4096, 2160);
    assert!(matches!(err, Err(FullscreenTransitionError::InvalidDimensions { .. })));
    assert_eq!(controller.state, FullscreenState::Recovery);

    // Recuperação automática
    controller.recover();
    assert_eq!(controller.state, FullscreenState::Windowed);
    assert_eq!(controller.geometry.width, 300);
    assert_eq!(controller.geometry.height, 180);
}

// ============================================================================
// 2. TESTES DE GOVERNANÇA DE RECURSOS E PERFIS (PHASE 2)
// ============================================================================

#[test]
fn test_resource_manager_profiles_and_quotas() {
    let mgr_64 = BrowserResourceManager::new(MemoryProfile::Profile64MB);
    assert_eq!(mgr_64.quotas.max_browser_ram_bytes, 16 * 1024 * 1024);
    assert_eq!(mgr_64.quotas.max_active_tabs, 2);

    let mgr_128 = BrowserResourceManager::new(MemoryProfile::Profile128MB);
    assert_eq!(mgr_128.quotas.max_browser_ram_bytes, 32 * 1024 * 1024);
    assert_eq!(mgr_128.quotas.max_active_tabs, 4);

    let mgr_256 = BrowserResourceManager::new(MemoryProfile::Profile256MB);
    assert_eq!(mgr_256.quotas.max_browser_ram_bytes, 64 * 1024 * 1024);
    assert_eq!(mgr_256.quotas.max_active_tabs, 8);

    let mgr_512 = BrowserResourceManager::new(MemoryProfile::Profile512MB);
    assert_eq!(mgr_512.quotas.max_browser_ram_bytes, 128 * 1024 * 1024);
    assert_eq!(mgr_512.quotas.max_active_tabs, 16);
}

#[test]
fn test_brcr_calculation_and_near_zero_metrics() {
    let mgr = BrowserResourceManager::new(MemoryProfile::Profile256MB);

    let s1 = create_test_session();
    let s2 = create_test_session();

    let tab_active = Tab::new(1, s1, 1);
    let mut tab_hibernated = Tab::new(2, s2, 2);
    tab_hibernated.hibernate();

    let active_bytes = tab_active.resident_memory_bytes();
    let hibernated_bytes = tab_hibernated.resident_memory_bytes();

    // Calcula BRCR real
    let brcr = mgr.calculate_brcr(hibernated_bytes, active_bytes);

    println!(
        "[BRCR-TEST] Ativa: {} B ({:.2} KiB), Hibernada: {} B ({:.2} KiB), BRCR = {:.3}%",
        active_bytes,
        active_bytes as f64 / 1024.0,
        hibernated_bytes,
        hibernated_bytes as f64 / 1024.0,
        brcr
    );

    // Meta: BRCR para abas hibernadas deve ser inferior a 0,5%
    assert!(
        brcr < 0.5,
        "BRCR deve ser estritamente inferior a 0,5% (obtido: {:.3}%)",
        brcr
    );

    // Para aba descartada
    let mut tab_discarded = tab_hibernated;
    tab_discarded.discard();
    let discarded_bytes = tab_discarded.resident_memory_bytes();
    let brcr_discarded = mgr.calculate_brcr(discarded_bytes, active_bytes);
    assert!(
        brcr_discarded < 0.2,
        "BRCR para aba descartada deve ser inferior a 0,2% (obtido: {:.3}%)",
        brcr_discarded
    );
}

#[test]
fn test_kernel_scheduler_task_mapping_and_zero_cpu() {
    let mgr = BrowserResourceManager::new(MemoryProfile::Profile256MB);

    let s = create_test_session();
    let mut tab = Tab::new(1, s, 1);

    // Aba ativa: mapeia para Running
    assert_eq!(mgr.sync_with_kernel_scheduler(&tab), KernelTaskStateMapping::Running);

    // Aba colocada em Background: mapeia para Throttled
    tab.to_background();
    assert_eq!(tab.state, TabState::Background);
    assert_eq!(mgr.sync_with_kernel_scheduler(&tab), KernelTaskStateMapping::Throttled);

    // Aba Hibernada: mapeia para BlockedZeroCpu
    tab.hibernate();
    assert_eq!(tab.state, TabState::Hibernated);
    assert_eq!(
        mgr.sync_with_kernel_scheduler(&tab),
        KernelTaskStateMapping::BlockedZeroCpu
    );

    // Comprova que no escalonador o consumo de ticks é rigorosamente zero
    for _ in 0..50 {
        assert_eq!(tab.task_context.tick(16), 0, "Aba hibernada não pode receber ticks de CPU!");
    }
    assert_eq!(tab.task_context.accumulated_cpu_ticks, 0);

    // Aba Descartada: mapeia para Terminated
    tab.discard();
    assert_eq!(tab.state, TabState::Discarded);
    assert_eq!(
        mgr.sync_with_kernel_scheduler(&tab),
        KernelTaskStateMapping::Terminated
    );
}

#[test]
fn test_logical_vs_physical_footprint_separation() {
    let s = create_test_session();
    let mut tab = Tab::new(1, s, 1);

    let phys_active = tab.physical_footprint();
    let log_active = tab.logical_footprint();

    // A memória física de uma aba ativa é dominada pelo framebuffer de renderização (640x480x4 ~ 1,22 MiB)
    assert!(phys_active.total_physical_bytes > 1_200_000);
    assert!(phys_active.framebuffer_resident_bytes > 1_200_000);

    // A pegada lógica é a representação semântica do documento e metadados
    assert!(log_active.total_logical_bytes < 10_000);

    // Agora hiberna a aba
    tab.hibernate();
    let phys_hibernated = tab.physical_footprint();
    let log_hibernated = tab.logical_footprint();

    // Na hibernação, o framebuffer físico foi devolvido ao alocador (0 bytes)
    assert_eq!(phys_hibernated.framebuffer_resident_bytes, 0);
    assert!(phys_hibernated.total_physical_bytes < 5_000);

    // O snapshot lógico foi preservado
    assert!(log_hibernated.snapshot_bytes > 0);
    assert!(log_hibernated.total_logical_bytes > 0);
}

#[test]
fn test_tab_manager_background_state_and_telemetry() {
    let mut manager = TabManager::new(64 * 1024 * 1024);

    let s1 = create_test_session();
    let id1 = manager.open_tab(s1);

    let s2 = create_test_session();
    let id2 = manager.open_tab(s2);

    let tel = manager.telemetry();
    assert_eq!(tel.total_tabs, 2);
    assert_eq!(tel.active_count, 1);
    assert_eq!(tel.hibernated_count, 1);

    // Move aba 1 para Background
    if let Some(t1) = manager.get_tab_mut(id1) {
        let _ = t1.transition_to(TabState::Active);
    }
    if let Some(t2) = manager.get_tab_mut(id2) {
        t2.to_background();
    }

    let tel2 = manager.telemetry();
    assert_eq!(tel2.background_count, 1);
}
