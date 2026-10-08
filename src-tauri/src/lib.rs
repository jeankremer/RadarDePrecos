//! Núcleo do Radar de Preços: banco local, Mercado Livre e comandos da interface.

mod backup;
mod db;
mod ml;
mod ml_auth;
mod prices;
mod secrets;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

const DB_FILE: &str = "radar.db";
const ML_LOGIN_WINDOW: &str = "ml-login";
/// Do app "Radar Ofertas JEV" no DevCenter. O Client ID é público; a chave secreta não fica no código.
const DEFAULT_ML_CLIENT_ID: &str = "417769415941125";
/// Precisa ser idêntica à cadastrada no DevCenter. A página nunca chega a carregar: o app captura antes.
const DEFAULT_ML_REDIRECT: &str = "https://gocomercio.com.br/oauth/mercadolivre/callback";

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Default)]
struct MlSession {
    access_token: Option<String>,
    /// Segundos Unix.
    expires_at: i64,
}

struct PendingLogin {
    verifier: String,
}

struct AppState {
    db: Mutex<Connection>,
    http: reqwest::Client,
    /// Mutex assíncrono: duas renovações ao mesmo tempo gastariam o refresh token (uso único).
    ml: tauri::async_runtime::Mutex<MlSession>,
    pending_login: Mutex<Option<PendingLogin>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn data_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    let dir = app.path().app_data_dir().map_err(|e| format!("Pasta de dados indisponível: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("Não foi possível criar a pasta de dados: {e}"))?;
    Ok(dir)
}

/// Configurações que não são secretas (texto puro em `config.json`, ao lado do banco).
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    backup_dir: Option<String>,
    ml_client_id: String,
    ml_redirect_uri: String,
    ml_nickname: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backup_dir: None,
            ml_client_id: DEFAULT_ML_CLIENT_ID.into(),
            ml_redirect_uri: DEFAULT_ML_REDIRECT.into(),
            ml_nickname: None,
        }
    }
}

