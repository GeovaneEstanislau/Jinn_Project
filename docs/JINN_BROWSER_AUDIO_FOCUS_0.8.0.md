# JINN BROWSER 0.8.0 — AUDIO FOCUS AND SUBSYSTEM AUDIT

**Documento:** `docs/JINN_BROWSER_AUDIO_FOCUS_0.8.0.md`  
**Classificação:** Especificação Técnica de Áudio e Declaração de Limitações  
**Versão:** 0.8.0  
**Data:** 05/11/2026  
**Status:** Governança Lógica Implementada [V] / Driver Físico Ring 0 Ausente [X]

---

## 1. Declaração Obrigatória de Transparência Técnica

> [!IMPORTANT]
> **Ausência de Driver de Áudio Físico no Kernel Jinn:**  
> O kernel Jinn atualmente **NÃO POSSUI** drivers de hardware físico para controladores de áudio como **Intel HDA (High Definition Audio)**, **AC97** ou **Sound Blaster 16** em Ring 0.
> 
> Consequentemente:
> 1. O Jinn Browser 0.8.0 **NÃO emite som audível** pelos alto-falantes de máquinas físicas ou do emulador QEMU.
> 2. O pipeline de áudio implementado no navegador opera no espaço de usuário como uma **abstração lógica e de governança de buffers PCM**.
> 3. Declarar que a reprodução de mídia em segundo plano está 100% concluída em nível de hardware seria incorreto. O subsistema está concluído estritamente no nível de **governança de foco e ciclo de vida de memória**.

---

## 2. O Gerenciador de Foco Exclusivo (`AudioFocusManager`)

Em sistemas operacionais modernos, múltiplas abas concorrendo para emitir áudio causam sobreposição sonora indesejada e degradação de recursos.

O Jinn Browser adota uma política rigorosa de **Foco Exclusivo de Áudio**:
- No máximo **uma única aba** em todo o sistema pode deter autorização para preencher buffers de áudio.
- A requisição de foco por uma nova aba revoga imediatamente a autorização da aba detentora anterior.
- A perda de foco transiciona a aba anterior de `AUDIO_BACKGROUND` para `BACKGROUND` ou `HIBERNATED`.

### Implementação:
```rust
pub struct AudioFocusManager {
    current_focus: Option<u64>,
}

impl AudioFocusManager {
    pub fn request_focus(&mut self, tab_id: u64) -> bool {
        self.current_focus = Some(tab_id);
        true
    }
    
    pub fn release_focus(&mut self, tab_id: u64) {
        if self.current_focus == Some(tab_id) {
            self.current_focus = None;
        }
    }
    
    pub fn has_focus(&self, tab_id: u64) -> bool {
        self.current_focus == Some(tab_id)
    }
}
```

---

## 3. Gestão e Desalocação do Buffer PCM

Uma vulnerabilidade crítica identificada no ciclo de vida anterior era a retenção de buffers de áudio alocados mesmo quando uma aba entrava em hibernação.

No Jinn Browser 0.8.0:
1. Quando uma aba está ativa ou em `AUDIO_BACKGROUND`, seu buffer PCM retém até 32 KiB de amostras brutas ($44,1 \text{ kHz}$, 16-bit estéreo):
   ```rust
   pub struct AudioStreamContext {
       pub sample_rate: u32,
       pub channels: u8,
       pub pcm_ring_buffer: Vec<u8>,
   }
   ```
2. Ao perder o foco ou sofrer hibernação forçada:
   ```rust
   // Desalocação mandatória:
   self.audio_context.pcm_ring_buffer.clear();
   self.audio_context.pcm_ring_buffer.shrink_to_fit();
   ```
3. Essa desalocação reduz o consumo do contexto de áudio de 32 KiB para **0 bytes residentes**.

---

## 4. Roadmap para Integração com Hardware Físico

Para alcançar a emissão física de áudio em versões futuras:

1. **Fase I (Driver Ring 0 Intel HDA / AC97):**
   - Criação de `src/drivers/audio/intel_hda.rs`.
   - Inicialização do barramento PCI (Vendor ID `0x8086`, Device ID `0x2668`).
   - Configuração de DMA e descritores de buffer (*Buffer Descriptor List*).
2. **Fase II (Serviço de Mixagem de Áudio em Ring 3):**
   - Servidor `aladdin-soundd` para receber streams PCM via IPC e mixar os canais antes de enviar ao Ring 0.
3. **Fase III (Syscalls de Áudio):**
   - `sys_audio_write(buffer: *const u8, len: usize) -> i64`.
   - `sys_audio_get_status() -> AudioStatus`.

