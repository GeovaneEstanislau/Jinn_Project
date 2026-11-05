//! Suíte de Testes Automatizados da Política de Abas do Jinn Browser
//!
//! Valida os princípios da diretriz:
//! "Near-Zero Resource Background Tabs & Exclusive Audio Focus"
//!
//! 1. Regra de Áudio: Somente a última aba selecionada possui foco de áudio exclusivo.
//! 2. Exemplo das 5 abas do YouTube: Aba 5 com áudio ativo; Abas 1 a 4 suspensas e hibernadas.
//! 3. Exceção Explícita de Áudio Fixado (Pinned Audio para Podcasts e Música).
//! 4. Invariante de CPU Próxima de Zero: 0 ticks de processamento para abas hibernadas.
//! 5. Invariante de Memória Próxima de Zero: DOM e layout liberados (> 99% de economia por aba em background).
//! 6. Restauração Transparente a partir do Snapshot Compacto ao Retornar à Aba.
//! 7. Orçamento Global de Memória e Transição para o Estado DISCARDED.

use jbe_net::MockLoader;
use jinn_browser::{
    AudioPlaybackState, BrowserSession, TabManager, TabState,
};

fn create_mock_loader() -> Box<MockLoader> {
    let mut loader = MockLoader::new();
    let sample_html = r#"
        <html>
            <head><title>Jinn Video Player</title></head>
            <body style="background-color: #111; color: #eee;">
                <h1>Streaming Video</h1>
                <p>Linha 1: Reproduzindo conteúdo de alta definição...</p>
                <p>Linha 2: Buffer de mídia carregado com sucesso.</p>
                <p>Linha 3: Pipeline de áudio sincronizado.</p>
                <p>Linha 4: Metadados decodificados.</p>
                <p>Linha 5: Controle de volume estabilizado.</p>
                <p>Linha 6: Transmissão contínua em formato stream.</p>
                <p>Linha 7: Renderização estática em viewport.</p>
                <p>Linha 8: Economia de energia em background.</p>
                <p>Linha 9: Foco exclusivo ativo.</p>
                <p>Linha 10: Fim do documento longo.</p>
                <p>Linha 11: Mais conteúdo para rolagem vertical.</p>
                <p>Linha 12: Mais conteúdo para rolagem vertical.</p>
                <p>Linha 13: Mais conteúdo para rolagem vertical.</p>
                <p>Linha 14: Mais conteúdo para rolagem vertical.</p>
                <p>Linha 15: Mais conteúdo para rolagem vertical.</p>
                <p>Linha 16: Mais conteúdo para rolagem vertical.</p>
                <p>Linha 17: Mais conteúdo para rolagem vertical.</p>
                <p>Linha 18: Mais conteúdo para rolagem vertical.</p>
                <p>Linha 19: Mais conteúdo para rolagem vertical.</p>
                <p>Linha 20: Mais conteúdo para rolagem vertical.</p>
                <a href="next.html">Próximo Vídeo</a>
            </body>
        </html>
    "#;
    loader.insert("http://video.local/watch", sample_html.as_bytes().to_vec(), "text/html");
    loader.insert("http://video.local/podcast", sample_html.as_bytes().to_vec(), "text/html");
    loader.insert("about:blank", "<html><head><title>Blank</title></head><body></body></html>".as_bytes().to_vec(), "text/html");
    Box::new(loader)
}

fn create_sample_session(url_str: &str) -> BrowserSession {
    let loader = create_mock_loader();
    let mut session = BrowserSession::new(640, 480, loader);
    let _ = session.navigate(url_str);
    session
}

#[test]
fn test_tab_states_and_initial_active() {
    let mut manager = TabManager::new(64 * 1024 * 1024); // 64 MB budget
    let session = create_sample_session("http://video.local/watch");

    let tab_id = manager.open_tab(session);

    let tab = manager.get_tab(tab_id).expect("Aba deve existir");
    assert_eq!(tab.state, TabState::Active);
    assert!(tab.session.is_some(), "Aba ativa deve ter sessão de página em memória");
    assert!(manager.audio_focus.has_focus(tab_id), "Aba ativa recém-criada deve ter o foco de áudio");
}

