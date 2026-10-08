# Radar de Preços — desenho

Data: 2026-10-07

App local para Windows que busca e compara preços no Mercado Livre, Amazon e Shopee e acompanha produtos ao longo
do tempo, avisando quando o preço cai. Mesma base do Cofre de Credenciais: Tauri 2, interface em HTML/CSS/JS puro,
núcleo em Rust.

## Decisões

| Tema | Decisão |
|---|---|
| Uso | Buscar e comparar entre lojas **e** acompanhar produtos escolhidos |
| Coleta | Só soluções gratuitas, sem serviço de scraping pago |
| Monitoramento | Em segundo plano (bandeja + iniciar com o Windows) com notificação do Windows |
| Arquitetura | Híbrida, tudo dentro do app (abordagem A) |
| Armazenamento | SQLite local, sem criptografia (preço não é dado sensível) |

Alternativas descartadas: navegador escondido para todas as lojas (lento e pesado em segundo plano) e robô
Playwright separado (instalador de 150 MB+ e dois runtimes).

## Telas

**Buscar (`Ctrl+1`)**: um termo, as lojas escolhidas e a busca nas três em paralelo. A lista única é ordenada por
preço e mostra loja, foto, preço, preço "de" com % de desconto, frete grátis/Prime/Full, avaliação e vendedor.
Cada loja entra na lista quando responde. O botão **Acompanhar** cria um produto novo ou junta o link a um que já
existe.

**Acompanhando (`Ctrl+2`)**: um **produto** agrupa um ou mais links, que podem ser de lojas diferentes. O
agrupamento é manual para não misturar produtos parecidos. A lista mostra o menor preço atual entre as lojas, o
menor preço histórico, a variação e um mini gráfico. O detalhe mostra o histórico com uma linha por loja.
**Promoção falsa**: o app compara o preço "de" com o histórico dos últimos 30 dias e avisa quando o desconto foi
inflado.

**Alertas (`Ctrl+3`)**: queda de preço, menor preço histórico, preço-alvo atingido e volta ao estoque. Cada
alerta também gera uma notificação do Windows, e clicar nela abre o produto. Regras por produto:
- preço-alvo (opcional)
- queda mínima, padrão de 5%
- avisar no menor histórico, ligado por padrão

**Ajustes**:
- intervalo de checagem (padrão de 3 h)
- iniciar com o Windows
- conectar conta de desenvolvedor do ML
- login na Shopee
- pasta de backup
- tema
- botão "Testar lojas"

Fora do escopo por enquanto: aviso no celular (Telegram), cupons, cashback, comparação automática por semelhança
e outras lojas.

## Coleta

Cada loja é um **coletor** em Rust com a mesma interface:
- `buscar(termo) -> Vec<Oferta>`
- `consultar(link) -> Leitura`, com preço, preço "de", estoque, frete e título

| Loja | Como |
|---|---|
| Mercado Livre | API oficial com login do usuário (OAuth + PKCE) no app "Radar Ofertas JEV" do DevCenter. O token é renovado automaticamente. Os itens acompanhados são consultados em lote (até 20 por chamada). Validado em 07/10: sem token tudo dá 403, `client_credentials` não é aceito e o redirect `localhost` é recusado, então o app usa `https://gocomercio.com.br/oauth/mercadolivre/callback` e captura a navegação antes de carregar |
| Amazon | HTTP direto em `amazon.com.br` com leitura do HTML. Se vier a página de verificação, tenta pelo navegador escondido; se continuar, pausa a loja e avisa. Sem nenhuma forma de burlar captcha: o usuário pode abrir a janela e resolver |
| Shopee | Janela WebView2 invisível. Um script injetado captura as respostas JSON que a própria página recebe, em vez de ler o DOM. O login é feito pelo usuário uma vez nessa janela e a sessão fica salva |

## Agendamento

- Fechar a janela esconde o app na bandeja. Plugins: autostart e notification.
- Em cada ciclo as checagens são espalhadas: uma loja por vez e pausa aleatória de 5–15 s entre os itens.
- **Espera após bloqueio, por loja**: 2× o intervalo, até no máximo 24 h. As outras lojas continuam normalmente.
- Ao iniciar, se a última checagem estiver atrasada, checa logo.
- O ícone da bandeja mostra o status, por exemplo "Última checagem 14:32 · Shopee pausada".

## Dados (SQLite em `%APPDATA%`)

| Tabela | Campos |
|---|---|
| `produtos` | nome, preço-alvo, queda mínima %, avisar no menor histórico, arquivado |
| `links` | produto, loja, código na loja (MLB / ASIN / ID da Shopee), url, título, foto, última checagem, último erro, falhas seguidas |
| `precos` | link, quando, preço, preço "de", em estoque, frete grátis |
| `alertas` | produto, link, tipo, preço antes/depois, quando, lido |

- Valores em **centavos inteiros**.
- Em `precos` só entra um ponto novo quando algo muda, ou no mínimo um por dia.
- **Backup** como no Cofre: uma cópia por dia na pasta escolhida, mantendo as 30 mais recentes, com restauração em
  Ajustes.

## Erros

O princípio é nunca gerar alerta falso.

- **Bloqueio** e **site mudou** são erros diferentes. O segundo diz qual coletor precisa de ajuste.
- **Leitura suspeita**: uma queda maior que 60% (por exemplo, a parcela lida como se fosse o preço) é confirmada
  com uma segunda leitura minutos depois, antes de virar alerta.
- Depois de 3 falhas seguidas, o link aparece como "link com problema". Os outros links do produto seguem
  funcionando.

## Testes

- **Coletores**: testados com páginas e respostas reais salvas em `tests/fixtures/`, sem acessar a rede.
- **Regras de alerta, promoção falsa e espera após bloqueio**: funções puras com testes unitários em Rust.
- **Interface**: `node --test` e um backend simulado `dev-mock.js` com dados de exemplo.
- **Checagem real**: feita manualmente pelo botão "Testar lojas" em Ajustes.
