-- ==============================================================================
-- Jinn OS - Script de Criação do Banco de Dados no Supabase
-- Tabela de Mensagens de Contato e Políticas de Segurança (Row Level Security - RLS)
-- ==============================================================================

-- 1. Cria a tabela pública de mensagens
CREATE TABLE IF NOT EXISTS public.jinn_messages (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT timezone('utc'::text, now()) NOT NULL,
    sender_name TEXT NOT NULL,
    sender_contact TEXT, -- E-mail opcional ou telefone/handle para retorno
    message_content TEXT NOT NULL,
    ip_address TEXT,
    user_agent TEXT
);

-- 2. Habilita Row Level Security (RLS) para proteção estrita dos dados
ALTER TABLE public.jinn_messages ENABLE ROW LEVEL SECURITY;

-- 3. Política para permitir que qualquer visitante do site envie uma mensagem (INSERT anônimo)
CREATE POLICY "Permitir inserção pública de mensagens" 
ON public.jinn_messages 
FOR INSERT 
TO anon 
WITH CHECK (true);

-- 4. Política para bloquear leitura pública das mensagens (mitigação de raspagem e spam)
-- Apenas o administrador autenticado no painel Supabase tem acesso de leitura (SELECT).
CREATE POLICY "Bloquear leitura pública de mensagens" 
ON public.jinn_messages 
FOR SELECT 
TO anon 
USING (false);

-- 5. Comentários para documentação de schema
COMMENT ON TABLE public.jinn_messages IS 'Mensagens de contato recebidas pelo portal web oficial do Jinn OS (jinn-os.dev.br)';
COMMENT ON COLUMN public.jinn_messages.sender_name IS 'Nome do visitante ou remetente';
COMMENT ON COLUMN public.jinn_messages.sender_contact IS 'Forma de contato fornecida (e-mail, linkedin, etc.)';
COMMENT ON COLUMN public.jinn_messages.message_content IS 'Conteúdo da mensagem ou proposta de colaboração';

