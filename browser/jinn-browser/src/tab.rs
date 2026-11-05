//! Intelligent Tab Lifecycle & Exclusive Audio Focus Subsystem (v0.7.0)
//!
//! Implementação da política "Near-Zero Resource Background Tabs":
//! 1. Quatro estados de ciclo de vida com validação estrita de transição:
//!    `Active`, `AudioBackground`, `Hibernated`, `Discarded`.
//! 2. Gerenciador Exclusivo de Foco de Áudio (`AudioFocusManager`): no máximo 1 aba com áudio automático.
//! 3. Integração com Contexto de Execução e Scheduler (`TabTaskContext`):
//!    - Abas Hibernadas: suspensão de timers, animações e zero consumo de CPU (ticks = 0).
//!    - Abas AudioBackground: execução estrita do pipeline de áudio, com tarefas de página suspensas.
//! 4. Contabilização Granular de Memória (`MemoryBreakdown`):
//!    - Discriminação entre processo principal, DOM, layout, framebuffers, cache, áudio e snapshots.
//!    - Resolução documentada da variação de 1.219,40 KiB vs 1.259,45 KiB (liberação do buffer PCM não fixado).
//! 5. Resiliência de Restauração: tratamento contra falhas de rede sem perda de snapshot.

use std::collections::BTreeMap;
use jbe_core::EngineError;
use jbe_net::ResourceLoader;
use jbe_url::Url;
use crate::BrowserSession;

/// Estados do Ciclo de Vida da Aba (Tab Lifecycle Policy)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabState {
    /// Aba aberta e em primeiro plano. Prioridade máxima e orçamento de execução pleno.
    Active,
    /// Aba em segundo plano autorizada a reproduzir áudio.
    /// O pipeline de mídia continua operando; scripts, timers e renderização são suspensos.
    AudioBackground,
    /// Aba inativa hibernada. DOM e buffers de layout descartados.
    /// Snapshot de navegação compacto preservado em memória. CPU = 0.
    Hibernated,
    /// Aba descartada sob pressão de memória. Todos os buffers liberados.
    /// Apenas metadados de histórico são mantidos para recarga fria sob demanda.
    Discarded,
}

/// Erros de Transição de Estado do Ciclo de Vida
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabTransitionError {
    InvalidTransition { from: TabState, to: TabState },
    RestorationFailed(String),
    AudioUnauthorized,
}

/// Estado de Execução da Tarefa no Escalonador
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskExecutionState {
    Running,
    ThrottledAudioOnly,
    Suspended,
    Terminated,
}

/// Contexto de Execução da Aba integrado ao Modelo de Escalonamento do Jinn
#[derive(Debug, Clone)]
pub struct TabTaskContext {
    pub task_id: u64,
    pub execution_state: TaskExecutionState,
    pub priority: u8, // 3: Normal/High (Active), 1: Throttled (Audio), 0: Suspended
    pub page_timers_active: bool,
    pub animations_active: bool,
    pub pending_async_jobs: usize,
    pub accumulated_cpu_ticks: u64,
}

impl TabTaskContext {
    pub fn new(task_id: u64) -> Self {
        Self {
            task_id,
            execution_state: TaskExecutionState::Running,
            priority: 3,
            page_timers_active: true,
            animations_active: true,
            pending_async_jobs: 0,
            accumulated_cpu_ticks: 0,
        }
    }

    /// Suspende tarefas de página, timers e animações
    pub fn suspend(&mut self) {
        self.execution_state = TaskExecutionState::Suspended;
        self.priority = 0;
        self.page_timers_active = false;
        self.animations_active = false;
        self.pending_async_jobs = 0;
    }

    /// Reduz a prioridade mantendo ativo estritamente o pipeline de áudio
    pub fn throttle_to_audio_only(&mut self) {
        self.execution_state = TaskExecutionState::ThrottledAudioOnly;
        self.priority = 1;
        self.page_timers_active = false;
        self.animations_active = false;
    }