fn load_config(app: &AppHandle) -> Config {
    data_dir(app)
        .ok()
        .and_then(|d| fs::read(d.join("config.json")).ok())
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn save_config(app: &AppHandle, config: &Config) -> CmdResult<()> {
    let json = serde_json::to_vec_pretty(config).map_err(err)?;
    fs::write(data_dir(app)?.join("config.json"), json).map_err(|e| format!("Não foi possível salvar as configurações: {e}"))
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

// ---------- acesso ao Mercado Livre ----------

/// Token válido, renovando com o refresh token quando faltar menos de 1 minuto.
async fn ensure_token(state: &AppState, cfg: &Config) -> CmdResult<String> {
    let mut s = state.ml.lock().await;
    if let Some(t) = &s.access_token {
        if now_secs() < s.expires_at - 60 {
            return Ok(t.clone());
        }
    }
    let refresh = secrets::get(secrets::ML_REFRESH)?.ok_or("Conecte sua conta do Mercado Livre em Ajustes")?;
    let secret = secrets::get(secrets::ML_SECRET)?.ok_or("Informe a chave secreta do Mercado Livre em Ajustes")?;
    match ml_auth::refresh(&state.http, &cfg.ml_client_id, &secret, &refresh).await {
        Ok(tok) => {
            if let Some(r) = &tok.refresh_token {
                secrets::set(secrets::ML_REFRESH, r)?;
            }
            s.access_token = Some(tok.access_token.clone());
            s.expires_at = now_secs() + tok.expires_in;
            Ok(tok.access_token)
        }
        Err(ml_auth::TokenError::Expired) => {
            secrets::delete(secrets::ML_REFRESH)?;
            Err(ml_auth::TokenError::Expired.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// GET na API do ML. Se o token for recusado (401), renova uma vez e tenta de novo.
async fn ml_get(app: &AppHandle, state: &AppState, path: &str, query: &[(&str, &str)]) -> Result<String, ml::ApiError> {
    let cfg = load_config(app);
    for attempt in 0..2 {
        let token = ensure_token(state, &cfg).await.map_err(ml::ApiError::Other)?;
        match ml::get(&state.http, &token, path, query).await {
            Err(ml::ApiError::Unauthorized) if attempt == 0 => state.ml.lock().await.access_token = None,
            r => return r,
        }
    }
    Err(ml::ApiError::Unauthorized)
}

/// Melhor oferta nova de um produto do catálogo. `Ok(None)` quando ninguém vende (a API responde 404).
async fn product_offers(app: &AppHandle, id: &str) -> Result<Option<ml::BestOffer>, ml::ApiError> {
    let state = app.state::<AppState>();
    match ml_get(app, &state, &format!("/products/{id}/items"), &[("limit", "20")]).await {
        Ok(body) => Ok(ml::parse_product_items(&body)),
        Err(ml::ApiError::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}

// ---------- comandos: conta do Mercado Livre ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MlStatus {
    client_id: String,
    redirect_uri: String,
    has_secret: bool,
    connected: bool,
    nickname: Option<String>,
}

#[tauri::command]
fn ml_status(app: AppHandle) -> CmdResult<MlStatus> {
    let cfg = load_config(&app);
    Ok(MlStatus {
        client_id: cfg.ml_client_id,
        redirect_uri: cfg.ml_redirect_uri,
        has_secret: secrets::get(secrets::ML_SECRET)?.is_some(),
        connected: secrets::get(secrets::ML_REFRESH)?.is_some(),
        nickname: cfg.ml_nickname,
    })
}

/// Salva Client ID e redirect. A chave secreta só é trocada quando vem preenchida.
/// Mudar Client ID ou redirect desconecta, porque o acesso pertence ao app antigo.
#[tauri::command]
async fn ml_save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    client_id: String,
    redirect_uri: String,
    client_secret: Option<String>,
) -> CmdResult<MlStatus> {
    let client_id = client_id.trim().to_string();
    let redirect_uri = redirect_uri.trim().to_string();
    if client_id.is_empty() || !client_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("O Client ID tem só números".into());
    }
    if !redirect_uri.starts_with("https://") {
        return Err("A URI de redirect precisa começar com https://".into());
    }
    let mut cfg = load_config(&app);
    let changed = cfg.ml_client_id != client_id || cfg.ml_redirect_uri != redirect_uri;
    if let Some(secret) = client_secret.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        secrets::set(secrets::ML_SECRET, &secret)?;
    }
    if changed {
        secrets::delete(secrets::ML_REFRESH)?;
        cfg.ml_nickname = None;
        state.ml.lock().await.access_token = None;
    }
    cfg.ml_client_id = client_id;
    cfg.ml_redirect_uri = redirect_uri;
    save_config(&app, &cfg)?;
    ml_status(app)
}

/// Abre a janela de login do ML. O resultado chega à interface pelo evento `ml-login`.
/// Precisa ser `async`: no Windows, criar janela num comando síncrono trava (a janela fica em branco).
#[tauri::command]
async fn ml_connect(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let cfg = load_config(&app);
    if secrets::get(secrets::ML_SECRET)?.is_none() {
        return Err("Salve a chave secreta antes de conectar".into());
    }
    let pkce = ml_auth::pkce();
    let st = ml_auth::random_state();
    let url = ml_auth::authorize_url(&cfg.ml_client_id, &cfg.ml_redirect_uri, &pkce.challenge, &st)?;
    *lock(&state.pending_login) = Some(PendingLogin { verifier: pkce.verifier });
    if let Some(w) = app.get_webview_window(ML_LOGIN_WINDOW) {
        let _ = w.close();
    }
    let redirect = cfg.ml_redirect_uri;
    let handle = app.clone();
    WebviewWindowBuilder::new(&app, ML_LOGIN_WINDOW, WebviewUrl::External(url))
        .title("Conectar ao Mercado Livre")
        .inner_size(520.0, 760.0)
        .center()
        .on_navigation(move |u| match ml_auth::callback_code(u.as_str(), &redirect, &st) {
            None => true,
            Some(code) => {
                // Cancela a navegação: o endereço do redirect nunca é carregado.
                let h = handle.clone();
                tauri::async_runtime::spawn(async move { finish_login(h, code).await });
                false
            }
        })
        .build()
        .map_err(err)?;
    Ok(())
}

async fn finish_login(app: AppHandle, code: Result<String, String>) {
    if let Some(w) = app.get_webview_window(ML_LOGIN_WINDOW) {
        let _ = w.close();
    }
    let payload = match complete_login(&app, code).await {
        Ok(nickname) => json!({ "ok": true, "nickname": nickname }),
        Err(error) => json!({ "ok": false, "error": error }),
    };
    let _ = app.emit("ml-login", payload);
}

async fn complete_login(app: &AppHandle, code: Result<String, String>) -> CmdResult<Option<String>> {
    let code = code?;
    let state = app.state::<AppState>();
    let pending = lock(&state.pending_login).take().ok_or("O login expirou. Tente conectar de novo")?;
    let mut cfg = load_config(app);
    let secret = secrets::get(secrets::ML_SECRET)?.ok_or("Salve a chave secreta antes de conectar")?;
    let token = ml_auth::exchange_code(&state.http, &cfg.ml_client_id, &secret, &cfg.ml_redirect_uri, &code, &pending.verifier)
        .await
        .map_err(err)?;
    let refresh = token
        .refresh_token
        .as_deref()
        .ok_or("O Mercado Livre não liberou o acesso contínuo. Ative \"offline_access\" nas permissões do app no DevCenter")?;
    secrets::set(secrets::ML_REFRESH, refresh)?;
    {
        let mut s = state.ml.lock().await;
        s.access_token = Some(token.access_token.clone());
        s.expires_at = now_secs() + token.expires_in;
    }
    let nickname = ml::get(&state.http, &token.access_token, "/users/me", &[]).await.ok().and_then(|b| ml::parse_nickname(&b));
    cfg.ml_nickname = nickname.clone();
    save_config(app, &cfg)?;
    Ok(nickname)
}

#[tauri::command]
async fn ml_disconnect(app: AppHandle, state: State<'_, AppState>) -> CmdResult<MlStatus> {
    secrets::delete(secrets::ML_REFRESH)?;
    state.ml.lock().await.access_token = None;
    let mut cfg = load_config(&app);
    cfg.ml_nickname = None;
    save_config(&app, &cfg)?;
    ml_status(app)
}

/// Só em desenvolvimento: testa vários endereços da API com o acesso atual e salva as respostas em
/// `src-tauri/tests/fixtures/ml` (resumo em `probe.txt`). Só dados públicos de catálogo e anúncios:
/// `/users/me` aparece só com o status, sem o corpo.
#[tauri::command]
async fn ml_dump_fixtures(app: AppHandle, state: State<'_, AppState>, query: String) -> CmdResult<String> {
    if !cfg!(debug_assertions) {
        return Err("Disponível só em desenvolvimento".into());
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("ml");
    let mut report = Vec::new();
    // Faz a chamada, anota OK/erro no resumo e salva o corpo quando `file` é informado.
    let mut probe = async |name: &str, path: &str, query: &[(&str, &str)], file: Option<&str>| -> Option<serde_json::Value> {
        match ml_get(&app, &state, path, query).await {
            Ok(body) => {
                report.push(format!("OK    {name}: {path} {query:?}"));
                if let Some(f) = file {
                    let _ = fs::write(dir.join(f), &body);
                }
                serde_json::from_str(&body).ok()
            }
            Err(e) => {
                report.push(format!("ERRO  {name}: {path} {query:?}\n      {e}"));
                None
            }
        }
    };
    let q = query.as_str();
    probe("busca de anúncios", "/sites/MLB/search", &[("q", q), ("limit", "5")], Some("real-search.json")).await;
    let products = probe("busca de catálogo", "/products/search", &[("status", "active"), ("site_id", "MLB"), ("q", q), ("limit", "10")], Some("real-products-search.json")).await;
    let highlights = probe("mais vendidos da categoria", "/highlights/MLB/category/MLB1672", &[], Some("real-highlights.json")).await;
    // Candidatos: mais vendidos primeiro (quase sempre têm vendedor), depois os resultados da busca.
    let mut candidates: Vec<String> = Vec::new();
    for v in [&highlights, &products].into_iter().flatten() {
        let list = v["content"].as_array().or_else(|| v["results"].as_array()).cloned().unwrap_or_default();
        candidates.extend(list.iter().filter_map(|x| x["id"].as_str().map(String::from)).take(6));
    }
    let (mut product_id, mut item_id) = (None, None);
    for p in &candidates {
        let prod = probe("produto do catálogo", &format!("/products/{p}"), &[], None).await;
        let winner = prod.as_ref().and_then(|v| v["buy_box_winner"]["item_id"].as_str()).map(String::from);
        let items = probe("anúncios do produto", &format!("/products/{p}/items"), &[], None).await;
        let first = items.as_ref().and_then(|v| v["results"][0]["item_id"].as_str()).map(String::from);
        if let Some(i) = winner.or(first) {
            if let Some(v) = &prod {
                let _ = fs::write(dir.join("real-product.json"), v.to_string());
            }
            if let Some(v) = &items {
                let _ = fs::write(dir.join("real-product-items.json"), v.to_string());
            }
            product_id = Some(p.clone());
            item_id = Some(i);
            break;
        }
    }
    if let Some(i) = &item_id {
        probe("anúncios em lote", "/items", &[("ids", i.as_str()), ("attributes", "id,title,price,status")], Some("real-items.json")).await;
        probe("anúncio", &format!("/items/{i}"), &[], Some("real-item.json")).await;
        probe("preço de venda", &format!("/items/{i}/sale_price"), &[("context", "channel_marketplace")], Some("real-sale-price.json")).await;
        probe("preços do anúncio", &format!("/items/{i}/prices"), &[], Some("real-item-prices.json")).await;
    }
    probe("usuário (só status)", "/users/me", &[], None).await;
    let ok = report.iter().filter(|l| l.starts_with("OK")).count();
    let total = report.len();
    let summary = format!("produto: {product_id:?}\nanúncio: {item_id:?}\n\n{}\n", report.join("\n"));
    fs::write(dir.join("probe.txt"), &summary).map_err(err)?;
    Ok(format!("Diagnóstico salvo: {ok} de {total} endereços OK. Avise o Claude."))
}

// ---------- comandos: busca e acompanhamento ----------

/// Produtos do catálogo consultados por busca (cada um custa uma chamada de ofertas, feitas em paralelo).
const SEARCH_LIMIT: &str = "20";

/// Busca no catálogo e traz a melhor oferta de cada produto. Produtos sem oferta ficam de fora.
#[tauri::command]
async fn search(app: AppHandle, state: State<'_, AppState>, query: String) -> CmdResult<Vec<ml::Offer>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let body = ml_get(&app, &state, "/products/search", &[("status", "active"), ("site_id", ml::SITE), ("q", q), ("limit", SEARCH_LIMIT)])
        .await
        .map_err(err)?;
    let tasks: Vec<_> = ml::parse_catalog_search(&body)?
        .into_iter()
        .map(|p| {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let best = product_offers(&app, &p.id).await;
                (p, best)
            })
        })
        .collect();
    let mut offers = Vec::new();
    let mut first_error = None;
    for task in tasks {
        match task.await {
            Ok((p, Ok(Some(best)))) => offers.push(ml::offer(p, best)),
            Ok((_, Ok(None))) => {}
            Ok((_, Err(e))) => first_error = first_error.or(Some(e.to_string())),
            Err(e) => first_error = first_error.or(Some(e.to_string())),
        }
    }
    // Se tudo falhou (sem conexão, acesso expirado), mostra o motivo em vez de "nada encontrado".
    match first_error {
        Some(e) if offers.is_empty() => Err(e),
        _ => Ok(offers),
    }
}

/// Para onde vai o link acompanhado: um produto existente ou um novo
/// (sem nome, o produto novo usa o nome do catálogo).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackTarget {
    product_id: Option<i64>,
    new_name: Option<String>,
}

/// Cria o produto (se preciso), o link e o primeiro ponto do histórico numa transação só.
fn insert_tracked(state: &AppState, target: &TrackTarget, link: &db::NewLink, reading: &prices::Reading) -> CmdResult<i64> {
    let mut conn = lock(&state.db);
    let tx = conn.transaction().map_err(err)?;
    let now = now();
    let product_id = match target.product_id {
        Some(id) => id,
        None => {
            let name = target.new_name.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(link.title);
            db::add_product(&tx, name, &now).map_err(err)?
        }
    };
    let link_id = db::add_link(&tx, product_id, link)?;
    db::record_reading(&tx, link_id, reading, &now).map_err(err)?;
    tx.commit().map_err(err)?;
    Ok(product_id)
}

#[tauri::command]
async fn track(app: AppHandle, state: State<'_, AppState>, offer: ml::Offer, target: TrackTarget) -> CmdResult<i64> {
    let reading = prices::Reading { price: Some(offer.price), list_price: offer.list_price, in_stock: true, free_shipping: offer.free_shipping };
    let link = db::NewLink { store: &offer.store, code: &offer.code, url: &offer.url, title: &offer.title, image: offer.image.as_deref() };
    let id = insert_tracked(&state, &target, &link, &reading)?;
    after_change(&app, &state);
    Ok(id)
}

#[tauri::command]
async fn track_url(app: AppHandle, state: State<'_, AppState>, url: String, target: TrackTarget) -> CmdResult<i64> {
    let id = match ml::parse_link(&url).ok_or("Cole um link do Mercado Livre")? {
        ml::LinkRef::Catalog(id) => id,
        ml::LinkRef::Item(_) => {
            return Err("Esse é o link de um anúncio avulso, que o Mercado Livre não deixa consultar. \
                Use o link do produto (com /p/MLB… no endereço) ou encontre o produto em Buscar."
                .into())
        }
    };
    let body = ml_get(&app, &state, &format!("/products/{id}"), &[]).await.map_err(|e| match e {
        ml::ApiError::NotFound => "Produto não encontrado no catálogo do Mercado Livre".to_string(),
        e => e.to_string(),
    })?;
    let product = ml::parse_product(&body).ok_or("Resposta inesperada do Mercado Livre")?;
    let best = product_offers(&app, &id).await.map_err(err)?;
    let url = ml::product_url(&id);
    let link = db::NewLink { store: ml::STORE, code: &id, url: &url, title: &product.name, image: product.image.as_deref() };
    let product_id = insert_tracked(&state, &target, &link, &ml::reading(best.as_ref()))?;
    after_change(&app, &state);
    Ok(product_id)
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct CheckSummary {
    checked: usize,
    changed: usize,
    failed: usize,
}

/// Consulta a melhor oferta de cada produto acompanhado e grava o que mudou.
#[tauri::command]
async fn check_now(app: AppHandle, state: State<'_, AppState>) -> CmdResult<CheckSummary> {
    let links = db::links_to_check(&lock(&state.db), ml::STORE).map_err(err)?;
    let mut sum = CheckSummary::default();
    for (link_id, code) in &links {
        let result = product_offers(&app, code).await;
        let conn = lock(&state.db);
        match result {
            Ok(best) => {
                if db::record_reading(&conn, *link_id, &ml::reading(best.as_ref()), &now()).map_err(err)? {
                    sum.changed += 1;
                }
                sum.checked += 1;
            }
            Err(e) => {
                db::record_failure(&conn, *link_id, &e.to_string(), &now()).map_err(err)?;
                sum.failed += 1;
            }
        }
    }
    after_change(&app, &state);
    Ok(sum)
}

#[tauri::command]
fn list_products(state: State<'_, AppState>) -> CmdResult<Vec<db::ProductSummary>> {
    db::list_products(&lock(&state.db), &today()).map_err(err)
}

#[tauri::command]
fn product_detail(state: State<'_, AppState>, id: i64) -> CmdResult<db::ProductDetail> {
    db::product_detail(&lock(&state.db), id, &today()).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => "Produto não encontrado".to_string(),
        e => e.to_string(),
    })
}

#[tauri::command]
fn rename_product(app: AppHandle, state: State<'_, AppState>, id: i64, name: String) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Err("O nome não pode ficar vazio".into());
    }
    db::rename_product(&lock(&state.db), id, &name).map_err(err)?;
    after_change(&app, &state);
    Ok(())
}

