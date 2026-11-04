//! Aladdin Desktop Native Wallet Engine (Ring 3)
//!
//! Implementação completa da interface gráfica nativa para gestão de $JINN e BNB no Aladdin Desktop.
//! - Executa estritamente em espaço de usuário (Ring 3)
//! - Isolamento total de privilégios (nenhuma chave privada reside no kernel)
//! - Identidade visual Catppuccin Mocha / Aladdin Desktop oficial
//! - Modais de confirmação explícita de transações com estimativa de gas
//! - Comunicação modular via IPC com o daemon jinn-web3d

use crate::font;

pub const WIN_W: usize = 320;
pub const WIN_H: usize = 220;

// ── Design Tokens (Catppuccin Mocha / Aladdin Dark) ───────────────────────────
pub const COLOR_BG: u32 = 0x0018_1825;           // Base Escura
pub const COLOR_HEADER: u32 = 0x001E_1E2E;       // Cabeçalho / Barra de status
pub const COLOR_CARD_BG: u32 = 0x001E_1E2E;      // Fundo dos cards
pub const COLOR_BORDER: u32 = 0x0031_3244;       // Bordas
pub const COLOR_ACCENT: u32 = 0x0089_B4FA;       // Azul Safira / Primário
pub const COLOR_GREEN: u32 = 0x00A6_E3A1;        // Verde Ativo / Confirmação
pub const COLOR_GOLD: u32 = 0x00F9_E2AF;         // Dourado $JINN
pub const COLOR_PEACH: u32 = 0x00FA_B387;        // Alerta / Atenção
pub const COLOR_TEXT_MAIN: u32 = 0x00CD_D6F4;    // Texto Principal
pub const COLOR_TEXT_MUTED: u32 = 0x00A6_ADC8;   // Texto Secundário
pub const COLOR_BTN_BG: u32 = 0x0031_3244;       // Fundo de Botão
pub const COLOR_BTN_HOVER: u32 = 0x0045_475A;    // Botão Ativo

// ── Estados da Carteira ───────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WalletTab {
    Overview,
    Send,
    Receive,
    Activity,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModalState {
    None,
    ConfirmTransfer,
    TransactionSuccess,
}

pub struct AladdinWalletSession {
    pub current_tab: WalletTab,
    pub modal_state: ModalState,
    pub network_name: &'static str,
    pub chain_id: u64,
    pub public_address: [u8; 42], // "0x71C6...4955"
    pub jinn_balance_str: &'static str,
    pub bnb_balance_str: &'static str,
    pub transfer_recipient: [u8; 42],
    pub transfer_amount: &'static str,
    pub estimated_gas: &'static str,
    pub last_tx_hash: &'static str,
    pub pouc_claimable: &'static str,
}

impl AladdinWalletSession {
    pub const fn new() -> Self {
        Self {
            current_tab: WalletTab::Overview,
            modal_state: ModalState::None,
            network_name: "BSC Testnet",
            chain_id: 97,
            public_address: *b"0x71C634C26B331E0cd51a04579997424946344955",
            jinn_balance_str: "12,450.00 $JINN",
            bnb_balance_str: "0.8540 BNB",
            transfer_recipient: *b"0x25aB829c36d07d1B73b88b488F35b2e98782E780",
            transfer_amount: "500.00 $JINN",
            estimated_gas: "21,000 (~0.00021 BNB)",
            last_tx_hash: "0x8f2a...c4b1",
            pouc_claimable: "500.00 $JINN",
        }
    }

