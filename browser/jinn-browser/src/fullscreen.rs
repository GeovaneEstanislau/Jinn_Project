//! Fullscreen Mode State Machine & Dynamic Window Geometry (v0.8.0)
//!
//! Implementação formal da máquina de estados de tela cheia para o Jinn Browser:
//! - Estados: Windowed, EnteringFullscreen, Fullscreen, ExitingFullscreen, ResizePending, Recovery.
//! - Transições determinísticas e reversíveis.
//! - Preservação de geometria anterior (saved_width, saved_height).
//! - Limites rigorosos de dimensão (mínimo 100x60, máximo 1024x768).
//! - Recuperação automática de falhas sem corrupção de estado.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenState {
    /// Modo janela padrão com dimensões controladas pelo Window Manager.
    Windowed,
    /// Transição em andamento: alocando buffers e preparando nova geometria.
    EnteringFullscreen,
    /// Modo tela cheia ativo ocupando toda a resolução disponível.
    Fullscreen,
    /// Transição de retorno: restaurando geometria anterior e desalocando superfícies extras.
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
            max_width: 1024,
            max_height: 768,
        }
    }

    pub fn is_valid(&self, w: usize, h: usize) -> bool {
        w >= self.min_width && w <= self.max_width && h >= self.min_height && h <= self.max_height
    }
}

/// Controlador formal de transição de tela cheia
#[derive(Debug, Clone)]
pub struct FullscreenController {
    pub state: FullscreenState,
    pub geometry: WindowGeometry,
    pub transition_count: usize,
    pub last_error: Option<FullscreenTransitionError>,
}

impl FullscreenController {
    pub fn new(initial_w: usize, initial_h: usize) -> Self {
        Self {
            state: FullscreenState::Windowed,
            geometry: WindowGeometry::new(initial_w, initial_h),
            transition_count: 0,
            last_error: None,
        }
    }

    /// Solicita transição para Tela Cheia
    pub fn request_fullscreen(&mut self, screen_w: usize, screen_h: usize) -> Result<(), FullscreenTransitionError> {
        if self.state != FullscreenState::Windowed {
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

        // 1. Salva geometria da janela
        self.geometry.saved_width = self.geometry.width;
        self.geometry.saved_height = self.geometry.height;

        // 2. Transição para EnteringFullscreen
        self.state = FullscreenState::EnteringFullscreen;

        // 3. Aplica novas dimensões
        self.geometry.width = screen_w;
        self.geometry.height = screen_h;

        // 4. Confirma Fullscreen
        self.state = FullscreenState::Fullscreen;
        self.transition_count += 1;
        self.last_error = None;
        Ok(())
    }

    /// Solicita saída do modo Tela Cheia e restauração da geometria anterior
    pub fn exit_fullscreen(&mut self) -> Result<(), FullscreenTransitionError> {
        if self.state != FullscreenState::Fullscreen {
            let err = FullscreenTransitionError::InvalidTransition {
                from: self.state,
                to: FullscreenState::ExitingFullscreen,
            };
            self.last_error = Some(err.clone());
            return Err(err);
        }

        self.state = FullscreenState::ExitingFullscreen;

        // Restaura dimensões salvas
        self.geometry.width = self.geometry.saved_width;
        self.geometry.height = self.geometry.saved_height;

        self.state = FullscreenState::Windowed;
        self.transition_count += 1;
        self.last_error = None;
        Ok(())
    }

    /// Alterna entre Janela e Tela Cheia de forma atômica
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