#[tauri::command]
fn delete_product(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    db::delete_product(&lock(&state.db), id).map_err(err)?;
    after_change(&app, &state);
    Ok(())
}

#[tauri::command]
fn delete_link(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    db::delete_link(&lock(&state.db), id).map_err(err)?;
    after_change(&app, &state);
    Ok(())
}

/// Depois de cada mudança nos dados: backup do dia na pasta escolhida.
/// Falhas não impedem a operação; aparecem no status do backup em Ajustes.
fn after_change(app: &AppHandle, state: &AppState) {
    if let Some(dir) = load_config(app).backup_dir {
        let _ = backup::run(&lock(&state.db), Path::new(&dir), &today(), backup::KEEP);
    }
}

// ---------- comandos: backup ----------

#[tauri::command]
fn backup_status(app: AppHandle) -> backup::BackupStatus {
    backup::status(load_config(&app).backup_dir.as_deref())
}

#[tauri::command]
fn set_backup_dir(app: AppHandle, state: State<'_, AppState>, dir: Option<String>) -> CmdResult<backup::BackupStatus> {
    if let Some(d) = &dir {
        backup::check_writable(Path::new(d))?;
    }
    let mut cfg = load_config(&app);
    cfg.backup_dir = dir.clone();
    save_config(&app, &cfg)?;
    if let Some(d) = &dir {
        backup::run(&lock(&state.db), Path::new(d), &today(), backup::KEEP)?;
    }
    Ok(backup::status(dir.as_deref()))
}

