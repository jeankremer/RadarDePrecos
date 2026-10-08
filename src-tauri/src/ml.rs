//! Mercado Livre: leitura das respostas da API oficial e chamadas HTTP.
//!
//! O app trabalha com **produtos do catálogo**: para apps não certificados, a API recusa (403) a busca de
//! anúncios e a consulta de anúncios (`/items`), mas libera a busca no catálogo e a lista de ofertas de
//! cada produto (`/products/{id}/items`), que já vem do menor preço para o maior.

use serde::{Deserialize, Serialize};

use crate::prices::Reading;

pub const STORE: &str = "ml";
pub const SITE: &str = "MLB";
const API: &str = "https://api.mercadolibre.com";

/// Página do produto no catálogo (a API devolve `permalink` vazio).
pub fn product_url(id: &str) -> String {
    format!("https://www.mercadolivre.com.br/p/{id}")
}

/// Um produto do catálogo com o menor preço entre as ofertas, já em centavos.
/// Também é o que a interface manda de volta para "Acompanhar".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub store: String,
    pub code: String,
    pub title: String,
    pub url: String,
    pub image: Option<String>,
    pub price: i64,
    pub list_price: Option<i64>,
    pub free_shipping: bool,
    pub full: bool,
    /// Quantos anúncios vendem o produto.
    pub sellers: u32,
}

/// Nome e foto de um produto do catálogo.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogProduct {
    pub id: String,
    pub name: String,
    pub image: Option<String>,
}

/// A oferta nova mais barata de um produto.
#[derive(Debug, Clone, PartialEq)]
pub struct BestOffer {
    pub price: i64,
    pub list_price: Option<i64>,
    pub free_shipping: bool,
    pub full: bool,
    pub sellers: u32,
}

pub fn to_cents(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

/// A API ainda devolve algumas fotos em http, que a CSP do app bloqueia.
fn https(url: Option<String>) -> Option<String> {
    url.map(|u| match u.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => u,
    })
}

/// O preço "de" só conta se for maior que o preço atual.
fn list_price(price: i64, original: Option<f64>) -> Option<i64> {
    original.map(to_cents).filter(|&o| o > price)
}

#[derive(Deserialize)]
struct Picture {
    url: String,
}

#[derive(Deserialize)]
struct ProductBody {
    id: String,
    name: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    pictures: Vec<Picture>,
}

impl From<ProductBody> for CatalogProduct {
    fn from(p: ProductBody) -> Self {
        CatalogProduct { id: p.id, name: p.name, image: https(p.pictures.into_iter().next().map(|pic| pic.url)) }
    }
}

/// Busca no catálogo (`/products/search`): só produtos ativos.
pub fn parse_catalog_search(json: &str) -> Result<Vec<CatalogProduct>, String> {
    #[derive(Deserialize)]
    struct Resp {
        results: Vec<ProductBody>,
    }
    let resp: Resp = serde_json::from_str(json).map_err(|e| format!("Resposta inesperada da busca do Mercado Livre: {e}"))?;
    Ok(resp
        .results
        .into_iter()
        .filter(|p| p.status.as_deref().map_or(true, |s| s == "active"))
        .map(CatalogProduct::from)
        .collect())
}

/// Produto do catálogo (`/products/{id}`).
pub fn parse_product(json: &str) -> Option<CatalogProduct> {
    serde_json::from_str::<ProductBody>(json).ok().map(CatalogProduct::from)
}

#[derive(Deserialize, Default)]
struct Shipping {
    #[serde(default)]
    free_shipping: bool,
    logistic_type: Option<String>,
}

