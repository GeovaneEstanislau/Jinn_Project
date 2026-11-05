//! Fullscreen Mode State Machine & Dynamic Window Geometry (v0.8.1)
//!
//! Implementação formal e endurecida da máquina de estados de tela cheia para o Jinn Browser:
//! - Estados: Windowed, EnteringFullscreen, Fullscreen, ExitingFullscreen, ResizePending, Recovery.
//! - Transições determinísticas, bidirecionais e com suporte a Two-Phase Commit assíncrono.
//! - Preservação estrita de geometria original (saved_width, saved_height).
//! - Limites rigorosos de dimensão (mínimo 100x60, máximo 1920x1080).
//! - Recuperação automática de falhas sem corrupção de estado ou vazamento de buffers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenState {
    /// Modo janela padrão com dimensões controladas pelo Window Manager.
    Windowed,
    /// Transição em andamento (Fase 1): buffers em alocação / aguardando sincronização do compositor.
    EnteringFullscreen,
    /// Modo tela cheia confirmado ocupando todo o viewport do display.
    Fullscreen,
    /// Transição de retorno (Fase 1): restaurando geometria anterior e liberando superfícies extras.
    ExitingFullscreen,
    /// Redimensionamento pendente aguardando sincronização com o compositor.
    ResizePending,
    /// Estado de recuperação após falha de alocação ou dimensão inválida.
    Recovery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullscreenTransitionError {
    InvalidTransition { from: FullscreenState, to: FullscreenState },
    AllocationFailed,
    InvalidDimensions { width: usize, height: usize },
    TimeoutOrAborted,
}

/// Geometria e limites da janela e do viewport
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGeometry {
    pub width: usize,
    pub height: usize,
    pub saved_width: usize,
    pub saved_height: usize,
    pub min_width: usize,
    pub min_height: usize,
    pub max_width: usize,
    pub max_height: usize,
}

impl WindowGeometry {
    pub const fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            saved_width: width,
            saved_height: height,
            min_width: 100,
            min_height: 60,
            max_width: 1920,
            max_height: 1080,
        }
    }

    pub fn is_valid(&self, w: usize, h: usize) -> bool {
        w >= self.min_width && w <= self.max_width && h >= self.min_height && h <= self.max_height
    }

    pub fn pixel_count(&self) -> usize {
        self.width.saturating_mul(self.height)
    }

    pub fn byte_size(&self) -> usize {
        self.pixel_count().saturating_mul(4)
    }
}

/// Controlador formal e endurecido de transição de tela cheia
#[derive(Debug, Clone)]
pub struct FullscreenController {
    pub state: FullscreenState,
    pub geometry: WindowGeometry,
    pub transition_count: usize,
    pub rollback_count: usize,
    pub last_error: Option<FullscreenTransitionError>,
}

impl FullscreenController {
    pub fn new(initial_w: usize, initial_h: usize) -> Self {
        Self {
            state: FullscreenState::Windowed,
            geometry: WindowGeometry::new(initial_w, initial_h),
            transition_count: 0,
            rollback_count: 0,
            last_error: None,
        }
    }

    /// Valida se uma transição de estado é formalmente permitida
    pub fn can_transition(from: FullscreenState, to: FullscreenState) -> bool {
        match (from, to) {
            // Transição normal para fullscreen (Two-Phase Commit ou direta)
            (FullscreenState::Windowed, FullscreenState::EnteringFullscreen) => true,
            (FullscreenState::EnteringFullscreen, FullscreenState::Fullscreen) => true,
            (FullscreenState::EnteringFullscreen, FullscreenState::Windowed) => true, // Abort / Rollback
            (FullscreenState::EnteringFullscreen, FullscreenState::Recovery) => true, // Falha de alocação

            // Transição normal de saída (Two-Phase Commit ou direta)
            (FullscreenState::Fullscreen, FullscreenState::ExitingFullscreen) => true,
            (FullscreenState::ExitingFullscreen, FullscreenState::Windowed) => true,
            (FullscreenState::ExitingFullscreen, FullscreenState::Fullscreen) => true, // Abort / Rollback
            (FullscreenState::ExitingFullscreen, FullscreenState::Recovery) => true,

            // Redimensionamento concorrente durante fullscreen
            (FullscreenState::Fullscreen, FullscreenState::ResizePending) => true,
            (FullscreenState::ResizePending, FullscreenState::Fullscreen) => true,
            (FullscreenState::ResizePending, FullscreenState::Windowed) => true,

            // Recuperação e fallback seguro
            (FullscreenState::Recovery, FullscreenState::Windowed) => true,
            (_, FullscreenState::Recovery) => true,

            _ => false,
        }
    }

    /// Inicia a Fase 1 da transição para Tela Cheia (EnteringFullscreen)
    pub fn begin_request_fullscreen(&mut self, screen_w: usize, screen_h: usize) -> Result<(), FullscreenTransitionError> {
        if !Self::can_transition(self.state, FullscreenState::EnteringFullscreen) {
            let err = FullscreenTransitionError::InvalidTransition {
                from: self.state,
                to: FullscreenState::EnteringFullscreen,
            };
            self.last_error = Some(err.clone());
            return Err(err);
        }

        if !self.geometry.is_valid(screen_w, screen_h) {
            let err = FullscreenTransitionError::InvalidDimensions {
                width: screen_w,
                height: screen_h,
            };
            self.last_error = Some(err.clone());
            self.state = FullscreenState::Recovery;
            return Err(err);
        }

        // 1. Salva geometria anterior da janela
        self.geometry.saved_width = self.geometry.width;
        self.geometry.saved_height = self.geometry.height;

        // 2. Transição para EnteringFullscreen
        self.state = FullscreenState::EnteringFullscreen;
        self.geometry.width = screen_w;
        self.geometry.height = screen_h;
        self.last_error = None;
        Ok(())
    }

