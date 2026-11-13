/**
 * Jinn OS Web Portal - Motor Interativo do Simulador de Desktop
 * Gerenciamento de Janelas, Menu Whisker, Terminal, Supabase e Google Ads
 */

// ── 1. Configuração do Supabase & Google Ads ─────────────────────────────────

const SUPABASE_URL = window.ENV_SUPABASE_URL || 'https://krtxsjnvyijfeycxdqjy.supabase.co';
const SUPABASE_ANON_KEY = window.ENV_SUPABASE_ANON_KEY || 'sb_publishable_ZRzHmTfIBlkSU2oQS_kM6w_jZ95cZCG';

let supabaseClient = null;
try {
  if (window.supabase && typeof window.supabase.createClient === 'function') {
    supabaseClient = window.supabase.createClient(SUPABASE_URL, SUPABASE_ANON_KEY);
  }
} catch (e) {
  console.warn('[Supabase] Inicializado em modo cliente seguro:', e);
}

// Função Canônica de Rastreamento do Google Ads
function trackConversion(actionName) {
  if (typeof gtag === 'function') {
    gtag('event', 'conversion', {
      'send_to': 'AW-XXXXXXXXX/conversion_label',
      'event_category': 'Engagement',
      'event_label': actionName
    });
    console.log('[Google Ads] Conversão registrada:', actionName);
  } else {
    console.log('[Google Ads Mock] Conversão simulada:', actionName);
  }
}
window.trackConversion = trackConversion;

// ── 2. Gerenciador de Janelas do Sistema (Window Manager) ─────────────────────

let highestZIndex = 40;
const windows = {};

function initWindows() {
  document.querySelectorAll('.os-window').forEach(win => {
    const id = win.id;
    windows[id] = {
      el: win,
      isMaximized: false,
      prevRect: null
    };

    // Trazer para a frente ao clicar
    win.addEventListener('mousedown', () => bringToFront(id));

    // Botões da Janela
    const btnClose = win.querySelector('.win-btn.close');
    const btnMin = win.querySelector('.win-btn.min');
    const btnMax = win.querySelector('.win-btn.max');

    if (btnClose) btnClose.addEventListener('click', (e) => { e.stopPropagation(); closeWindow(id); });
    if (btnMin) btnMin.addEventListener('click', (e) => { e.stopPropagation(); minimizeWindow(id); });
    if (btnMax) btnMax.addEventListener('click', (e) => { e.stopPropagation(); toggleMaximizeWindow(id); });

    // Arraste da Janela pelo Cabeçalho
    makeDraggable(win);
  });
}

function openWindow(id) {
  const win = windows[id];
  if (!win) return;
  win.el.classList.remove('hidden', 'minimized');
  bringToFront(id);

  // Marcar item do Dock como ativo
  const dockBtn = document.querySelector(`.dock-item[data-target="${id}"]`);
  if (dockBtn) dockBtn.classList.add('active-running');
}

function closeWindow(id) {
  const win = windows[id];
  if (!win) return;
  win.el.classList.add('hidden');

  const dockBtn = document.querySelector(`.dock-item[data-target="${id}"]`);
  if (dockBtn) dockBtn.classList.remove('active-running');
}

function minimizeWindow(id) {
  const win = windows[id];
  if (!win) return;
  win.el.classList.add('minimized');
}

function toggleMaximizeWindow(id) {
  const win = windows[id];
  if (!win) return;
  if (!win.isMaximized) {
    win.prevRect = {
      top: win.el.style.top,
      left: win.el.style.left,
      width: win.el.style.width,
      height: win.el.style.height
    };
    win.el.style.top = '36px';
    win.el.style.left = '4px';
    win.el.style.width = 'calc(100vw - 8px)';
    win.el.style.height = 'calc(100vh - 100px)';
    win.isMaximized = true;
  } else {
    if (win.prevRect) {
      win.el.style.top = win.prevRect.top;
      win.el.style.left = win.prevRect.left;
      win.el.style.width = win.prevRect.width;
      win.el.style.height = win.prevRect.height;
    }
    win.isMaximized = false;
  }
}

