//! Login no Mercado Livre (OAuth 2 com PKCE) e renovação do acesso.
//!
//! O ML não aceita token só do app (`client_credentials`): o usuário entra uma vez numa janela do Radar,
//! o app captura o `code` no redirect e troca pelo token. O refresh token é de uso único: cada renovação
//! devolve um novo, que precisa ser guardado na hora.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::Url;

const AUTH_URL: &str = "https://auth.mercadolivre.com.br/authorization";
const TOKEN_URL: &str = "https://api.mercadolibre.com/oauth/token";

fn random_b64(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    OsRng.fill_bytes(&mut b);
    B64URL.encode(b)
}

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn pkce() -> Pkce {
    let verifier = random_b64(32);
    let challenge = challenge_for(&verifier);
    Pkce { verifier, challenge }
}

pub fn challenge_for(verifier: &str) -> String {
    B64URL.encode(Sha256::digest(verifier.as_bytes()))
}

/// Valor aleatório que volta no redirect, para recusar retornos que o app não pediu.
pub fn random_state() -> String {
    random_b64(16)
}

pub fn authorize_url(client_id: &str, redirect: &str, challenge: &str, state: &str) -> Result<Url, String> {
    Url::parse_with_params(
        AUTH_URL,
        &[
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", redirect),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("state", state),
        ],
    )
    .map_err(|e| e.to_string())
}

/// Se `url` é o retorno do login (mesmo endereço do redirect), devolve o código ou o motivo da recusa.
/// `None` para qualquer outra página, que a janela continua carregando normalmente.
pub fn callback_code(url: &str, redirect: &str, state: &str) -> Option<Result<String, String>> {
    let u = Url::parse(url).ok()?;
    let r = Url::parse(redirect).ok()?;
    if u.scheme() != r.scheme() || u.host_str() != r.host_str() || u.path() != r.path() {
        return None;
    }
    let get = |k: &str| u.query_pairs().find(|(key, _)| key == k).map(|(_, v)| v.into_owned());
    if let Some(err) = get("error") {
        return Some(Err(format!("O Mercado Livre recusou o login: {}", get("error_description").unwrap_or(err))));
    }
    if get("state").as_deref() != Some(state) {
        return Some(Err("Retorno do login inválido. Tente conectar de novo".into()));
    }
    Some(get("code").ok_or_else(|| "O Mercado Livre não devolveu o código de acesso".to_string()))
}

#[derive(Debug, Deserialize)]
pub struct Token {
    pub access_token: String,
    /// Segundos (o ML usa 6 horas).
    pub expires_in: i64,
    pub refresh_token: Option<String>,
}

#[derive(Debug)]
pub enum TokenError {
    /// Refresh token vencido ou já usado: é preciso conectar de novo.
    Expired,
    Other(String),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::Expired => write!(f, "A conexão com o Mercado Livre expirou. Conecte de novo em Ajustes"),
            TokenError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

async fn post(http: &reqwest::Client, form: &[(&str, &str)]) -> Result<Token, TokenError> {
    let resp = http
        .post(TOKEN_URL)
        .header("accept", "application/json")
        .form(form)
        .send()
        .await
        .map_err(|e| TokenError::Other(format!("Sem conexão com o Mercado Livre: {e}")))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| TokenError::Other(e.to_string()))?;
    if status.is_success() {
        return serde_json::from_str(&body).map_err(|e| TokenError::Other(format!("Resposta inesperada do login: {e}")));
    }
    if body.contains("invalid_grant") {
        return Err(TokenError::Expired);
    }
    Err(TokenError::Other(format!("O Mercado Livre recusou o acesso ({status}): {body}")))
}

pub async fn exchange_code(
    http: &reqwest::Client,
    client_id: &str,
    secret: &str,
    redirect: &str,
    code: &str,
    verifier: &str,
) -> Result<Token, TokenError> {
    post(
        http,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("client_secret", secret),
            ("code", code),
            ("redirect_uri", redirect),
            ("code_verifier", verifier),
        ],
    )
    .await
}

pub async fn refresh(http: &reqwest::Client, client_id: &str, secret: &str, refresh_token: &str) -> Result<Token, TokenError> {
    post(
        http,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("client_secret", secret),
            ("refresh_token", refresh_token),
        ],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const REDIRECT: &str = "https://gocomercio.com.br/oauth/mercadolivre/callback";

    #[test]
    fn pkce_segue_a_rfc_7636() {
        // Exemplo do apêndice B da RFC 7636.
        assert_eq!(challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let p = pkce();
        assert_eq!(p.verifier.len(), 43);
        assert_eq!(p.challenge, challenge_for(&p.verifier));
    }

    #[test]
    fn monta_o_endereco_de_autorizacao() {
        let u = authorize_url("417769415941125", REDIRECT, "abc", "st1").unwrap();
        assert_eq!(u.host_str(), Some("auth.mercadolivre.com.br"));
        let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(q["client_id"], "417769415941125");
        assert_eq!(q["redirect_uri"], REDIRECT);
        assert_eq!((q["code_challenge"].as_str(), q["code_challenge_method"].as_str()), ("abc", "S256"));
        assert_eq!(q["state"], "st1");
    }

    #[test]
    fn captura_o_retorno_do_login() {
        assert_eq!(callback_code("https://www.mercadolivre.com.br/login", REDIRECT, "st1"), None);
        assert_eq!(callback_code("https://gocomercio.com.br/outra", REDIRECT, "st1"), None);
        assert_eq!(callback_code(&format!("{REDIRECT}?code=TG-123&state=st1"), REDIRECT, "st1"), Some(Ok("TG-123".into())));
        assert!(matches!(callback_code(&format!("{REDIRECT}?code=TG-123&state=outro"), REDIRECT, "st1"), Some(Err(_))));
        assert!(matches!(callback_code(&format!("{REDIRECT}?error=access_denied"), REDIRECT, "st1"), Some(Err(_))));
    }
}
