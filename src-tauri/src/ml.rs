//! Mercado Livre: leitura das respostas da API oficial e chamadas HTTP.

use serde::{Deserialize, Serialize};

use crate::prices::Reading;

pub const STORE: &str = "ml";
pub const SITE: &str = "MLB";
const API: &str = "https://api.mercadolibre.com";
/// Máximo de anúncios por consulta em lote.
pub const BATCH: usize = 20;
/// Campos pedidos na consulta em lote, para respostas menores.
pub const ITEM_ATTRS: &str = "id,title,price,original_price,status,available_quantity,permalink,thumbnail,shipping";

/// Um resultado de busca, já em centavos. Também é o que a interface manda de volta para "Acompanhar".
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
    pub seller: Option<String>,
}

/// Um anúncio consultado pelo código.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemInfo {
    pub code: String,
    pub title: String,
    pub url: String,
    pub image: Option<String>,
    pub reading: Reading,
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

#[derive(Deserialize, Default)]
struct Shipping {
    #[serde(default)]
    free_shipping: bool,
    logistic_type: Option<String>,
}

impl Shipping {
    /// "Full": o produto sai do armazém do Mercado Livre.
    fn full(&self) -> bool {
        self.logistic_type.as_deref() == Some("fulfillment")
    }
}

#[derive(Deserialize)]
struct SearchResp {
    results: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    id: String,
    title: String,
    price: Option<f64>,
    original_price: Option<f64>,
    permalink: String,
    thumbnail: Option<String>,
    shipping: Option<Shipping>,
    seller: Option<Seller>,
}

#[derive(Deserialize)]
struct Seller {
    nickname: Option<String>,
}

pub fn parse_search(json: &str) -> Result<Vec<Offer>, String> {
    let resp: SearchResp =
        serde_json::from_str(json).map_err(|e| format!("Resposta inesperada da busca do Mercado Livre: {e}"))?;
    Ok(resp
        .results
        .into_iter()
        .filter_map(|it| {
            let price = to_cents(it.price?);
            let ship = it.shipping.unwrap_or_default();
            Some(Offer {
                store: STORE.into(),
                code: it.id,
                title: it.title,
                url: it.permalink,
                image: https(it.thumbnail),
                price,
                list_price: list_price(price, it.original_price),
                free_shipping: ship.free_shipping,
                full: ship.full(),
                seller: it.seller.and_then(|s| s.nickname),
            })
        })
        .collect())
}

#[derive(Deserialize)]
struct BatchEntry {
    code: u16,
    body: serde_json::Value,
}

#[derive(Deserialize)]
struct ItemBody {
    id: String,
    title: String,
    price: Option<f64>,
    original_price: Option<f64>,
    status: String,
    available_quantity: Option<i64>,
    permalink: String,
    thumbnail: Option<String>,
    shipping: Option<Shipping>,
}

/// Consulta em lote: um resultado por código pedido, **na mesma ordem** do pedido.
pub fn parse_items(json: &str) -> Result<Vec<Result<ItemInfo, String>>, String> {
    let entries: Vec<BatchEntry> =
        serde_json::from_str(json).map_err(|e| format!("Resposta inesperada do Mercado Livre: {e}"))?;
    Ok(entries
        .into_iter()
        .map(|e| match e.code {
            200 => serde_json::from_value::<ItemBody>(e.body)
                .map(item_info)
                .map_err(|err| format!("Resposta inesperada do anúncio: {err}")),
            404 => Err("Anúncio não encontrado no Mercado Livre".into()),
            c => Err(format!("O Mercado Livre respondeu {c} para este anúncio")),
        })
        .collect())
}

fn item_info(b: ItemBody) -> ItemInfo {
    let price = b.price.map(to_cents);
    let ship = b.shipping.unwrap_or_default();
    let in_stock = b.status == "active" && b.available_quantity.map_or(true, |q| q > 0);
    ItemInfo {
        code: b.id,
        title: b.title,
        url: b.permalink,
        image: https(b.thumbnail),
        reading: Reading {
            price,
            list_price: price.and_then(|p| list_price(p, b.original_price)),
            in_stock,
            free_shipping: ship.free_shipping,
        },
    }
}

/// Preço de venda real, com promoções (`/items/{id}/sale_price`): (preço, preço "de").
pub fn parse_sale_price(json: &str) -> Option<(i64, Option<i64>)> {
    #[derive(Deserialize)]
    struct Sale {
        amount: Option<f64>,
        regular_amount: Option<f64>,
    }
    let s: Sale = serde_json::from_str(json).ok()?;
    let price = to_cents(s.amount?);
    Some((price, list_price(price, s.regular_amount)))
}