    /// Confirma (Commit) a transição para Fullscreen após alocação bem-sucedida de buffers
    pub fn commit_fullscreen(&mut self) -> Result<(), FullscreenTransitionError> {
        if self.state != FullscreenState::EnteringFullscreen {
            let err = FullscreenTransitionError::InvalidTransition {
                from: self.state,
                to: FullscreenState::Fullscreen,
            };
            self.last_error = Some(err.clone());
            return Err(err);
        }

        self.state = FullscreenState::Fullscreen;
        self.transition_count += 1;
        self.last_error = None;
        Ok(())
    }

    /// Aborta a transição para Fullscreen caso a alocação de buffer ou compositor falhe (Rollback)
    pub fn abort_fullscreen(&mut self) {
        if self.state == FullscreenState::EnteringFullscreen {
            self.geometry.width = self.geometry.saved_width;
            self.geometry.height = self.geometry.saved_height;
            self.state = FullscreenState::Windowed;
            self.rollback_count += 1;
            self.last_error = Some(FullscreenTransitionError::AllocationFailed);
        }
    }

    /// Solicita saída do modo Tela Cheia (Fase 1: ExitingFullscreen)
    pub fn begin_exit_fullscreen(&mut self) -> Result<(), FullscreenTransitionError> {
        if !Self::can_transition(self.state, FullscreenState::ExitingFullscreen) {
            let err = FullscreenTransitionError::InvalidTransition {
                from: self.state,
                to: FullscreenState::ExitingFullscreen,
            };
            self.last_error = Some(err.clone());
            return Err(err);
        }

        self.state = FullscreenState::ExitingFullscreen;
        self.last_error = None;
        Ok(())
    }

    /// Confirma a saída e restaura a geometria salva da janela
    pub fn commit_exit_fullscreen(&mut self) -> Result<(), FullscreenTransitionError> {
        if self.state != FullscreenState::ExitingFullscreen {
            let err = FullscreenTransitionError::InvalidTransition {
                from: self.state,
                to: FullscreenState::Windowed,
            };
            self.last_error = Some(err.clone());
            return Err(err);
        }

        self.geometry.width = self.geometry.saved_width;
        self.geometry.height = self.geometry.saved_height;
        self.state = FullscreenState::Windowed;
        self.transition_count += 1;
        self.last_error = None;
        Ok(())
    }

    /// Transição síncrona completa para compatibilidade direta
    pub fn request_fullscreen(&mut self, screen_w: usize, screen_h: usize) -> Result<(), FullscreenTransitionError> {
        self.begin_request_fullscreen(screen_w, screen_h)?;
        self.commit_fullscreen()
    }

    /// Saída síncrona completa para compatibilidade direta
    pub fn exit_fullscreen(&mut self) -> Result<(), FullscreenTransitionError> {
        self.begin_exit_fullscreen()?;
        self.commit_exit_fullscreen()
    }

    /// Alterna entre Janela e Tela Cheia de forma atômica e resiliente
    pub fn toggle_fullscreen(&mut self, screen_w: usize, screen_h: usize) -> Result<FullscreenState, FullscreenTransitionError> {
        match self.state {
            FullscreenState::Windowed => {
                self.request_fullscreen(screen_w, screen_h)?;
                Ok(FullscreenState::Fullscreen)
            }
            FullscreenState::Fullscreen => {
                self.exit_fullscreen()?;
                Ok(FullscreenState::Windowed)
            }
            FullscreenState::Recovery => {
                self.recover();
                Ok(FullscreenState::Windowed)
            }
            other => {
                let err = FullscreenTransitionError::InvalidTransition {
                    from: other,
                    to: FullscreenState::EnteringFullscreen,
                };
                self.last_error = Some(err.clone());
                Err(err)
            }
        }
    }

    /// Trata evento de redimensionamento da janela vindo do compositor
    pub fn handle_resize(&mut self, new_w: usize, new_h: usize) -> Result<(), FullscreenTransitionError> {
        if !self.geometry.is_valid(new_w, new_h) {
            let err = FullscreenTransitionError::InvalidDimensions {
                width: new_w,
                height: new_h,
            };
            self.last_error = Some(err.clone());
            return Err(err);
        }

        self.geometry.width = new_w;
        self.geometry.height = new_h;
        if self.state == FullscreenState::Fullscreen && (new_w != self.geometry.max_width || new_h != self.geometry.max_height) {
            self.state = FullscreenState::Windowed;
        }
        Ok(())
    }

    /// Recuperação de emergência após erro inesperado ou dimensões corrompidas
    pub fn recover(&mut self) {
        self.geometry.width = self.geometry.saved_width.max(300);
        self.geometry.height = self.geometry.saved_height.max(180);
        self.state = FullscreenState::Windowed;
        self.last_error = None;
    }

    pub fn is_fullscreen(&self) -> bool {
        self.state == FullscreenState::Fullscreen
    }
}