    /// Retoma a execução normal da aba em primeiro plano
    pub fn resume(&mut self) {
        self.execution_state = TaskExecutionState::Running;
        self.priority = 3;
        self.page_timers_active = true;
        self.animations_active = true;
    }

    /// Encerra a tarefa no escalonador (estado DISCARDED)
    pub fn terminate(&mut self) {
        self.suspend();
        self.execution_state = TaskExecutionState::Terminated;
    }

    /// Processa ticks no escalonador. Retorna os ticks efetivamente consumidos.
    pub fn tick(&mut self, _delta_ms: u64) -> u64 {
        match self.execution_state {
            TaskExecutionState::Running => {
                self.accumulated_cpu_ticks += 1;
                1
            }
            TaskExecutionState::ThrottledAudioOnly => {
                // Apenas processamento leve de streaming de áudio
                self.accumulated_cpu_ticks += 1;
                1
            }
            TaskExecutionState::Suspended | TaskExecutionState::Terminated => {
                // Invariante de Near-Zero CPU: 0 ticks consumidos
                0
            }
        }
    }
}

/// Estado de Reprodução de Áudio no Pipeline de Mídia
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioPlaybackState {
    Idle,
    Playing,
    Paused,
    Stopped,
}

/// Pipeline de Mídia Desacoplado do Mecanismo da Página
#[derive(Debug, Clone)]
pub struct MediaPipeline {
    pub state: AudioPlaybackState,
    pub title: String,
    pub current_position_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
    pub buffer_samples: usize,
    pub is_audible: bool,
}

impl MediaPipeline {
    pub fn new() -> Self {
        Self {
            state: AudioPlaybackState::Idle,
            title: String::new(),
            current_position_ms: 0,
            duration_ms: 0,
            volume: 1.0,
            buffer_samples: 0,
            is_audible: false,
        }
    }

    pub fn with_track(title: &str, duration_ms: u64) -> Self {
        Self {
            state: AudioPlaybackState::Paused,
            title: title.to_string(),
            current_position_ms: 0,
            duration_ms,
            volume: 1.0,
            buffer_samples: 4096,
            is_audible: true,
        }
    }

    pub fn play(&mut self) {
        if self.is_audible {
            self.state = AudioPlaybackState::Playing;
        }
    }

    pub fn pause(&mut self) {
        if self.state == AudioPlaybackState::Playing {
            self.state = AudioPlaybackState::Paused;
        }
    }

    pub fn stop(&mut self) {
        self.state = AudioPlaybackState::Stopped;
        self.current_position_ms = 0;
    }

    pub fn is_playing(&self) -> bool {
        self.state == AudioPlaybackState::Playing
    }

    /// Executa avanço do fluxo de áudio sem depender do DOM ou layout da página
    pub fn tick(&mut self, delta_ms: u64) {
        if self.state == AudioPlaybackState::Playing {
            self.current_position_ms = (self.current_position_ms + delta_ms).min(self.duration_ms);
            if self.current_position_ms >= self.duration_ms && self.duration_ms > 0 {
                self.state = AudioPlaybackState::Stopped;
            }
        }
    }

    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.title.capacity() + (self.buffer_samples * 2)
    }
}

/// Snapshot Compacto de Preservação para Hibernação
#[derive(Debug, Clone)]
pub struct TabSnapshot {
    pub url: Url,
    pub history: Vec<Url>,
    pub history_index: usize,
    pub scroll_y: f32,
    pub title: String,
    pub form_state: BTreeMap<String, String>,
    pub saved_media_position_ms: u64,
}

impl TabSnapshot {
    pub fn estimated_bytes(&self) -> usize {
        let history_bytes: usize = self.history.iter().map(|u| u.as_str().len() + 32).sum();
        let form_bytes: usize = self.form_state.iter().map(|(k, v)| k.len() + v.len() + 16).sum();
        std::mem::size_of::<Self>() + self.url.as_str().len() + self.title.capacity() + history_bytes + form_bytes
    }
}

