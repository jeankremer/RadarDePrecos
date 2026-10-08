//! Núcleo do Radar de Preços: banco local, Mercado Livre e comandos da interface.

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
async fn ml_get(app: &AppHandle, state: &AppState, path: &str, query: &[(&str, &str)]) -> CmdResult<String> {
    let cfg = load_config(app);
    for attempt in 0..2 {
        let token = ensure_token(state, &cfg).await?;
        match ml::get(&state.http, &token, path, query).await {
            Err(ml::ApiError::Unauthorized) if attempt == 0 => state.ml.lock().await.access_token = None,
            r => return r.map_err(err),
        }
    }
    Err(ml::ApiError::Unauthorized.to_string())
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
        probe("anúncios em lote", "/items", &[("ids", i.as_str()), ("attributes", ml::ITEM_ATTRS)], Some("real-items.json")).await;
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

// ---------- comandos: busca ----------

#[tauri::command]
async fn search(app: AppHandle, state: State<'_, AppState>, query: String) -> CmdResult<Vec<ml::Offer>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let body = ml_get(&app, &state, &format!("/sites/{}/search", ml::SITE), &[("q", q), ("limit", "50")]).await?;
    ml::parse_search(&body)
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
            search
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o aplicativo");
}