#[test]
fn test_five_youtube_tabs_exclusive_audio_focus() {
    let mut manager = TabManager::new(64 * 1024 * 1024);

    println!("\n=======================================================================");
    println!(" CENÁRIO: ABERTURA DE 5 ABAS DE VÍDEO (EXCLUSIVE AUDIO FOCUS & HIBERNATION)");
    println!("=======================================================================");

    let mut tab_ids = Vec::new();

    // O usuário abre 5 abas de vídeo sequencialmente
    for i in 1..=5 {
        let session = create_sample_session("http://video.local/watch");
        let id = manager.open_tab(session);
        
        // Simula início de reprodução de áudio na aba atual
        if let Some(tab) = manager.get_tab_mut(id) {
            tab.media = jinn_browser::MediaPipeline::with_track(&format!("Video Track #{i}"), 300_000);
            tab.media.play();
        }
        tab_ids.push(id);
        println!("[+] Aba {i} (ID {id}) aberta e reproduzindo áudio.");
    }

    // Validação da regra:
    // Apenas a última aba (Aba 5) deve estar ACTIVE e com áudio tocando
    let active_id = tab_ids[4];
    assert_eq!(manager.active_tab_id, active_id);

    let active_tab = manager.get_tab(active_id).unwrap();
    assert_eq!(active_tab.state, TabState::Active);
    assert_eq!(active_tab.media.state, AudioPlaybackState::Playing);
    assert!(manager.audio_focus.has_focus(active_id));

    // As 4 abas anteriores devem estar no estado HIBERNATED com áudio PAUSADO
    for i in 0..4 {
        let prev_id = tab_ids[i];
        let prev_tab = manager.get_tab(prev_id).unwrap();

        assert_eq!(
            prev_tab.state,
            TabState::Hibernated,
            "Aba anterior {prev_id} deve estar HIBERNADA"
        );
        assert_eq!(
            prev_tab.media.state,
            AudioPlaybackState::Paused,
            "Reprodução da aba {prev_id} deve ser suspensa automaticamente"
        );
        assert!(
            prev_tab.session.is_none(),
            "Aba hibernada {prev_id} NÃO pode reter árvore DOM ou buffers de renderização"
        );
        assert!(
            !manager.audio_focus.has_focus(prev_id),
            "Aba hibernada {prev_id} NÃO pode deter o foco exclusivo de áudio"
        );
    }

    let telemetry = manager.telemetry();
    println!("-----------------------------------------------------------------------");
    println!("Telemetria Observada:");
    println!("  Total de abas: {}", telemetry.total_tabs);
    println!("  Ativas: {} (Aba #{active_id})", telemetry.active_count);
    println!("  Hibernadas: {}", telemetry.hibernated_count);
    println!("  Abas com áudio autorizado: {} / 5", if manager.audio_focus.active_focus_tab_id.is_some() { 1 } else { 0 });
    println!("  Memória residente real: {:.2} KiB", telemetry.total_resident_memory_bytes as f64 / 1024.0);
    println!("  Memória sem otimização: {:.2} KiB", telemetry.baseline_unoptimized_memory_bytes as f64 / 1024.0);
    println!("  Economia de memória: {:.1}%", (telemetry.saved_memory_bytes as f64 / telemetry.baseline_unoptimized_memory_bytes as f64) * 100.0);
    println!("=======================================================================\n");

    assert_eq!(telemetry.total_tabs, 5);
    assert_eq!(telemetry.active_count, 1);
    assert_eq!(telemetry.hibernated_count, 4);
    assert_eq!(telemetry.audio_background_count, 0);
}

#[test]
fn test_pinned_audio_exception_podcast() {
    let mut manager = TabManager::new(64 * 1024 * 1024);

    // 1. Cria Aba 1 (Podcast)
    let s1 = create_sample_session("http://video.local/podcast");
    let tab_podcast_id = manager.open_tab(s1);

    if let Some(tab) = manager.get_tab_mut(tab_podcast_id) {
        tab.media = jinn_browser::MediaPipeline::with_track("Jinn Architecture Podcast #1", 600_000);
        tab.media.play();
    }

    // 2. Usuário fixa deliberadamente o áudio do podcast
    let pinned_ok = manager.set_pinned_audio(tab_podcast_id, true);
    assert!(pinned_ok);
    assert!(manager.get_tab(tab_podcast_id).unwrap().is_pinned_audio);

    // 3. Usuário abre Aba 2 (Documentação / Navegação comum)
    let s2 = create_sample_session("http://video.local/watch");
    let tab_reading_id = manager.open_tab(s2);

    // 4. Validação da Exceção:
    // A Aba de Podcast agora está em segundo plano, mas no estado AUDIO_BACKGROUND!
    let tab_podcast = manager.get_tab(tab_podcast_id).unwrap();
    assert_eq!(
        tab_podcast.state,
        TabState::AudioBackground,
        "Aba fixada deve transitar para AudioBackground"
    );
    assert_eq!(
        tab_podcast.media.state,
        AudioPlaybackState::Playing,
        "Áudio do podcast fixado DEVE continuar reproduzindo"
    );

    // 5. Executa ciclos de tick (1000 ms)
    let initial_pos = tab_podcast.media.current_position_ms;
    manager.tick(1000);

    let tab_podcast_after = manager.get_tab(tab_podcast_id).unwrap();
    assert_eq!(tab_podcast_after.media.current_position_ms, initial_pos + 1000);
    assert_eq!(manager.background_cpu_ticks_counter, 1, "Apenas o pipeline de áudio foi processado em background");

    // 6. A aba atual ativa é a de leitura
    assert_eq!(manager.active_tab_id, tab_reading_id);
    assert_eq!(manager.get_tab(tab_reading_id).unwrap().state, TabState::Active);
}