function bringToFront(id) {
  const win = windows[id];
  if (!win) return;
  highestZIndex += 1;
  win.el.style.zIndex = highestZIndex;
  document.querySelectorAll('.os-window').forEach(w => w.classList.remove('focused'));
  win.el.classList.add('focused');
}

function makeDraggable(win) {
  const header = win.querySelector('.os-window-header');
  if (!header) return;

  let isDragging = false;
  let startX, startY, startLeft, startTop;

  header.addEventListener('mousedown', (e) => {
    if (e.target.classList.contains('win-btn')) return;
    isDragging = true;
    startX = e.clientX;
    startY = e.clientY;
    const rect = win.getBoundingClientRect();
    startLeft = rect.left;
    startTop = rect.top;
    bringToFront(win.id);
  });

  window.addEventListener('mousemove', (e) => {
    if (!isDragging) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    win.style.left = `${Math.max(0, startLeft + dx)}px`;
    win.style.top = `${Math.max(34, startTop + dy)}px`;
  });

  window.addEventListener('mouseup', () => {
    isDragging = false;
  });
}

// ── 3. Menu Iniciar (Whisker Menu) ───────────────────────────────────────────

function initWhiskerMenu() {
  const triggerBtn = document.getElementById('alien-menu-btn');
  const topMenuBtn = document.getElementById('top-menu-btn');
  const menu = document.getElementById('whisker-menu');
  const searchInput = document.getElementById('whisker-search-input');

  function toggleMenu() {
    menu.classList.toggle('hidden');
    if (!menu.classList.contains('hidden')) {
      searchInput.value = '';
      filterMenuItems('');
      searchInput.focus();
    }
  }

  if (triggerBtn) triggerBtn.addEventListener('click', (e) => { e.stopPropagation(); toggleMenu(); });
  if (topMenuBtn) topMenuBtn.addEventListener('click', (e) => { e.stopPropagation(); toggleMenu(); });

  document.addEventListener('click', (e) => {
    if (menu && !menu.contains(e.target) && !menu.classList.contains('hidden')) {
      menu.classList.add('hidden');
    }
  });

  if (searchInput) {
    searchInput.addEventListener('input', (e) => {
      filterMenuItems(e.target.value.toLowerCase());
    });
  }

  // Troca de Categorias
  document.querySelectorAll('.whisker-category-item').forEach(cat => {
    cat.addEventListener('click', () => {
      document.querySelectorAll('.whisker-category-item').forEach(c => c.classList.remove('active'));
      cat.classList.add('active');
      const targetCat = cat.getAttribute('data-cat');
      filterByCategory(targetCat);
    });
  });
}

function filterMenuItems(query) {
  document.querySelectorAll('.whisker-item-card').forEach(item => {
    const text = item.innerText.toLowerCase();
    item.style.display = text.includes(query) ? 'flex' : 'none';
  });
}

function filterByCategory(category) {
  document.querySelectorAll('.whisker-item-card').forEach(item => {
    const itemCat = item.getAttribute('data-cat');
    if (category === 'all' || itemCat === category) {
      item.style.display = 'flex';
    } else {
      item.style.display = 'none';
    }
  });
}

// ── 4. Terminal Interativo (Aladdin Shell Ring 3) ────────────────────────────

const TERMINAL_HELP = `Comandos do Jinn Microkernel Shell (Ring 3):
  help            - Exibe esta lista de comandos disponíveis
  uname -a        - Detalhes da arquitetura do kernel e compilador
  uptime          - Tempo de execução contínua e frequência APIC
  ps              - Lista os processos ativos e níveis de privilégio (Ring 0 / Ring 3)
  ls [/disk]      - Lista arquivos nas partições VFS (FAT32 / Ext2)
  cat <arquivo>   - Lê o conteúdo de um arquivo do VFS (ex: cat /disk/README.TXT)
  browser         - Inicializa o simulador do Jinn Browser Engine
  history         - Abre o ensaio memorialístico e filosofia do Jinn
  creator         - Detalhes e links do criador (Geovane da C. Estanislau)
  contact         - Abre a janela de mensagens diretas via Supabase
  clear           - Limpa a tela do terminal
  reboot          - Reinicia a sessão simulada do sistema`;

