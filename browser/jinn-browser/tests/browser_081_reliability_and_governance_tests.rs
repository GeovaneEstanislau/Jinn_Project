//! Jinn Browser 0.8.1 — Reliability Engineering, Fullscreen Hardening & Kernel Resource Governance Tests
//!
//! Suíte de testes automatizados para validação formal de:
//! 1. FullscreenController Two-Phase Commit e reversibilidade (begin/commit/abort).
//! 2. Rejeição de transições inválidas e integridade da máquina de estados.
//! 3. Métricas físicas granulares: resident_pages, released_pages, shared_memory, graphics/media footprints.
//! 4. Decomposição formal da métrica BRCR (BrcrMetricsBreakdown: CPU, Memória, Gráficos, Mídia, I/O, Composto).
//! 5. Imposição de CPU zero ticks pelo scheduler sob 1, 5, 10 e 20 abas hibernadas.
//! 6. Isolamento e preempção de tarefas de áudio vs tarefas visuais.
//! 7. Estresse de alternância rápida de abas e restauração repetida (100 iterações).

use jinn_browser::*;
use jbe_net::MockLoader;
use jbe_url::Url;

fn create_mock_loader() -> Box<MockLoader> {
    let mut loader = MockLoader::new();
    let sample_html = r#"
        <html>
            <head><title>Jinn Test Page 0.8.1</title></head>
            <body>
                <h1>Jinn Browser 0.8.1</h1>
                <p>Conteúdo de teste para validação de tela cheia e governança.</p>
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
    let _ = session.load_url(Url::parse("http://test.local/index").unwrap());
    session
}

#[test]
fn test_fullscreen_two_phase_commit_and_abort() {
    let mut fc = FullscreenController::new(300, 180);
    assert_eq!(fc.state, FullscreenState::Windowed);

    // Fase 1: Início da requisição (EnteringFullscreen)
    assert!(fc.begin_request_fullscreen(1024, 768).is_ok());
    assert_eq!(fc.state, FullscreenState::EnteringFullscreen);
    assert_eq!(fc.geometry.width, 1024);
    assert_eq!(fc.geometry.height, 768);
    assert_eq!(fc.geometry.saved_width, 300);
    assert_eq!(fc.geometry.saved_height, 180);

    // Abort/Rollback simulando falha de alocação ou cancelamento pelo compositor
    fc.abort_fullscreen();
    assert_eq!(fc.state, FullscreenState::Windowed);
    assert_eq!(fc.geometry.width, 300);
    assert_eq!(fc.geometry.height, 180);
    assert_eq!(fc.rollback_count, 1);

    // Transição bem-sucedida em duas fases
    assert!(fc.begin_request_fullscreen(1024, 768).is_ok());
    assert_eq!(fc.state, FullscreenState::EnteringFullscreen);
    assert!(fc.commit_fullscreen().is_ok());
    assert_eq!(fc.state, FullscreenState::Fullscreen);
    assert_eq!(fc.transition_count, 1);

    // Saída em duas fases
    assert!(fc.begin_exit_fullscreen().is_ok());
    assert_eq!(fc.state, FullscreenState::ExitingFullscreen);
    assert!(fc.commit_exit_fullscreen().is_ok());
    assert_eq!(fc.state, FullscreenState::Windowed);
    assert_eq!(fc.geometry.width, 300);
    assert_eq!(fc.geometry.height, 180);
    assert_eq!(fc.transition_count, 2);
}

#[test]
fn test_fullscreen_invalid_transitions_rejection() {
    let mut fc = FullscreenController::new(300, 180);

    // Não pode sair de fullscreen se já está em Windowed
    assert!(fc.begin_exit_fullscreen().is_err());
    assert!(fc.exit_fullscreen().is_err());

    // Não pode fazer commit de fullscreen se não está em EnteringFullscreen
    assert!(fc.commit_fullscreen().is_err());

    // Dimensões absurdas ou negativas/zero devem ser rejeitadas
    assert!(fc.begin_request_fullscreen(0, 0).is_err());
    assert!(fc.begin_request_fullscreen(50, 20).is_err()); // Abaixo do mínimo 100x60
    assert!(fc.begin_request_fullscreen(5000, 5000).is_err()); // Acima do máximo 1920x1080
}

#[test]
fn test_physical_memory_granular_metrics() {
    let s = create_test_session();
    let mut tab = Tab::new(1, s, 1);

    // Aba ativa
    let phys_active = tab.physical_footprint();
    assert!(phys_active.resident_pages > 0);
    assert_eq!(phys_active.released_pages, 0);
    assert!(phys_active.framebuffer_resident_bytes > 1_000_000); // 640x480x4
    assert!(phys_active.shared_memory_bytes > 1_000_000);

    // Hiberna a aba
    tab.hibernate();
    let phys_hib = tab.physical_footprint();
    assert_eq!(phys_hib.framebuffer_resident_bytes, 0);
    assert_eq!(phys_hib.shared_memory_bytes, 0);
    assert!(phys_hib.released_pages > 0);
    assert!(tab.released_pages() > 200); // Mais de 200 páginas físicas de 4 KiB liberadas
    assert!(phys_hib.total_physical_bytes < 5_000);
}