#[tauri::command]
fn backup_now(app: AppHandle, state: State<'_, AppState>) -> CmdResult<backup::BackupStatus> {
    let dir = load_config(&app).backup_dir.ok_or("Escolha uma pasta de backup primeiro")?;
    backup::run(&lock(&state.db), Path::new(&dir), &today(), backup::KEEP)?;
    Ok(backup::status(Some(&dir)))
}

/// Troca o banco atual por um backup. O atual é guardado ao lado, renomeado.
#[tauri::command]
fn restore_backup(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let source = PathBuf::from(&path);
    db::check_backup(&source)?;
    let target = data_dir(&app)?.join(DB_FILE);
    let keep = target.with_file_name(format!("radar-antes-restauracao-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let mut conn = lock(&state.db);
    conn.execute("VACUUM INTO ?1", [keep.to_string_lossy().into_owned()])
        .map_err(|e| format!("Não foi possível guardar o banco atual: {e}"))?;
    *conn = Connection::open_in_memory().map_err(err)?; // fecha o arquivo para poder substituir
    let copied = fs::copy(&source, &target);
    *conn = db::open(&target).map_err(err)?; // reabre mesmo se a cópia falhou
    copied.map_err(|e| format!("Não foi possível restaurar: {e}"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let conn = db::open(&data_dir(app.handle())?.join(DB_FILE))?;
            let http = reqwest::Client::builder()
                .user_agent(concat!("RadarDePrecos/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(20))
                .build()?;
            app.manage(AppState {
                db: Mutex::new(conn),
                http,
                ml: Default::default(),
                pending_login: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ml_status,
            ml_save_settings,
            ml_connect,
            ml_disconnect,
            ml_dump_fixtures,
            search,
            track,
            track_url,
            check_now,
            list_products,
            product_detail,
            rename_product,
            delete_product,
            delete_link,
            backup_status,
            set_backup_dir,
            backup_now,
            restore_backup
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o aplicativo");
}