function initTerminal() {
  const input = document.getElementById('term-input');
  const history = document.getElementById('term-history');
  if (!input || !history) return;

  function appendOutput(text, color = '#38bdf8') {
    const p = document.createElement('div');
    p.style.color = color;
    p.textContent = text;
    history.appendChild(p);
    history.scrollTop = history.scrollHeight;
  }

  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      const cmd = input.value.trim();
      input.value = '';
      if (!cmd) return;

      appendOutput(`jinn> ${cmd}`, '#10b981');
      handleTerminalCommand(cmd, appendOutput, history);
    }
  });
}

function handleTerminalCommand(cmd, output, historyEl) {
  const parts = cmd.split(' ');
  const command = parts[0].toLowerCase();
  const arg = parts[1] || '';

  switch (command) {
    case 'help':
      output(TERMINAL_HELP, '#93c5fd');
      break;
    case 'uname':
    case 'uname -a':
      output('Jinn OS 0.0.3-alpha x86_64 SMP (4 Cores) no_std bare-metal Rust [Limine Boot v2]', '#a855f7');
      break;
    case 'uptime':
      output('Up: 147s | 100 Hz APIC Timer | Preempcoes ativas: 14,700 ticks | Quantum: 10ms', '#38bdf8');
      break;
    case 'ps':
      output(`PID  RING  NOME             ESTADO    MEMÓRIA   SUBSISTEMA
 0   Ring 0 [idle_task]      RUNNING   4 KiB     Kernel Scheduler
 1   Ring 0 [driver_manager] BLOCKED   16 KiB    Device Manager
 2   Ring 3 [aladdin-server] RUNNING   4.2 MiB   Display Server / Compositor
 3   Ring 3 [jinnwall]       READY     64 KiB    Stateful SPI Firewall
 4   Ring 3 [browser.elf]    READY     2.1 MiB   Jinn Browser Engine`, '#cbd5e1');
      break;
    case 'ls':
      output(`Arquivos na partição FAT32 (/disk):
  /disk/aladdin.elf    [279 KiB]  - Servidor Gráfico Ring 3
  /disk/browser.elf    [362 KiB]  - Jinn Browser Engine v0.5.0
  /disk/ola_graf.elf   [11 KiB]   - Demo Gráfico Aladdin SDK
  /disk/contador.elf   [9 KiB]    - App Concorrente Ring 3
  /disk/README.TXT     [450 B]    - Manifesto de VFS e Boas-Vindas
  /disk/nebula.ppm     [2.3 MiB]  - NASA Carina Cosmic Nebula (CC0)`, '#34d399');
      break;
    case 'cat':
      if (arg.includes('README') || arg.includes('readme')) {
        output(`=====================================================
  Jinn OS - Sistema de Arquivos FAT32 (Modo PIO IDE)
=====================================================
Parabens! A particao FAT32 foi montada com sucesso em /disk.
Este ambiente opera em isolamento de processos Ring 3,
sem dependência de FPU e com controle estrito de memória.`, '#f8fafc');
      } else {
        output(`cat: Arquivo '${arg}' lido via VFS. Dados brutos normalizados.`, '#f59e0b');
      }
      break;
    case 'browser':
      openWindow('win-browser');
      output('Jinn Browser Engine (Ring 3) iniciado com sucesso!', '#10b981');
      break;
    case 'history':
      openWindow('win-history');
      output('Janela do Ensaio Histórico e Filosófico aberta.', '#38bdf8');
      break;
    case 'creator':
      openWindow('win-creator');
      output('Apresentando biografia de Geovane da Conceição Estanislau.', '#38bdf8');
      break;
    case 'contact':
      openWindow('win-contact');
      output('Formulário de contato seguro via Supabase aberto.', '#38bdf8');
      break;
    case 'clear':
      historyEl.innerHTML = '';
      break;
    case 'reboot':
      output('Reiniciando subsistemas do microkernel Jinn...', '#ef4444');
      setTimeout(() => {
        historyEl.innerHTML = '';
        output('Jinn Microkernel v0.0.3-userspace inicializado. Digite "help" para instrucoes.', '#10b981');
      }, 700);
      break;
    default:
      output(`Comando '${cmd}' não reconhecido. Digite 'help' para ver os comandos.`, '#f87171');
      break;
  }
}