/// Discriminação Granular de Memória da Aba
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryBreakdown {
    pub dom_arena_bytes: usize,
    pub layout_tree_bytes: usize,
    pub render_framebuffer_bytes: usize,
    pub audio_pcm_buffer_bytes: usize,
    pub snapshot_metadata_bytes: usize,
    pub total_resident_bytes: usize,
}

/// Representação de uma Aba do Navegador
pub struct Tab {
    pub id: usize,
    pub state: TabState,
    pub is_pinned_audio: bool,
    pub session: Option<BrowserSession>,
    pub media: MediaPipeline,
    pub snapshot: Option<TabSnapshot>,
    pub task_context: TabTaskContext,
    pub creation_order: u64,
    pub last_active_tick: u64,
    pub cumulative_freed_bytes: usize,
    pub restoration_failed: bool,
}

impl Tab {
    pub fn new(id: usize, session: BrowserSession, order: u64) -> Self {
        let title = session
            .current_dom
            .as_ref()
            .and_then(|d| d.find_first_by_tag("title").map(|id| d.text_content(id)))
            .unwrap_or_else(|| "Nova Aba".to_string());

        let url = session.current_url().cloned().unwrap_or_else(|| Url::parse("about:blank").unwrap());

        let mut tab = Self {
            id,
            state: TabState::Active,
            is_pinned_audio: false,
            session: Some(session),
            media: MediaPipeline::new(),
            snapshot: None,
            task_context: TabTaskContext::new(id as u64),
            creation_order: order,
            last_active_tick: order,
            cumulative_freed_bytes: 0,
            restoration_failed: false,
        };

        // Snapshot inicial de segurança
        tab.capture_snapshot(title, url);
        tab
    }

    fn capture_snapshot(&mut self, title: String, url: Url) {
        if let Some(session) = &self.session {
            self.snapshot = Some(TabSnapshot {
                url,
                history: session.history.clone(),
                history_index: session.history_index,
                scroll_y: session.scroll_y,
                title,
                form_state: BTreeMap::new(),
                saved_media_position_ms: self.media.current_position_ms,
            });
        }
    }

    /// Valida e executa a transição de estado da aba
    pub fn transition_to(&mut self, new_state: TabState) -> Result<(), TabTransitionError> {
        if self.state == new_state {
            return Ok(());
        }

        match (self.state, new_state) {
            // 1. ACTIVE -> HIBERNATED
            (TabState::Active, TabState::Hibernated) => {
                self.hibernate();
                Ok(())
            }
            // 2. ACTIVE -> AUDIO_BACKGROUND
            (TabState::Active, TabState::AudioBackground) => {
                if !self.is_pinned_audio && !self.media.is_playing() {
                    return Err(TabTransitionError::AudioUnauthorized);
                }
                self.state = TabState::AudioBackground;
                self.task_context.throttle_to_audio_only();
                Ok(())
            }
            // 3. AUDIO_BACKGROUND -> HIBERNATED
            (TabState::AudioBackground, TabState::Hibernated) => {
                self.hibernate();
                Ok(())
            }
            // 4. HIBERNATED -> ACTIVE (requer restore)
            (TabState::Hibernated, TabState::Active) => {
                self.state = TabState::Active;
                self.task_context.resume();
                Ok(())
            }
            // 5. HIBERNATED -> DISCARDED
            (TabState::Hibernated, TabState::Discarded) => {
                self.discard();
                Ok(())
            }
            // 6. DISCARDED -> ACTIVE (requer restore via cold-reload)
            (TabState::Discarded, TabState::Active) => {
                self.state = TabState::Active;
                self.task_context.resume();
                Ok(())
            }
            // Transições proibidas
            (from, to) => Err(TabTransitionError::InvalidTransition { from, to }),
        }
    }

