# Etapa 2: segundo plano, alertas e notificações — plano de implementação

> **Para o Claude:** execute tarefa por tarefa, com TDD nas regras puras e commit ao fim de cada tarefa.

**Objetivo:** o Radar checa os preços sozinho, fica perto do relógio, inicia com o Windows e avisa por notificação
quando um preço cai, chega ao preço-alvo, atinge o menor já visto ou volta a ter oferta. Tem uma tela de Alertas e um
detector de promoção inflada.

**Base:** etapa 1 (0.1.0), em que o ML funciona pelo catálogo (`/products/{id}/items`). Versão desta etapa: **0.2.0**.

**Novas dependências:** `tauri` com `tray-icon`, `tauri-plugin-notification`, `tauri-plugin-autostart`,
`tauri-plugin-single-instance` e `tokio` (feature `time`).

---

## Decisões

| Tema | Decisão |
|---|---|
| Intervalo | Configurável em Ajustes: 1, 3, 6, 12 ou 24 h, ou desligado. Padrão de 3 h |
| Espalhar | Na checagem automática, pausa aleatória de 5 a 15 s entre produtos. "Checar agora" não espera |
| Atraso ao ligar | Se a última checagem automática passou do intervalo, checa 30 s depois de abrir |
| Espera após bloqueio | Se **todas** as consultas de uma rodada falharem, o próximo intervalo dobra (até 24 h). Volta ao normal na primeira rodada boa |
| Fechar a janela | Esconde na bandeja. Na primeira vez avisa "O Radar continua rodando perto do relógio". **Sair** fica no menu da bandeja |
| Bandeja | Clique abre a janela. Menu com Abrir, Checar agora e Sair. Dica com a última checagem |
| Iniciar com o Windows | Desligado por padrão, com opção em Ajustes. Inicia escondido (`--minimized`) |
| Uma instância só | Abrir o app de novo mostra a janela que já existe |
| Uma checagem por vez | Se já tiver uma em andamento, "Checar agora" avisa |
| Regras por produto | Preço-alvo (opcional), queda mínima (padrão 5%), menor já visto (ligado). Ficam no detalhe do produto |
| Prioridade | No máximo 1 alerta por produto por rodada: preço-alvo > menor já visto > queda > voltou a ter oferta |
| Primeira leitura | Nunca gera alerta |
| Leitura suspeita | Queda maior que 60% em relação à leitura anterior do link **não é gravada**. O app checa de novo depois de 10 min; se o preço se confirmar (diferença de até 2%), grava e avisa, e se não se confirmar, descarta |
| Promoção inflada | O preço "de" da melhor oferta é mais de 5% acima do maior preço visto nos últimos 30 dias, com pelo menos 7 dias de histórico. Aparece como selo na lista e no detalhe |
| Notificação | Notificação do Windows com título "Radar de Preços" e o texto do alerta. Pode ser desligada em Ajustes. **Limitação:** no Windows o plugin não informa o clique na notificação, então ele não abre o produto. O alerta fica na tela Alertas |
| Tela Alertas | `Ctrl+3`. Lista com data, produto, tipo e preço de → para. Contador de não lidos na barra lateral. Abrir a tela marca tudo como lido. Clicar abre o produto |

## Tarefas

1. **Regras de alerta (`alerts.rs`, TDD)**: `evaluate`, `suspicious`, `confirms`, `inflated` e `brl` em Rust, mais o texto da notificação.
2. **Banco (migração 2, TDD)**: tabela `alerts`; `add_alert`, `list_alerts`, `unread_alerts`, `mark_alerts_read`,
   `update_rules`, `products_to_check` (links agrupados por produto), `last_price`, e `inflated`/`max_30d` no resumo.
3. **Checagem única (`run_check`)**: um só caminho para "Checar agora" e para a automática, com regras, leitura suspeita,
   gravação de alertas, notificação, eventos `checked`/`alerts` e trava de uma checagem por vez.
4. **Agendador + bandeja + fechar para a bandeja + uma instância + iniciar escondido.**
5. **Iniciar com o Windows e configurações** (`check_settings`, `set_check_settings`) + cartão "Checagem automática" em Ajustes.
6. **Tela Alertas** + contador na barra lateral + atualização das telas quando uma checagem termina.
7. **Regras no detalhe do produto** (cartão "Avisos") + selo de promoção inflada.
8. **Backend simulado, README, versão 0.2.0, instalador, merge e push.**