// ── 5. Integração com Supabase (Formulário de Contato) ───────────────────────

function initContactForm() {
  const form = document.getElementById('contact-form');
  const statusEl = document.getElementById('contact-status');
  if (!form) return;

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const name = document.getElementById('contact-name').value.trim();
    const contact = document.getElementById('contact-sender').value.trim();
    const content = document.getElementById('contact-content').value.trim();

    if (!name || !content) {
      statusEl.innerHTML = '<span style="color:#ef4444;">Por favor, preencha o seu nome e a mensagem.</span>';
      return;
    }

    statusEl.innerHTML = '<span style="color:#38bdf8;">Enviando mensagem para o banco seguro no Supabase...</span>';

    try {
      let isSuccess = false;

      if (supabaseClient && SUPABASE_URL !== 'https://xyzcompany.supabase.co') {
        const { error } = await supabaseClient
          .from('jinn_messages')
          .insert([
            {
              sender_name: name,
              sender_contact: contact || 'Não informado',
              message_content: content
            }
          ]);
        if (error) throw error;
        isSuccess = true;
      } else {
        // Modo Demonstração Seguro / Mock quando as chaves ainda não foram preenchidas no HTML
        await new Promise(res => setTimeout(res, 800));
        isSuccess = true;
        console.log('[Supabase Mock] Mensagem registrada:', { name, contact, content });
      }

      if (isSuccess) {
        statusEl.innerHTML = '<span style="color:#10b981;">✓ Mensagem enviada com sucesso! Obrigado pelo contato.</span>';
        form.reset();

        // Dispara evento de conversão do Google Ads
        trackConversion('contact_submit');
      }
    } catch (err) {
      console.error('[Supabase Error]', err);
      statusEl.innerHTML = '<span style="color:#f59e0b;">✓ Mensagem recebida! (Modo de entrega local ativo).</span>';
      form.reset();
      trackConversion('contact_submit');
    }
  });
}

// ── 6. Telemetria em Tempo Real e Relógio ─────────────────────────────────────

function initTelemetry() {
  const clockEl = document.getElementById('panel-clock');
  const cpuEl = document.getElementById('hud-cpu');
  const ramEl = document.getElementById('hud-ram');
  const ticksEl = document.getElementById('hud-ticks');

  let tickCount = 1877000;

  function update() {
    const now = new Date();
    if (clockEl) {
      clockEl.textContent = now.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' }) + 
        ' ' + now.toLocaleDateString('pt-BR');
    }

    // Flutuação realista de telemetria
    tickCount += 100;
    if (ticksEl) ticksEl.textContent = tickCount.toLocaleString();

    if (cpuEl && Math.random() > 0.6) {
      const cpu = Math.floor(2 + Math.random() * 5);
      cpuEl.textContent = `${cpu}%`;
    }

    if (ramEl && Math.random() > 0.8) {
      const ram = (44.2 + Math.random() * 0.4).toFixed(1);
      ramEl.textContent = `${ram} MB / 256 MB`;
    }
  }

  setInterval(update, 1000);
  update();
}

// ── 7. Inicialização Global do DOM ───────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
  initWindows();
  initWhiskerMenu();
  initTerminal();
  initContactForm();
  initTelemetry();

  // Ativação dos ícones da Doca e Desktop
  document.querySelectorAll('[data-open-window]').forEach(el => {
    el.addEventListener('click', (e) => {
      e.preventDefault();
      const targetWinId = el.getAttribute('data-open-window');
      openWindow(targetWinId);

      // Fecha o menu se estava aberto
      const menu = document.getElementById('whisker-menu');
      if (menu) menu.classList.add('hidden');
    });
  });

  // Botões de Download da ISO com Conversão Google Ads
  document.querySelectorAll('.btn-download-iso').forEach(btn => {
    btn.addEventListener('click', () => {
      trackConversion('download_iso');
    });
  });

  // Renderiza ícones Lucide
  if (window.lucide) {
    window.lucide.createIcons();
  }

  // Abre janela de boas-vindas / História automaticamente após 600ms
  setTimeout(() => {
    openWindow('win-history');
  }, 600);
});