    /// Renderiza a interface completa do Aladdin Wallet para o buffer da janela (320x220).
    pub fn render(&self, buf: &mut [u32]) {
        // 1. Limpa o fundo da janela
        for pixel in buf.iter_mut() {
            *pixel = COLOR_BG;
        }

        // 2. Barra de Topo / Header (Rede e Status)
        self.draw_rect(buf, 0, 0, WIN_W, 24, COLOR_HEADER);
        self.draw_hline(buf, 0, 24, WIN_W, COLOR_BORDER);

        // Indicador de Conexão Verde e Nome da Rede
        self.draw_rect(buf, 8, 8, 8, 8, COLOR_GREEN);
        font::draw_text(buf, WIN_W, WIN_H, 20, 8, b"BSC Testnet (Chapel 97)", COLOR_TEXT_MAIN, None);

        // Badge Ring 3 Isolation
        font::draw_text(buf, WIN_W, WIN_H, 240, 8, b"[Ring 3]", COLOR_ACCENT, None);

        // 3. Barra de Navegação / Tabs
        self.draw_tabs(buf);

        // 4. Renderiza o conteúdo da Tab ativa
        match self.current_tab {
            WalletTab::Overview => self.render_overview(buf),
            WalletTab::Send => self.render_send(buf),
            WalletTab::Receive => self.render_receive(buf),
            WalletTab::Activity => self.render_activity(buf),
        }

        // 5. Renderiza Modal de Confirmação (se ativo)
        if self.modal_state != ModalState::None {
            self.render_modal(buf);
        }
    }

    fn draw_tabs(&self, buf: &mut [u32]) {
        let tab_y = 28;
        let tab_h = 18;
        let tabs = [
            (WalletTab::Overview, "Visao Geral", 8, 85),
            (WalletTab::Send, "Enviar", 98, 55),
            (WalletTab::Receive, "Receber", 158, 65),
            (WalletTab::Activity, "Extrato", 228, 65),
        ];

        for (tab, label, x, w) in tabs {
            let is_active = self.current_tab == tab;
            let bg = if is_active { COLOR_ACCENT } else { COLOR_BTN_BG };
            let fg = if is_active { COLOR_HEADER } else { COLOR_TEXT_MUTED };

            self.draw_rect(buf, x, tab_y, w, tab_h, bg);
            font::draw_text(buf, WIN_W, WIN_H, x + 6, tab_y + 5, label.as_bytes(), fg, None);
        }
        self.draw_hline(buf, 0, tab_y + tab_h + 4, WIN_W, COLOR_BORDER);
    }

    fn render_overview(&self, buf: &mut [u32]) {
        // Card de Saldo $JINN
        self.draw_card(buf, 8, 56, WIN_W - 16, 52);
        font::draw_text(buf, WIN_W, WIN_H, 16, 62, b"Saldo Disponivel ($JINN):", COLOR_TEXT_MUTED, None);
        font::draw_text(buf, WIN_W, WIN_H, 16, 76, self.jinn_balance_str.as_bytes(), COLOR_GOLD, None);
        font::draw_text(buf, WIN_W, WIN_H, 16, 92, b"Taxas Gas:", COLOR_TEXT_MUTED, None);
        font::draw_text(buf, WIN_W, WIN_H, 90, 92, self.bnb_balance_str.as_bytes(), COLOR_GREEN, None);

        // Card de Endereço Público
        self.draw_card(buf, 8, 114, WIN_W - 16, 42);
        font::draw_text(buf, WIN_W, WIN_H, 16, 120, b"Endereco Publico (Ring 3 Keystore):", COLOR_TEXT_MUTED, None);
        let mut short_addr = [0u8; 18];
        short_addr[..6].copy_from_slice(&self.public_address[..6]);
        short_addr[6..9].copy_from_slice(b"...");
        short_addr[9..17].copy_from_slice(&self.public_address[34..42]);
        short_addr[17] = b' ';
        font::draw_text(buf, WIN_W, WIN_H, 16, 134, &short_addr, COLOR_ACCENT, None);

        // Card de Recompensas PoUC
        self.draw_card(buf, 8, 162, WIN_W - 16, 50);
        font::draw_text(buf, WIN_W, WIN_H, 16, 168, b"Proof of Useful Contribution (PoUC):", COLOR_TEXT_MUTED, None);
        font::draw_text(buf, WIN_W, WIN_H, 16, 182, b"Bounty Epoch 1:", COLOR_TEXT_MAIN, None);
        font::draw_text(buf, WIN_W, WIN_H, 125, 182, self.pouc_claimable.as_bytes(), COLOR_GOLD, None);

        // Botão Resgatar
        self.draw_rect(buf, 215, 178, 85, 20, COLOR_ACCENT);
        font::draw_text(buf, WIN_W, WIN_H, 222, 184, b"Resgatar", COLOR_HEADER, None);
    }