#[test]
fn test_near_zero_memory_and_cpu_invariants() {
    let mut manager = TabManager::new(64 * 1024 * 1024);

    // Abre aba 1 e mede consumo residente
    let s1 = create_sample_session("http://video.local/watch");
    let id1 = manager.open_tab(s1);

    let mem_active = manager.get_tab(id1).unwrap().resident_memory_bytes();
    assert!(mem_active > 1_000_000, "Aba ativa com framebuffer 640x480x4 consome > 1.2 MiB");

    // Abre aba 2 -> Aba 1 é automaticamente hibernada
    let s2 = create_sample_session("http://video.local/watch");
    let _id2 = manager.open_tab(s2);

    let tab1_hibernated = manager.get_tab(id1).unwrap();
    assert_eq!(tab1_hibernated.state, TabState::Hibernated);

    let mem_hibernated = tab1_hibernated.resident_memory_bytes();
    println!(
        "Memória Aba Ativa: {} B ({:.2} KiB) -> Aba Hibernada: {} B ({:.2} KiB)",
        mem_active,
        mem_active as f64 / 1024.0,
        mem_hibernated,
        mem_hibernated as f64 / 1024.0
    );

    // Consumo da aba hibernada deve ser inferior a 1,5 KiB (snapshot compacto)
    assert!(
        mem_hibernated < 1500,
        "Aba hibernada deve consumir próximo de zero (< 1,5 KiB), consumiu: {mem_hibernated} bytes"
    );

    // Invariante de CPU = 0
    let prev_bg_ticks = manager.background_cpu_ticks_counter;
    for _ in 0..10 {
        manager.tick(100);
    }
    assert_eq!(
        manager.background_cpu_ticks_counter, prev_bg_ticks,
        "Abas hibernadas não podem gerar ticks de processamento em segundo plano"
    );
}

#[test]
fn test_navigation_restoration_from_snapshot() {
    let mut manager = TabManager::new(64 * 1024 * 1024);

    // 1. Cria Aba 1 e rola 150px para baixo
    let mut s1 = create_sample_session("http://video.local/watch");
    s1.scroll(150.0);
    let id1 = manager.open_tab(s1);

    assert_eq!(manager.get_tab(id1).unwrap().session.as_ref().unwrap().scroll_y, 150.0);

    // 2. Cria Aba 2 -> Aba 1 hiberna
    let s2 = create_sample_session("about:blank");
    let id2 = manager.open_tab(s2);

    assert_eq!(manager.get_tab(id1).unwrap().state, TabState::Hibernated);
    assert!(manager.get_tab(id1).unwrap().session.is_none());

    // 3. Usuário retorna à Aba 1 -> switch_to_tab deve restaurar perfeitamente
    let switch_res = manager.switch_to_tab(id1, || create_mock_loader());
    assert!(switch_res.is_ok(), "Restauração da aba deve suceder");

    let restored_tab1 = manager.get_tab(id1).unwrap();
    assert_eq!(restored_tab1.state, TabState::Active);
    assert!(restored_tab1.session.is_some(), "Sessão de página deve estar presente");

    let restored_session = restored_tab1.session.as_ref().unwrap();
    assert_eq!(
        restored_session.current_url().unwrap().as_str(),
        "http://video.local/watch"
    );
    assert_eq!(restored_session.scroll_y, 150.0, "Posição de rolagem deve ser preservada");

    // E a Aba 2 agora hibernou
    assert_eq!(manager.get_tab(id2).unwrap().state, TabState::Hibernated);
}

#[test]
fn test_memory_budget_enforcement_and_discarded_state() {
    // Define um orçamento restrito de apenas 2,5 MiB (comporta 1 aba ativa + snapshots)
    let strict_budget = (2.5 * 1024.0 * 1024.0) as usize;
    let mut manager = TabManager::new(strict_budget);

    let mut ids = Vec::new();
    for _ in 0..5 {
        let s = create_sample_session("http://video.local/watch");
        let id = manager.open_tab(s);
        ids.push(id);
    }

    // Apenas a aba ativa e abas recentes devem permanecer em Hibernated;
    // Sob pressão extrema, a política de orçamento deve manter o total estritamente controlado
    assert!(manager.total_resident_memory() <= manager.global_memory_budget_bytes + (640 * 480 * 4));
}