/// Ofertas de um produto (`/products/{id}/items`): a nova mais barata, ou `None` se não houver.
/// Usadas e recondicionadas ficam de fora para não comparar coisas diferentes.
pub fn parse_product_items(json: &str) -> Option<BestOffer> {
    #[derive(Deserialize)]
    struct Item {
        price: Option<f64>,
        original_price: Option<f64>,
        condition: Option<String>,
        shipping: Option<Shipping>,
    }
    #[derive(Deserialize)]
    struct Paging {
        total: u32,
    }
    #[derive(Deserialize)]
    struct Resp {
        results: Vec<Item>,
        paging: Option<Paging>,
    }
    let resp: Resp = serde_json::from_str(json).ok()?;
    let total = resp.paging.map_or(resp.results.len() as u32, |p| p.total);
    let best = resp
        .results
        .into_iter()
        .filter(|i| i.condition.as_deref().map_or(true, |c| c == "new"))
        .filter_map(|i| i.price.map(|p| (to_cents(p), i)))
        .min_by_key(|(price, _)| *price)?;
    let (price, item) = best;
    let ship = item.shipping.unwrap_or_default();
    Some(BestOffer {
        price,
        list_price: list_price(price, item.original_price),
        free_shipping: ship.free_shipping,
        full: ship.logistic_type.as_deref() == Some("fulfillment"),
        sellers: total,
    })
}

/// O que gravar no histórico: a melhor oferta, ou "sem estoque" quando ninguém vende.
pub fn reading(best: Option<&BestOffer>) -> Reading {
    match best {
        Some(b) => Reading { price: Some(b.price), list_price: b.list_price, in_stock: true, free_shipping: b.free_shipping },
        None => Reading { price: None, list_price: None, in_stock: false, free_shipping: false },
    }
}

pub fn offer(p: CatalogProduct, b: BestOffer) -> Offer {
    Offer {
        store: STORE.into(),
        url: product_url(&p.id),
        code: p.id,
        title: p.name,
        image: p.image,
        price: b.price,
        list_price: b.list_price,
        free_shipping: b.free_shipping,
        full: b.full,
        sellers: b.sellers,
    }
}

pub fn parse_nickname(json: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Me {
        nickname: String,
    }
    serde_json::from_str::<Me>(json).ok().map(|m| m.nickname)
}

#[derive(Debug, PartialEq)]
pub enum LinkRef {
    /// Página de catálogo (`/p/MLB…`): é o que o app acompanha.
    Catalog(String),
    /// Anúncio avulso (MLB + dígitos). A API não deixa consultar.
    Item(String),
}

/// Dígitos logo depois de `pat` (ignorando um hífen), se forem pelo menos 6.
fn digits_after(s: &str, pat: &str) -> Option<String> {
    let mut rest = s;
    while let Some(i) = rest.find(pat) {
        let tail = &rest[i + pat.len()..];
        let tail = tail.strip_prefix('-').unwrap_or(tail);
        let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 6 {
            return Some(digits);
        }
        rest = &rest[i + pat.len()..];
    }
    None
}