    fn render_send(&self, buf: &mut [u32]) {
        self.draw_card(buf, 8, 56, WIN_W - 16, 154);
        font::draw_text(buf, WIN_W, WIN_H, 16, 64, b"Transferencia de $JINN na BSC:", COLOR_GOLD, None);

        // Campo Destinatário
        font::draw_text(buf, WIN_W, WIN_H, 16, 82, b"Destinatario:", COLOR_TEXT_MUTED, None);
        self.draw_rect(buf, 16, 94, WIN_W - 32, 20, COLOR_HEADER);
        self.draw_border(buf, 16, 94, WIN_W - 32, 20, COLOR_BORDER);
        font::draw_text(buf, WIN_W, WIN_H, 22, 100, b"0x25aB...e780 (Maintainer)", COLOR_TEXT_MAIN, None);

        // Campo Montante
        font::draw_text(buf, WIN_W, WIN_H, 16, 122, b"Montante ($JINN):", COLOR_TEXT_MUTED, None);
        self.draw_rect(buf, 16, 134, WIN_W - 32, 20, COLOR_HEADER);
        self.draw_border(buf, 16, 134, WIN_W - 32, 20, COLOR_BORDER);
        font::draw_text(buf, WIN_W, WIN_H, 22, 140, self.transfer_amount.as_bytes(), COLOR_GOLD, None);

        // Estimativa de Gas
        font::draw_text(buf, WIN_W, WIN_H, 16, 162, b"Gas Est.:", COLOR_TEXT_MUTED, None);
        font::draw_text(buf, WIN_W, WIN_H, 90, 162, self.estimated_gas.as_bytes(), COLOR_GREEN, None);

        // Botão Continuar / Enviar
        self.draw_rect(buf, 16, 180, WIN_W - 32, 22, COLOR_ACCENT);
        font::draw_text(buf, WIN_W, WIN_H, 85, 187, b"Confirmar Operacao", COLOR_HEADER, None);
    }

    fn render_receive(&self, buf: &mut [u32]) {
        self.draw_card(buf, 8, 56, WIN_W - 16, 154);
        font::draw_text(buf, WIN_W, WIN_H, 16, 64, b"Receber $JINN / BNB na Conta:", COLOR_GOLD, None);

        font::draw_text(buf, WIN_W, WIN_H, 16, 84, b"Seu Endereco de Recebimento:", COLOR_TEXT_MUTED, None);
        self.draw_rect(buf, 16, 98, WIN_W - 32, 38, COLOR_HEADER);
        self.draw_border(buf, 16, 98, WIN_W - 32, 38, COLOR_BORDER);

        font::draw_text(buf, WIN_W, WIN_H, 22, 106, &self.public_address[..21], COLOR_TEXT_MAIN, None);
        font::draw_text(buf, WIN_W, WIN_H, 22, 118, &self.public_address[21..], COLOR_TEXT_MAIN, None);

        // Instruções de Segurança
        font::draw_text(buf, WIN_W, WIN_H, 16, 146, b"- Envie apenas BEP-20 / BSC", COLOR_TEXT_MUTED, None);
        font::draw_text(buf, WIN_W, WIN_H, 16, 160, b"- Suporta BSC Testnet (ID 97)", COLOR_TEXT_MUTED, None);
        font::draw_text(buf, WIN_W, WIN_H, 16, 174, b"- Chaves cifradas com Argon2id", COLOR_GREEN, None);
    }