#[test]
fn test_detailed_brcr_metrics_breakdown() {
    let s1 = create_test_session();
    let s2 = create_test_session();
    let active_tab = Tab::new(1, s1, 1);
    let mut bg_tab = Tab::new(2, s2, 2);

    bg_tab.hibernate();

    let rm = BrowserResourceManager::new(MemoryProfile::Profile256MB);
    let brcr = rm.calculate_detailed_brcr(&active_tab, &bg_tab);

    // CPU em hibernação deve ser rigorosamente 0.0%
    assert_eq!(brcr.cpu_background_ratio, 0.0);

    // Retenção de memória física deve ser inferior a 0.5%
    assert!(brcr.memory_retention_ratio < 0.5, "Memory retention deve ser < 0.5%, medido: {}%", brcr.memory_retention_ratio);

    // Superfícies e buffers gráficos devem ser completamente liberados (0.0%)
    assert_eq!(brcr.graphics_retention_ratio, 0.0);

    // Buffers PCM de áudio liberados (0.0%)
    assert_eq!(brcr.media_retention_ratio, 0.0);

    // Atividade I/O de fundo deve ser 0.0%
    assert_eq!(brcr.io_background_ratio, 0.0);

    // Métrica composta ponderada (< 0.5%)
    assert!(brcr.composite_background_ratio < 0.5);
}

#[test]
fn test_scheduler_cpu_ticks_zero_under_1_5_10_20_hibernated_tabs() {
    let mut tabs = Vec::new();

    // 1 ativa + 20 abas para teste de escala
    for i in 0..21 {
        let s = create_test_session();
        let mut tab = Tab::new(i, s, i as u64);
        if i > 0 {
            tab.hibernate();
        }
        tabs.push(tab);
    }

    assert_eq!(tabs[0].state, TabState::Active);
    for tab in tabs.iter().skip(1) {
        assert_eq!(tab.state, TabState::Hibernated);
        assert_eq!(tab.task_context.execution_state, TaskExecutionState::Suspended);
    }

    // Simula 100 ticks globais do escalonador
    for _ in 0..100 {
        for tab in tabs.iter_mut() {
            let _ = tab.task_context.tick(10);
        }
    }

    // A aba ativa consumiu 100 ticks
    assert_eq!(tabs[0].task_context.accumulated_cpu_ticks, 100);

    // Todas as 20 abas hibernadas consumiram rigorosamente ZERO ticks
    for (idx, tab) in tabs.iter().enumerate().skip(1) {
        assert_eq!(
            tab.task_context.accumulated_cpu_ticks,
            0,
            "Aba hibernada {} consumiu ticks indevidos de CPU!",
            idx
        );
    }
}

#[test]
fn test_audio_task_isolation_and_pcm_cleanup() {
    let s = create_test_session();
    let mut tab = Tab::new(1, s, 1);
    let mut afm = AudioFocusManager::new();

    // Inicia áudio na aba
    tab.media.play();
    tab.media.buffer_samples = 8192; // Buffer PCM ativo
    afm.request_focus(1);
    assert_eq!(afm.active_focus_tab_id, Some(1));
    tab.is_pinned_audio = true;

    // Transita para AUDIO_BACKGROUND
    assert!(tab.transition_to(TabState::AudioBackground).is_ok());
    assert_eq!(tab.state, TabState::AudioBackground);
    assert_eq!(tab.task_context.execution_state, TaskExecutionState::ThrottledAudioOnly);
    assert_eq!(tab.media.buffer_samples, 8192); // Áudio preservado

    // Perda de foco de áudio por outra aba
    afm.release_focus(1);
    tab.is_pinned_audio = false;

    // Entra em hibernação
    tab.hibernate();
    assert_eq!(tab.state, TabState::Hibernated);
    assert_eq!(tab.media.buffer_samples, 0); // Buffer PCM foi estritamente desalocado
    assert_eq!(tab.task_context.execution_state, TaskExecutionState::Suspended);
}

#[test]
fn test_tab_restoration_stress_100_cycles() {
    let s = create_test_session();
    let mut tab = Tab::new(1, s, 1);

    for cycle in 0..100 {
        // Active -> Hibernated
        tab.hibernate();
        assert_eq!(tab.state, TabState::Hibernated);
        assert!(tab.snapshot.is_some());

        // Hibernated -> Active (Restore)
        let loader = create_mock_loader();
        assert!(tab.restore(loader).is_ok(), "Falha na restauração no ciclo {}", cycle);
        assert_eq!(tab.state, TabState::Active);
        assert!(tab.session.is_some());
    }
}
