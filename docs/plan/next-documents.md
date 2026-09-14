# Plano: Próximos Documentos

Objetivo: organizar e priorizar os próximos documentos a serem escritos para serviços e drivers do Jinn OS.

Prioridade alta
- `driver-manifest.md` — especificação do manifesto que cada driver deve fornecer (metadata, permissões, recursos, ABI).
- `service-api-spec.md` — convenções de API/IPC para serviços (schemas, mensagens, erros).
- `driver-dev-guide.md` — tutorial passo-a-passo para criar um driver user-space para Jinn.

Prioridade média
- `testing-and-benchmarks.md` — guias para testes de latência, jitter e stress.
- `security-guidelines.md` — práticas seguras para drivers e serviços (attestation, capabilities).

Prioridade baixa
- `examples/driver-sample.md` — exemplo completo verificável de driver simples.
- `contributing.md` — como contribuir com docs e código.

Próximos passos:
1. Escrever `driver-manifest.md` (esqueleto e exemplos).
2. Escrever `service-api-spec.md` com um template de mensagens IPC (Cap'n Proto/Flatbuffers).
3. Implementar um exemplo em `examples/`.

Se aprovar, começo pelo `driver-manifest.md` e gero também um exemplo de manifesto.