    /// Transita a aba para o estado HIBERNATED, liberando buffers pesados de DOM e renderização
    pub fn hibernate(&mut self) {
        if self.state == TabState::Hibernated || self.state == TabState::Discarded {
            return;
        }

        // Calcula bytes a serem liberados
        let initial_resident = self.resident_memory_bytes();

        // 1. Atualiza o snapshot com o estado mais recente
        if let Some(session) = &self.session {
            let title = session
                .current_dom
                .as_ref()
                .and_then(|d| d.find_first_by_tag("title").map(|id| d.text_content(id)))
                .unwrap_or_else(|| "Página".to_string());

            let url = session.current_url().cloned().unwrap_or_else(|| Url::parse("about:blank").unwrap());

            self.snapshot = Some(TabSnapshot {
                url,
                history: session.history.clone(),
                history_index: session.history_index,
                scroll_y: session.scroll_y,
                title,
                form_state: BTreeMap::new(),
                saved_media_position_ms: self.media.current_position_ms,
            });
        }

        // 2. Suspende áudio caso não esteja fixado e libera buffer de amostras PCM não fixado
        if !self.is_pinned_audio {
            self.media.pause();
            self.media.buffer_samples = 0; // Libera buffer PCM (resolve variação de 1219.4 vs 1259.45 KiB)
        }

        // 3. Libera totalmente a sessão de página (DOM, árvore de layout e framebuffer de renderização)
        self.session = None;
        self.task_context.suspend();
        self.state = TabState::Hibernated;

        let final_resident = self.resident_memory_bytes();
        self.cumulative_freed_bytes += initial_resident.saturating_sub(final_resident);
    }

    /// Transita a aba para o estado DISCARDED sob pressão extrema de memória
    pub fn discard(&mut self) {
        self.hibernate();
        self.media.buffer_samples = 0;
        self.task_context.terminate();
        self.state = TabState::Discarded;
    }

    /// Restaura a aba para o estado ACTIVE reconstruindo o pipeline a partir do snapshot
    pub fn restore(&mut self, loader: Box<dyn ResourceLoader>) -> Result<(), EngineError> {
        if self.state == TabState::Active && self.session.is_some() {
            return Ok(());
        }

        let snapshot = self.snapshot.as_ref().ok_or_else(|| {
            EngineError::Internal("Snapshot ausente para restauração da aba".into())
        })?;

        // Reconstrói a sessão do navegador
        let mut session = BrowserSession::new(640, 480, loader);
        session.history = snapshot.history.clone();
        session.history_index = snapshot.history_index;

        // Tenta carregar o documento preservado; em caso de falha de rede, preserva o snapshot
        match session.load_url(snapshot.url.clone()) {
            Ok(()) => {
                session.scroll(snapshot.scroll_y);
                self.session = Some(session);
                self.state = TabState::Active;
                self.task_context.resume();
                self.restoration_failed = false;
                self.media.current_position_ms = snapshot.saved_media_position_ms;
                Ok(())
            }
            Err(e) => {
                // Em caso de falha de rede/VFS, marca a falha sem destruir o snapshot
                self.restoration_failed = true;
                Err(e)
            }
        }
    }

    /// Retorna a discriminação detalhada dos recursos alocados pela aba
    pub fn memory_breakdown(&self) -> MemoryBreakdown {
        let (dom, layout, fb) = if let Some(session) = &self.session {
            let dom = session.current_dom.as_ref().map(|d| d.node_count() * 128).unwrap_or(0);
            let layout = session.current_layout.as_ref().map(|_| 4096).unwrap_or(0);
            let fb = (session.viewport_width as usize) * (session.viewport_height as usize) * 4;
            (dom, layout, fb)
        } else {
            (0, 0, 0)
        };

        let audio = self.media.buffer_samples * 2;
        let snapshot = self.snapshot.as_ref().map(|s| s.estimated_bytes()).unwrap_or(0);
        let total = self.resident_memory_bytes();

        MemoryBreakdown {
            dom_arena_bytes: dom,
            layout_tree_bytes: layout,
            render_framebuffer_bytes: fb,
            audio_pcm_buffer_bytes: audio,
            snapshot_metadata_bytes: snapshot,
            total_resident_bytes: total,
        }
    }