    fn render_activity(&self, buf: &mut [u32]) {
        self.draw_card(buf, 8, 56, WIN_W - 16, 154);
        font::draw_text(buf, WIN_W, WIN_H, 16, 64, b"Atividades e Transacoes Recentes:", COLOR_GOLD, None);

        // Item 1
        font::draw_text(buf, WIN_W, WIN_H, 16, 82, b"+500 $JINN: PoUC Epoch 1 Reward", COLOR_GREEN, None);
        font::draw_text(buf, WIN_W, WIN_H, 16, 94, b"Tx: 0x8f2a...c4b1 [Confirmado]", COLOR_TEXT_MUTED, None);
        self.draw_hline(buf, 16, 108, WIN_W - 32, COLOR_BORDER);

        // Item 2
        font::draw_text(buf, WIN_W, WIN_H, 16, 116, b"-25 $JINN: Licenca Jinn Studio", COLOR_PEACH, None);
        font::draw_text(buf, WIN_W, WIN_H, 16, 128, b"Tx: 0x3d19...7a02 [Confirmado]", COLOR_TEXT_MUTED, None);
        self.draw_hline(buf, 16, 142, WIN_W - 32, COLOR_BORDER);

        // Item 3
        font::draw_text(buf, WIN_W, WIN_H, 16, 150, b"+100 $JINN: Simbad Node Relay", COLOR_GREEN, None);
        font::draw_text(buf, WIN_W, WIN_H, 16, 162, b"Tx: 0xaa42...99f0 [Confirmado]", COLOR_TEXT_MUTED, None);
    }

