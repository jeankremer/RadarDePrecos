# Radar de Preços

Busca preços no Mercado Livre e acompanha o histórico dos produtos escolhidos. É um app local para Windows feito com
[Tauri 2](https://tauri.app): interface em HTML/CSS/JS e núcleo em Rust, na mesma base do Cofre de Credenciais.
O desenho e os planos ficam em [docs/plans/](docs/plans/).

## Telas

| Tela | Atalho | Para que serve |
|---|---|---|
| Buscar | `Ctrl+1` | Busca no catálogo do Mercado Livre. Cada cartão é um produto com o menor preço novo entre os vendedores, preço "de", Full e frete grátis |
| Acompanhando | `Ctrl+2` | Produtos acompanhados com o menor preço atual, o menor já visto, a variação e um mini gráfico de 30 dias. **Checar agora** consulta todos. Clicar num produto abre o histórico em gráfico e os links |
| Ajustes | | Conta do Mercado Livre, backup e restauração |

`Ctrl+F` vai para o campo de busca ou para o campo de link.

## Como funciona no Mercado Livre

Para apps não certificados, a API do ML recusa a busca de anúncios e a consulta de anúncios (`/items`). Ela libera o
**catálogo**: a busca de produtos (`/products/search`) e as ofertas de cada produto (`/products/{id}/items`).
Por isso o Radar trabalha com **produtos do catálogo**:

- O preço acompanhado é a **menor oferta nova** entre todos os vendedores do produto. Usados ficam de fora.
- Quando ninguém vende o produto, o histórico registra "sem oferta". Isso não conta como erro.
- Em "Acompanhar link" só entram links de produto (`…/p/MLB…`). Links de anúncio avulso
  (`produto.mercadolivre.com.br/MLB-…`) não podem ser consultados pela API.
- Produtos fora do catálogo não aparecem. Eles ficam para uma próxima etapa, pelo navegador escondido.

Um **produto** do Radar pode juntar mais de um link (por exemplo, cores diferentes do mesmo SSD). O gráfico mostra uma
linha por link, e a lista mostra o menor preço entre eles. O histórico só grava um ponto quando algo muda, ou um por
dia de checagem, e os valores ficam em centavos.

## Conectar o Mercado Livre

1. O app de desenvolvedor é o **Radar Ofertas JEV**, no DevCenter do Mercado Livre. O Client ID e a URI de redirect já
   vêm preenchidos em Ajustes.
2. Cole a **chave secreta** (Secret Key do DevCenter) em Ajustes e clique em **Salvar**.
3. Clique em **Conectar** e entre na sua conta do ML na janela que abre. Ela fecha sozinha.

A URI de redirect precisa ser **idêntica** à cadastrada no DevCenter. O app captura o retorno do login antes de a
página carregar, então o endereço não precisa existir. A chave secreta e o acesso ficam no **Gerenciador de
Credenciais do Windows** (serviço `com.jeank.radar`), fora do banco, dos backups e do git. O acesso é renovado sozinho.
Se ficar muito tempo sem uso, o app pede para conectar de novo.

## Onde ficam os dados

`%APPDATA%\com.jeank.radar\radar.db` (SQLite) e `config.json`, que guarda a pasta de backup, o Client ID, a URI de
redirect e o apelido da conta.

## Backup

Em **Ajustes**, escolha uma pasta (o botão "Usar o OneDrive" aparece se o OneDrive estiver configurado). A cada mudança,
o app copia o banco para `radar-AAAA-MM-DD.db` nessa pasta, uma cópia por dia, e mantém as 30 mais recentes.

Para restaurar (inclusive num computador novo), use **Restaurar um backup**. O banco atual é guardado como
`radar-antes-restauracao-….db`. A conta do ML precisa ser conectada de novo num computador novo, porque a chave fica
no Windows.

## Desenvolvimento

Pré-requisitos: Node.js, Rust (rustup) e Visual Studio Build Tools com C++.

```bash
npm install
npm run tauri dev
```

Abre o app com recarga automática. Para mexer só na interface pelo navegador, rode `npm run dev` e abra
http://localhost:1420. Um backend simulado ([src/dev-mock.js](src/dev-mock.js)) carrega dados de exemplo e não grava
nada.

Testes: `npm test` (JS com `node --test` e Rust com `cargo test`). Os testes do ML usam respostas reais salvas em
`src-tauri/tests/fixtures/ml/`, sem acessar a rede.

Em modo de desenvolvimento, **Ajustes → Diagnóstico da API** testa os endereços da API com o acesso atual e salva as
respostas e um resumo (`probe.txt`) nessa mesma pasta. Esses arquivos ficam fora do git.

## Gerar o instalador

```bash
npm run tauri build
```

O instalador sai em `src-tauri/target/release/bundle/nsis/`.

## Estrutura

```
src/                  interface (HTML/CSS/JS)
  main.js             navegação e atalhos
  core.js             toast, diálogo e chamada ao Rust
  format.js           preços, porcentagens e datas (sem Tauri, testado)
  charts.js           gráfico em degraus e mini gráfico (SVG puro, testado)
  search.js           tela Buscar
  products.js         tela Acompanhando e detalhe do produto
  track.js            diálogo "Acompanhar"
  settings.js         Ajustes: Mercado Livre e backup
  dev-mock.js         backend simulado (só no navegador, em desenvolvimento)
src-tauri/src/        núcleo em Rust
  lib.rs              comandos, configuração, login e acesso à API
  db.rs               banco SQLite: produtos, links e histórico
  prices.rs           regras do histórico (quando gravar, melhor preço por dia)
  ml.rs               leitura das respostas da API do Mercado Livre
  ml_auth.rs          OAuth com PKCE e renovação do acesso
  secrets.rs          Gerenciador de Credenciais do Windows
  backup.rs           backup diário e rotação
```