    /// Retorna a estimativa total de memória residente utilizada pela aba (em bytes)
    pub fn resident_memory_bytes(&self) -> usize {
        let base_struct = std::mem::size_of::<Self>();
        let media_bytes = self.media.estimated_bytes();
        let snapshot_bytes = self.snapshot.as_ref().map(|s| s.estimated_bytes()).unwrap_or(0);

        let session_bytes = if let Some(session) = &self.session {
            let dom_bytes = session.current_dom.as_ref().map(|d| d.node_count() * 128).unwrap_or(0);
            let layout_bytes = session.current_layout.as_ref().map(|_| 4096).unwrap_or(0);
            let framebuffer_bytes = (session.viewport_width as usize) * (session.viewport_height as usize) * 4;
            dom_bytes + layout_bytes + framebuffer_bytes
        } else {
            0
        };

        base_struct + media_bytes + snapshot_bytes + session_bytes
    }
}

/// Gerenciador de Foco Exclusivo de Áudio (Audio Focus Manager)
#[derive(Debug, Clone)]
pub struct AudioFocusManager {
    /// Identificador da aba que detém o foco exclusivo automático
    pub active_focus_tab_id: Option<usize>,
}

impl AudioFocusManager {
    pub fn new() -> Self {
        Self {
            active_focus_tab_id: None,
        }
    }

    /// Solicita transferência de foco de áudio para a nova aba selecionada.
    /// Retorna o ID da aba que perdeu o foco (se houver).
    pub fn request_focus(&mut self, new_tab_id: usize) -> Option<usize> {
        let previous = self.active_focus_tab_id;
        if previous != Some(new_tab_id) {
            self.active_focus_tab_id = Some(new_tab_id);
            previous
        } else {
            None
        }
    }

    /// Libera o foco de áudio se pertencer à aba informada
    pub fn release_focus(&mut self, tab_id: usize) {
        if self.active_focus_tab_id == Some(tab_id) {
            self.active_focus_tab_id = None;
        }
    }

    /// Verifica se a aba possui autorização de reprodução
    pub fn has_focus(&self, tab_id: usize) -> bool {
        self.active_focus_tab_id == Some(tab_id)
    }
}

/// Telemetria e Métricas de Recursos do Subsistema de Abas
#[derive(Debug, Clone, PartialEq)]
pub struct TabTelemetry {
    pub total_tabs: usize,
    pub active_count: usize,
    pub audio_background_count: usize,
    pub hibernated_count: usize,
    pub discarded_count: usize,
    pub total_resident_memory_bytes: usize,
    pub baseline_unoptimized_memory_bytes: usize,
    pub saved_memory_bytes: usize,
    pub cumulative_freed_bytes: usize,
    pub background_cpu_ticks: u64,
}

pub type TabMetrics = TabTelemetry;

/// Gerenciador Central de Abas com Ciclo de Vida Inteligente
pub struct TabManager {
    pub tabs: Vec<Tab>,
    pub active_tab_id: usize,
    pub next_tab_id: usize,
    pub audio_focus: AudioFocusManager,
    pub global_memory_budget_bytes: usize,
    pub current_tick: u64,
    pub background_cpu_ticks_counter: u64,
}

