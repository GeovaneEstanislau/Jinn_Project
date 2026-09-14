# Predictive Engine — Jinn OS

## 1. Visão Geral

O `Predictive Engine` é um serviço central responsável por analisar telemetria do sistema (Scheduler, Cache Manager, Memory Core, serviços) e emitir previsões, hints e políticas adaptativas para otimizar desempenho, latência e consumo de recursos no Jinn.

Ele não toma decisões autoritárias; emite recomendações que componentes (Scheduler, Cache Manager, Process Supervisor) podem consultar ou aceitar automaticamente, dependendo de políticas configuradas.

## 2. Objetivos

- Fornecer previsões de carga e comportamento de serviços.
- Gerar hints para warming/prefetch, migração de threads e alocação de pools.
- Minimizar overhead quando inativo e ser responsivo para bursts.
- Suportar modelos heurísticos e ML-based plugáveis.

## 3. Responsabilidades

- Coletar e agregar telemetria (latência, throughput, miss-rate, CPU usage).
- Treinar e avaliar modelos (online/offline) para prever demanda futura.
- Emitir sinais e recomendações com confiança e janelas temporais.

## 4. Arquitetura Interna

- `Ingest Pipeline`: coleta eventos e amostras de telemetria.
- `Feature Store`: mantém séries temporais e features derivadas.
- `Model Runner`: executa modelos heurísticos e ML para previsões.
- `Decision API`: expõe recomendações com metadados (confiança, TTL).

Topologia:

  [Telemetry Sources] -> [Ingest] -> [Feature Store] -> [Model Runner] -> [Decision API]

## 5. Estruturas de Dados

- `TelemetrySample { ts, source, metric, value }`
- `FeatureVector { ts, features... }`
- `Prediction { target, window_start, window_end, confidence, action }`

## 6. Fluxo de Funcionamento

1. Coleta: componentes emitem amostras via IPC para Ingest Pipeline.
2. Transformação: Feature Store normaliza e extrai features (moving averages, histograms).
3. Inferência: Model Runner avalia modelos e gera `Prediction`.
4. Publicação: Decision API publica predictions com TTL; subscritores recebem notificações.

## 7. Interfaces Públicas

- `predict_subscribe(filter)` — subscribe to predictions.
- `predict_query(target, window)` — synchronous query for immediate hints.
- `predict_feedback(prediction_id, outcome)` — feedback loop for training.

## 8. Integração com outros componentes

- Scheduler: recebe hints de migração e perfil switching.
- Cache Manager: recebe warming/prefetch hints.
- Process Supervisor: recomenda scale-out/in e pre-start warmups.
- Memory Core: recomenda pool reserves e NUMA placement.

## 9. Segurança

- Predictions considered advisory; ações automatizadas requerem capability/consent per policy.
- Telemetry may conter sensíveis; proteções e políticas de retenção aplicadas.

## 10. Escalabilidade

- Projeto para execução distribuída: farm de model runners com sharding por serviço ou tenant.
- Feature Store com retenção configurável e downsampling para séries longas.

## 11. Futuras Evoluções

- Support for federated learning across nodes.
- Runtime adaptation of model complexity based on load and resource budget.

## 12. Comparação com sistemas modernos

- Inspirado por práticas de observability e AIOps; difere por ser um subsistema nativo do SO com hooks de baixa-latência para escalonamento e memory management.

## 13. Pseudocódigo

```pseudo
on_telemetry(sample):
  feature_store.append(sample)
  if model_runner.should_run(sample.ts):
    fv = feature_store.build_vector(window)
    pred = model_runner.infer(fv)
    decision_api.publish(pred)

on_feedback(pred_id, outcome):
  model_runner.update(pred_id, outcome)

```

## 14. Diagramas ASCII

Dataflow:

  [Scheduler, Cache, Services] -> [Ingest] -> [Feature Store] -> [Model Runner] -> [Decision API]

## 15. Considerações para implementação em Rust

- Organização em crates: `jinn-predict-ingest`, `jinn-predict-store`, `jinn-predict-models`, `jinn-predict-api`.
- ML runners podem usar bindings para `onnx-runtime` ou `tract` para modelos offline; heuristics implementados nativamente.
- Garantir isolamento: modelos treinados por terceiros executados em sandbox com limites de recursos.

---

Arquivo: [Predictive Engine](./predictive-engine.md)