/// Anúncio que está vendendo numa página de catálogo (`/products/{id}`).
pub fn parse_buy_box_winner(json: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Winner {
        item_id: String,
    }
    #[derive(Deserialize)]
    struct Product {
        buy_box_winner: Option<Winner>,
    }
    serde_json::from_str::<Product>(json).ok()?.buy_box_winner.map(|w| w.item_id)
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
    /// Anúncio (MLB + dígitos), consultado direto.
    Item(String),
    /// Página de catálogo (`/p/MLB…`): é preciso descobrir qual anúncio está vendendo.
    Catalog(String),
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

/// Reconhece o que o usuário colou: link de anúncio, de catálogo ou só o código.
pub fn parse_link(input: &str) -> Option<LinkRef> {
    let s = input.trim().to_uppercase();
    // Catálogo com o anúncio escolhido: ...#wid=MLB123 ou ...item_id:MLB123
    for pat in ["WID=MLB", "ITEM_ID:MLB", "ITEM_ID%3AMLB"] {
        if let Some(d) = digits_after(&s, pat) {
            return Some(LinkRef::Item(format!("MLB{d}")));
        }
    }
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
    fn le_a_busca() {
        let offers = parse_search(include_str!("../tests/fixtures/ml/search.json")).unwrap();
        assert_eq!(offers.len(), 2, "anúncio sem preço fica de fora");
        let a = &offers[0];
        assert_eq!((a.code.as_str(), a.price, a.list_price), ("MLB3456789012", 38990, Some(49990)));
        assert!(a.free_shipping && a.full);
        assert_eq!(a.image.as_deref(), Some("https://http2.mlstatic.com/D_123456-MLA0000000000_012024-I.jpg"));
        assert_eq!(a.seller.as_deref(), Some("KINGSTON OFICIAL"));
        let b = &offers[1];
        assert_eq!((b.price, b.list_price, b.free_shipping, b.full, b.seller.clone()), (35900, None, false, false, None));
    }

    #[test]
    fn le_a_consulta_em_lote() {
        let items = parse_items(include_str!("../tests/fixtures/ml/items.json")).unwrap();
        assert_eq!(items.len(), 3);
        let a = items[0].as_ref().unwrap();
        assert_eq!(a.reading, Reading { price: Some(37990), list_price: Some(49990), in_stock: true, free_shipping: true });
        assert_eq!(a.image.as_deref(), Some("https://http2.mlstatic.com/D_1.jpg"));
        let b = items[1].as_ref().unwrap();
        assert!(!b.reading.in_stock, "anúncio pausado conta como sem estoque");
        assert_eq!(items[2].as_ref().unwrap_err(), "Anúncio não encontrado no Mercado Livre");
    }

    #[test]
    fn le_o_preco_de_venda_e_o_catalogo() {
        assert_eq!(parse_sale_price(include_str!("../tests/fixtures/ml/sale_price.json")), Some((36990, Some(49990))));
        assert_eq!(parse_sale_price(include_str!("../tests/fixtures/ml/sale_price_sem_promo.json")), Some((35900, None)));
        assert_eq!(parse_sale_price("{}"), None);
        assert_eq!(parse_buy_box_winner(include_str!("../tests/fixtures/ml/product.json")).as_deref(), Some("MLB3456789012"));
        assert_eq!(parse_buy_box_winner(r#"{"id":"MLB1","buy_box_winner":null}"#), None);
        assert_eq!(parse_nickname(r#"{"id":1,"nickname":"JEANK"}"#).as_deref(), Some("JEANK"));
    }

    #[test]
    fn reconhece_links_do_mercado_livre() {
        use LinkRef::*;
        let item = || Some(Item("MLB3456789012".into()));
        assert_eq!(parse_link("https://produto.mercadolivre.com.br/MLB-3456789012-ssd-kingston-_JM"), item());
        assert_eq!(parse_link("https://www.mercadolivre.com.br/ssd/p/MLB19698968#wid=MLB3456789012&sid=search"), item());
        assert_eq!(parse_link("https://www.mercadolivre.com.br/ssd/p/MLB19698968?pdp_filters=item_id:MLB3456789012"), item());
        assert_eq!(parse_link("  mlb3456789012 "), item());
        assert_eq!(parse_link("https://www.mercadolivre.com.br/ssd-kingston/p/MLB19698968"), Some(Catalog("MLB19698968".into())));
        assert_eq!(parse_link("https://www.amazon.com.br/dp/B0ABCDEFGH"), None);
    }
}