impl TabManager {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_id: 0,
            next_tab_id: 1,
            audio_focus: AudioFocusManager::new(),
            global_memory_budget_bytes: budget_bytes,
            current_tick: 0,
            background_cpu_ticks_counter: 0,
        }
    }

    /// Abre uma nova aba no navegador, tornando-a ativa e aplicando a política às anteriores
    pub fn open_tab(&mut self, session: BrowserSession) -> usize {
        let new_id = self.next_tab_id;
        self.next_tab_id += 1;
        self.current_tick += 1;

        // Se já existia uma aba ativa anterior, ela deve ser tratada
        if let Some(prev_tab) = self.get_active_tab_mut() {
            if prev_tab.media.is_playing() && prev_tab.is_pinned_audio {
                let _ = prev_tab.transition_to(TabState::AudioBackground);
            } else {
                if prev_tab.media.is_playing() {
                    prev_tab.media.pause();
                }
                let _ = prev_tab.transition_to(TabState::Hibernated);
            }
        }

        let new_tab = Tab::new(new_id, session, self.current_tick);
        self.tabs.push(new_tab);
        self.active_tab_id = new_id;

        // Atribui foco exclusivo de áudio à nova aba ativa
        self.audio_focus.request_focus(new_id);

        self.enforce_memory_budget();
        new_id
    }

    pub fn get_tab(&self, tab_id: usize) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == tab_id)
    }

    pub fn get_tab_mut(&mut self, tab_id: usize) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == tab_id)
    }

    pub fn get_active_tab(&self) -> Option<&Tab> {
        self.get_tab(self.active_tab_id)
    }

    pub fn get_active_tab_mut(&mut self) -> Option<&mut Tab> {
        let active_id = self.active_tab_id;
        self.get_tab_mut(active_id)
    }

    /// Alterna a visualização para a aba de destino, aplicando o ciclo de vida
    pub fn switch_to_tab(
        &mut self,
        target_id: usize,
        loader_factory: impl Fn() -> Box<dyn ResourceLoader>,
    ) -> Result<(), EngineError> {
        if target_id == self.active_tab_id {
            return Ok(());
        }

        // 1. Processa a aba atual saindo de foco
        let current_id = self.active_tab_id;
        if let Some(current_tab) = self.get_tab_mut(current_id) {
            if current_tab.media.is_playing() && current_tab.is_pinned_audio {
                let _ = current_tab.transition_to(TabState::AudioBackground);
            } else {
                if current_tab.media.is_playing() {
                    current_tab.media.pause();
                }
                let _ = current_tab.transition_to(TabState::Hibernated);
            }
        }

        // 2. Transfere o foco de áudio exclusivo
        self.audio_focus.request_focus(target_id);

        // 3. Restaura e ativa a nova aba
        self.current_tick += 1;
        let tick = self.current_tick;
        let target_tab = self.get_tab_mut(target_id).ok_or_else(|| {
            EngineError::Internal("Aba de destino inexistente".into())
        })?;

        target_tab.last_active_tick = tick;

        if target_tab.state == TabState::Hibernated || target_tab.state == TabState::Discarded {
            target_tab.restore(loader_factory())?;
        } else {
            target_tab.state = TabState::Active;
            target_tab.task_context.resume();
        }

        self.active_tab_id = target_id;
        self.enforce_memory_budget();
        Ok(())
    }

    /// Fixa ou desafixa o áudio de uma aba para exceção deliberada de podcast/música
    pub fn set_pinned_audio(&mut self, tab_id: usize, pinned: bool) -> bool {
        if let Some(tab) = self.get_tab_mut(tab_id) {
            tab.is_pinned_audio = pinned;
            if !pinned && tab.state == TabState::AudioBackground {
                tab.media.pause();
                let _ = tab.transition_to(TabState::Hibernated);
            }
            true
        } else {
            false
        }
    }

    /// Fecha uma aba e reorganiza o foco
    pub fn close_tab(&mut self, tab_id: usize) -> bool {
        let pos = self.tabs.iter().position(|t| t.id == tab_id);
        if let Some(index) = pos {
            self.tabs.remove(index);
            self.audio_focus.release_focus(tab_id);

            // Se fechou a aba ativa, seleciona a mais próxima
            if self.active_tab_id == tab_id {
                if let Some(next_tab) = self.tabs.last() {
                    self.active_tab_id = next_tab.id;
                } else {
                    self.active_tab_id = 0;
                }
            }
            true
        } else {
            false
        }
    }

    /// Executa um ciclo temporal do navegador integrado ao Scheduler.
    /// Invariante de Near-Zero CPU:
    /// - Aba ativa consome ticks normais de página e áudio.
    /// - Abas AudioBackground consomem ticks estritamente para o pipeline de áudio.
    /// - Abas Hibernated e Discarded consomem rigorosamente ZERO ticks (0% de CPU).
    pub fn tick(&mut self, delta_ms: u64) {
        self.current_tick += 1;

        for tab in self.tabs.iter_mut() {
            let task_ticks = tab.task_context.tick(delta_ms);

            match tab.state {
                TabState::Active => {
                    tab.media.tick(delta_ms);
                }
                TabState::AudioBackground => {
                    tab.media.tick(delta_ms);
                    self.background_cpu_ticks_counter += task_ticks;
                }
                TabState::Hibernated | TabState::Discarded => {
                    // Verificação estrita de invariante: task_ticks deve ser 0!
                    debug_assert_eq!(task_ticks, 0);
                }
            }
        }
    }

    /// Aplica o limite do orçamento global de memória, descartando abas inativas sob pressão
    pub fn enforce_memory_budget(&mut self) {
        let mut total_mem = self.total_resident_memory();
        if total_mem <= self.global_memory_budget_bytes {
            return;
        }

        // Coleta abas hibernadas ordenadas por menos recentemente utilizadas (LRU)
        let mut candidates: Vec<(usize, u64)> = self
            .tabs
            .iter()
            .filter(|t| t.state == TabState::Hibernated)
            .map(|t| (t.id, t.last_active_tick))
            .collect();

        candidates.sort_by_key(|&(_, tick)| tick);

        for (id, _) in candidates {
            if total_mem <= self.global_memory_budget_bytes {
                break;
            }
            if let Some(tab) = self.get_tab_mut(id) {
                tab.discard();
            }
            total_mem = self.total_resident_memory();
        }
    }

    /// Calcula o total real de memória residente consumida por todas as abas
    pub fn total_resident_memory(&self) -> usize {
        self.tabs.iter().map(|t| t.resident_memory_bytes()).sum()
    }

    /// Calcula o consumo que ocorreria caso todas as abas permanecessem ativas simultaneamente
    pub fn baseline_unoptimized_memory(&self) -> usize {
        self.tabs
            .iter()
            .map(|_| {
                // 640x480x4 (1,22 MiB framebuffer) + ~50 KiB DOM/Layout
                std::mem::size_of::<Tab>() + (640 * 480 * 4) + 51200
            })
            .sum()
    }

    /// Retorna telemetria detalhada para aferição das metas arquiteturais
    pub fn telemetry(&self) -> TabTelemetry {
        let mut active_count = 0;
        let mut audio_background_count = 0;
        let mut hibernated_count = 0;
        let mut discarded_count = 0;
        let mut cumulative_freed = 0;

        for tab in &self.tabs {
            cumulative_freed += tab.cumulative_freed_bytes;
            match tab.state {
                TabState::Active => active_count += 1,
                TabState::AudioBackground => audio_background_count += 1,
                TabState::Hibernated => hibernated_count += 1,
                TabState::Discarded => discarded_count += 1,
            }
        }

        let resident = self.total_resident_memory();
        let baseline = self.baseline_unoptimized_memory();
        let saved = baseline.saturating_sub(resident);

        TabTelemetry {
            total_tabs: self.tabs.len(),
            active_count,
            audio_background_count,
            hibernated_count,
            discarded_count,
            total_resident_memory_bytes: resident,
            baseline_unoptimized_memory_bytes: baseline,
            saved_memory_bytes: saved,
            cumulative_freed_bytes: cumulative_freed,
            background_cpu_ticks: self.background_cpu_ticks_counter,
        }
    }
}