/// Reconhece o que o usuário colou. O catálogo tem prioridade, mesmo quando o link aponta um anúncio dele.
pub fn parse_link(input: &str) -> Option<LinkRef> {
    let s = input.trim().to_uppercase();
    if let Some(d) = digits_after(&s, "/P/MLB") {
        return Some(LinkRef::Catalog(format!("MLB{d}")));
    }
    digits_after(&s, "MLB").map(|d| LinkRef::Item(format!("MLB{d}")))
}

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Forbidden(String),
    NotFound,
    TooMany,
    Other(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Unauthorized => write!(f, "O acesso ao Mercado Livre expirou. Conecte de novo em Ajustes"),
            ApiError::Forbidden(body) => write!(f, "O Mercado Livre recusou a consulta (403): {}", short(body)),
            ApiError::NotFound => write!(f, "Não encontrado no Mercado Livre"),
            ApiError::TooMany => write!(f, "Muitas consultas seguidas. Tente de novo em alguns minutos"),
            ApiError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

fn short(body: &str) -> String {
    body.chars().take(200).collect()
}

/// GET autenticado na API. Devolve o corpo da resposta.
pub async fn get(http: &reqwest::Client, token: &str, path: &str, query: &[(&str, &str)]) -> Result<String, ApiError> {
    let resp = http
        .get(format!("{API}{path}"))
        .bearer_auth(token)
        .query(query)
        .send()
        .await
        .map_err(|e| ApiError::Other(format!("Sem conexão com o Mercado Livre: {e}")))?;
    let status = resp.status().as_u16();
    let body = resp.text().await.map_err(|e| ApiError::Other(e.to_string()))?;
    match status {
        200..=299 => Ok(body),
        401 => Err(ApiError::Unauthorized),
        403 => Err(ApiError::Forbidden(body)),
        404 => Err(ApiError::NotFound),
        429 => Err(ApiError::TooMany),
        _ => Err(ApiError::Other(format!("O Mercado Livre respondeu {status}: {}", short(&body)))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_a_busca_no_catalogo() {
        let found = parse_catalog_search(include_str!("../tests/fixtures/ml/catalog-search.json")).unwrap();
        assert_eq!(found.len(), 10);
        assert_eq!(found[0].id, "MLB29752372");
        assert!(found[0].name.starts_with("Dell Inspiron"));
        assert_eq!(found[0].image.as_deref(), Some("https://http2.mlstatic.com/D_NQ_NP_641549-MLU73999134188_012024-F.jpg"));
    }

    #[test]
    fn le_o_produto() {
        let p = parse_product(include_str!("../tests/fixtures/ml/product.json")).unwrap();
        assert_eq!(p.id, "MLB39766120");
        assert_eq!(p.name, "SSD Kingston NV3 1TB M.2 2280 PCIe 4.0 NVMe 6000 MB/s");
        assert!(p.image.unwrap().starts_with("https://http2.mlstatic.com/"));
    }

    #[test]
    fn pega_a_oferta_nova_mais_barata() {
        let best = parse_product_items(include_str!("../tests/fixtures/ml/product-items.json")).unwrap();
        assert_eq!(best, BestOffer { price: 99700, list_price: Some(144900), free_shipping: true, full: false, sellers: 198 });
        assert_eq!(reading(Some(&best)), Reading { price: Some(99700), list_price: Some(144900), in_stock: true, free_shipping: true });
        assert_eq!(reading(None).in_stock, false);
    }

    #[test]
    fn ignora_usados_e_lista_vazia() {
        let json = r#"{"paging":{"total":2},"results":[
            {"price":50,"condition":"used","shipping":{"free_shipping":true}},
            {"price":80,"original_price":70,"condition":"new","shipping":{"free_shipping":false,"logistic_type":"fulfillment"}}]}"#;
        assert_eq!(parse_product_items(json), Some(BestOffer { price: 8000, list_price: None, free_shipping: false, full: true, sellers: 2 }));
        assert_eq!(parse_product_items(r#"{"paging":{"total":1},"results":[{"price":50,"condition":"used"}]}"#), None);
        assert_eq!(parse_product_items(r#"{"paging":{"total":0},"results":[]}"#), None);
    }

    #[test]
    fn monta_a_oferta_do_produto() {
        let p = CatalogProduct { id: "MLB39766120".into(), name: "SSD".into(), image: None };
        let o = offer(p, BestOffer { price: 99700, list_price: None, free_shipping: true, full: false, sellers: 3 });
        assert_eq!((o.code.as_str(), o.url.as_str(), o.sellers), ("MLB39766120", "https://www.mercadolivre.com.br/p/MLB39766120", 3));
    }

    #[test]
    fn reconhece_links_do_mercado_livre() {
        use LinkRef::*;
        let catalog = || Some(Catalog("MLB19698968".into()));
        assert_eq!(parse_link("https://www.mercadolivre.com.br/ssd-kingston/p/MLB19698968"), catalog());
        assert_eq!(parse_link("https://www.mercadolivre.com.br/ssd/p/MLB19698968#wid=MLB3456789012&sid=search"), catalog());
        assert_eq!(parse_link("https://produto.mercadolivre.com.br/MLB-3456789012-ssd-kingston-_JM"), Some(Item("MLB3456789012".into())));
        assert_eq!(parse_link("  mlb3456789012 "), Some(Item("MLB3456789012".into())));
        assert_eq!(parse_link("https://www.amazon.com.br/dp/B0ABCDEFGH"), None);
        assert_eq!(parse_nickname(r#"{"id":1,"nickname":"JEANK"}"#).as_deref(), Some("JEANK"));
    }
}
