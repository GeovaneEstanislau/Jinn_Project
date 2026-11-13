# Portal Web Oficial do Jinn OS (`jinn-os.dev.br`)

Este repositório contém a aplicação web completa do portal oficial do **Jinn OS**, projetada como uma experiência imersiva de simulação de desktop de sistema operacional (baseada na interface XFCE / Cairo-Dock com o papel de parede temático da águia).

---

## 🚀 Tecnologias Empregadas

- **Interface & Design:** HTML5 / CSS3 / Tailwind CSS com estética *Glassmorphism*, paleta Dr460nized / Obsidian Glass.
- **Dock Dinâmica:** Cairo-Dock flutuante com reflexos translúcidos e zoom com amortecimento elástico cúbico.
- **Menu Iniciar:** Whisker Menu com categorização e busca rápida em tempo real com links diretos para os arquivos no GitHub.
- **Terminal Interativo:** Emulador de terminal executável em JavaScript com interpretador de comandos do sistema (`help`, `uname -a`, `uptime`, `ps`, `ls`, `cat`, etc.).
- **Banco de Dados (Supabase):** Envio e captação de mensagens de contato sem exposição de e-mail e com proteção contra spam via *Row Level Security* (RLS).
- **Métricas & Marketing (Google Ads):** Integração pronta do `gtag.js` com disparos customizados de conversão (`trackConversion`).

---

## 📁 Estrutura de Arquivos

```text
web/
├── index.html            # Aplicação SPA completa (Desktop, Janelas e Dock)
├── style.css             # Estilos de vidro fosco, sombras, reflexos e animações
├── app.js                # Lógica do Window Manager, Terminal e Conexão Supabase
├── CNAME                 # Domínio personalizado (jinn-os.dev.br)
├── supabase-schema.sql   # Script SQL para criar tabela e regras de RLS no Supabase
├── README.md             # Instruções de configuração e deploy
└── assets/
    └── wallpaper-eagle.jpg # Papel de parede de alta resolução da águia
```

---

## 🛠️ Como Testar Localmente

Basta abrir o arquivo `index.html` diretamente em qualquer navegador moderno (Chrome, Firefox, Edge, Safari):

```bash
# Ou utilizando um servidor estático local:
cd web
npx serve .
# ou
python -m http.server 8080
```

---


     - `185.199.111.153`
3. No repositório GitHub, vá em **Settings** -> **Pages** -> selecione o branch para publicação e confirme o domínio customizado.