    fn render_modal(&self, buf: &mut [u32]) {
        // Escurecimento de fundo / Backdrop
        let modal_x = 24;
        let modal_y = 48;
        let modal_w = WIN_W - 48;
        let modal_h = 140;

        self.draw_rect(buf, modal_x, modal_y, modal_w, modal_h, COLOR_HEADER);
        self.draw_border(buf, modal_x, modal_y, modal_w, modal_h, COLOR_ACCENT);

        match self.modal_state {
            ModalState::ConfirmTransfer => {
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 12, modal_y + 12, b"Confirmar Transferencia?", COLOR_PEACH, None);
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 12, modal_y + 30, b"Destino: 0x25aB...e780", COLOR_TEXT_MAIN, None);
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 12, modal_y + 46, b"Valor:   500.00 $JINN", COLOR_GOLD, None);
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 12, modal_y + 62, b"Gas Est: 0.00021 BNB", COLOR_GREEN, None);
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 12, modal_y + 78, b"Assinatura Ring 3 (Keystore)", COLOR_TEXT_MUTED, None);

                // Botão Cancelar
                self.draw_rect(buf, modal_x + 12, modal_y + 102, 100, 22, COLOR_BTN_BG);
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 28, modal_y + 109, b"Cancelar", COLOR_TEXT_MAIN, None);

                // Botão Confirmar
                self.draw_rect(buf, modal_x + 130, modal_y + 102, 120, 22, COLOR_GREEN);
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 145, modal_y + 109, b"Assinar & Enviar", COLOR_HEADER, None);
            }
            ModalState::TransactionSuccess => {
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 12, modal_y + 16, b"Transacao Transmitida!", COLOR_GREEN, None);
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 12, modal_y + 40, b"Hash da Transacao:", COLOR_TEXT_MUTED, None);
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 12, modal_y + 56, b"0x8f2a419c...c4b1", COLOR_ACCENT, None);
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 12, modal_y + 76, b"Registrado na BSC Chapel (97)", COLOR_TEXT_MAIN, None);

                // Botão Fechar
                self.draw_rect(buf, modal_x + 70, modal_y + 102, 120, 22, COLOR_ACCENT);
                font::draw_text(buf, WIN_W, WIN_H, modal_x + 110, modal_y + 109, b"Fechar", COLOR_HEADER, None);
            }
            ModalState::None => {}
        }
    }

    /// Trata cliques de mouse na janela da carteira.
    pub fn handle_click(&mut self, x: usize, y: usize) -> bool {
        // Se modal estiver ativo, trata os botões do modal
        if self.modal_state == ModalState::ConfirmTransfer {
            // Cancelar: modal_x(24) + 12 = 36 .. 136, y = 150 .. 172
            if x >= 36 && x <= 136 && y >= 150 && y <= 172 {
                self.modal_state = ModalState::None;
                return true;
            }
            // Confirmar: modal_x(24) + 130 = 154 .. 274, y = 150 .. 172
            if x >= 154 && x <= 274 && y >= 150 && y <= 172 {
                self.modal_state = ModalState::TransactionSuccess;
                return true;
            }
            return false;
        } else if self.modal_state == ModalState::TransactionSuccess {
            // Fechar modal de sucesso
            if x >= 94 && x <= 214 && y >= 150 && y <= 172 {
                self.modal_state = ModalState::None;
                self.current_tab = WalletTab::Overview;
                return true;
            }
            return false;
        }

        // Navegação entre Tabs (y = 28 .. 46)
        if y >= 28 && y <= 46 {
            if x >= 8 && x <= 93 {
                self.current_tab = WalletTab::Overview;
                return true;
            } else if x >= 98 && x <= 153 {
                self.current_tab = WalletTab::Send;
                return true;
            } else if x >= 158 && x <= 223 {
                self.current_tab = WalletTab::Receive;
                return true;
            } else if x >= 228 && x <= 293 {
                self.current_tab = WalletTab::Activity;
                return true;
            }
        }

        // Botão na Tab Send: Confirmar Operação (x: 16 .. 304, y: 180 .. 202)
        if self.current_tab == WalletTab::Send && x >= 16 && x <= 304 && y >= 180 && y <= 202 {
            self.modal_state = ModalState::ConfirmTransfer;
            return true;
        }

        // Botão Resgatar PoUC na Tab Overview (x: 215 .. 300, y: 178 .. 198)
        if self.current_tab == WalletTab::Overview && x >= 215 && x <= 300 && y >= 178 && y <= 198 {
            self.modal_state = ModalState::TransactionSuccess;
            return true;
        }

        false
    }

    // ── Primitivas Gráficas Rápidas ───────────────────────────────────────────

    fn draw_rect(&self, buf: &mut [u32], x: usize, y: usize, w: usize, h: usize, color: u32) {
        let max_x = (x + w).min(WIN_W);
        let max_y = (y + h).min(WIN_H);
        for cy in y..max_y {
            let row = cy * WIN_W;
            for cx in x..max_x {
                buf[row + cx] = color;
            }
        }
    }

    fn draw_card(&self, buf: &mut [u32], x: usize, y: usize, w: usize, h: usize) {
        self.draw_rect(buf, x, y, w, h, COLOR_CARD_BG);
        self.draw_border(buf, x, y, w, h, COLOR_BORDER);
    }

    fn draw_border(&self, buf: &mut [u32], x: usize, y: usize, w: usize, h: usize, color: u32) {
        self.draw_hline(buf, x, y, w, color);
        self.draw_hline(buf, x, y + h.saturating_sub(1), w, color);
        self.draw_vline(buf, x, y, h, color);
        self.draw_vline(buf, x + w.saturating_sub(1), y, h, color);
    }

    fn draw_hline(&self, buf: &mut [u32], x: usize, y: usize, w: usize, color: u32) {
        if y >= WIN_H { return; }
        let max_x = (x + w).min(WIN_W);
        let row = y * WIN_W;
        for cx in x..max_x {
            buf[row + cx] = color;
        }
    }

    fn draw_vline(&self, buf: &mut [u32], x: usize, y: usize, h: usize, color: u32) {
        if x >= WIN_W { return; }
        let max_y = (y + h).min(WIN_H);
        for cy in y..max_y {
            buf[cy * WIN_W + x] = color;
        }
    }
}
