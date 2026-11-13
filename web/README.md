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

## 🗄️ Configuração do Banco de Dados no Supabase

1. Acesse o painel do seu projeto no [Supabase](https://supabase.com).
2. Vá em **SQL Editor** no menu lateral esquerdo.
3. Copie e cole todo o conteúdo do arquivo `supabase-schema.sql` e clique em **Run**.
4. Vá em **Project Settings** -> **API** e obtenha a **Project URL** e a **anon public key**.
5. No arquivo `index.html` (ou via variável de ambiente), configure:
   ```javascript
   window.ENV_SUPABASE_URL = 'https://SUA_URL_DO_SUPABASE.supabase.co';
   window.ENV_SUPABASE_ANON_KEY = 'SUA_CHAVE_ANONIMA_PUBLICA';
   ```

---

## 📈 Configuração do Google Ads

No cabeçalho de `index.html`, substitua `AW-XXXXXXXXX` pelo seu ID de tag global do Google Ads:

```html
<script async src="https://www.googletagmanager.com/gtag/js?id=AW-SEU_ID_AQUI"></script>
<script>
  window.dataLayer = window.dataLayer || [];
  function gtag(){dataLayer.push(arguments);}
  gtag('js', new Date());
  gtag('config', 'AW-SEU_ID_AQUI');
</script>
```

As conversões já estão mapeadas e são acionadas automaticamente nas seguintes ações:
- **Download da ISO:** `trackConversion('download_iso')`
- **Envio de Mensagem de Contato:** `trackConversion('contact_submit')`

---

## 🌐 Publicação no GitHub Pages (`jinn-os.dev.br`)

1. O arquivo `CNAME` já está configurado com `jinn-os.dev.br`.
2. No seu provedor de domínio (Registro.br ou Cloudflare), configure os seguintes registros DNS:
   - **Registro CNAME:**
     - Nome: `www` ou `@` (se suportado CNAME flattening)
     - Destino: `GeovaneEstanislau.github.io.`
   - **Registros A (GitHub Pages Apex):**
     - `185.199.108.153`
     - `185.199.109.153`
     - `185.199.110.153`
     - `185.199.111.153`
3. No repositório GitHub, vá em **Settings** -> **Pages** -> selecione o branch para publicação e confirme o domínio customizado.

