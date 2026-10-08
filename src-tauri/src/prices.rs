//! Regras puras sobre o histórico de preços (sem banco nem rede, fáceis de testar).

use serde::{Deserialize, Serialize};

/// O que uma loja informou numa checagem. Valores em centavos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub price: Option<i64>,
    pub list_price: Option<i64>,
    pub in_stock: bool,
    pub free_shipping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricePoint {
    /// Data e hora local, `AAAA-MM-DDTHH:MM:SS`.
    pub at: String,
    #[serde(flatten)]
    pub reading: Reading,
}

fn day(at: &str) -> &str {
    at.get(..10).unwrap_or(at)
}

/// Grava um ponto novo só quando algo muda, ou no primeiro registro do dia
/// (assim o histórico fica leve e o gráfico ainda mostra que houve checagem).
pub fn should_record(last: Option<&PricePoint>, new: &Reading, now: &str) -> bool {
    match last {
        None => true,
        Some(p) => p.reading != *new || day(&p.at) != day(now),
    }
}

/// Preço em vigor no fim do dia `d` (o último ponto até aquele dia). Sem estoque não conta.
pub fn price_on(points: &[PricePoint], d: &str) -> Option<i64> {
    points
        .iter()
        .take_while(|p| day(&p.at) <= d)
        .last()
        .filter(|p| p.reading.in_stock)
        .and_then(|p| p.reading.price)
}

/// Menor preço entre os links em cada dia.
pub fn daily_best(links: &[Vec<PricePoint>], days: &[String]) -> Vec<Option<i64>> {
    days.iter().map(|d| links.iter().filter_map(|pts| price_on(pts, d)).min()).collect()
}

/// Menor preço já registrado com estoque.
pub fn lowest_ever(links: &[Vec<PricePoint>]) -> Option<i64> {
    links.iter().flatten().filter(|p| p.reading.in_stock).filter_map(|p| p.reading.price).min()
}

/// Preço atual de um link: o último ponto, se tiver estoque.
pub fn current(points: &[PricePoint]) -> Option<i64> {
    points.last().filter(|p| p.reading.in_stock).and_then(|p| p.reading.price)
}

/// Índice do link com o menor preço atual, e esse preço.
pub fn current_best(links: &[Vec<PricePoint>]) -> Option<(usize, i64)> {
    links
        .iter()
        .enumerate()
        .filter_map(|(i, pts)| current(pts).map(|p| (i, p)))
        .min_by_key(|&(_, p)| p)
}

/// Melhor preço no primeiro dia acompanhado: base da variação "desde que começou".
pub fn first_best(links: &[Vec<PricePoint>]) -> Option<i64> {
    let first = links.iter().filter_map(|pts| pts.first()).map(|p| day(&p.at).to_string()).min()?;
    daily_best(links, &[first])[0]
}

/// Os últimos `n` dias terminando em `today` (AAAA-MM-DD), do mais antigo para o mais recente.
pub fn last_days(today: &str, n: usize) -> Vec<String> {
    let end = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d").expect("data no formato AAAA-MM-DD");
    (0..n as i64).rev().map(|i| (end - chrono::Duration::days(i)).format("%Y-%m-%d").to_string()).collect()
}

/// Maior preço com estoque registrado a partir do dia `since` (inclusive).
pub fn max_since(points: &[PricePoint], since: &str) -> Option<i64> {
    points
        .iter()
        .filter(|p| day(&p.at) >= since && p.reading.in_stock)
        .filter_map(|p| p.reading.price)
        .max()
}

/// Dias desde o primeiro ponto até `today` (0 sem pontos).
pub fn days_tracked(points: &[PricePoint], today: &str) -> i64 {
    let parse = |d: &str| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
    match (points.first().and_then(|p| parse(day(&p.at))), parse(today)) {
        (Some(first), Some(t)) => (t - first).num_days(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maior_preco_e_dias_acompanhados() {
        let pts = vec![
            pt("2026-09-01T10:00:00", r(900)),
            pt("2026-09-20T10:00:00", r(700)),
            pt("2026-10-01T10:00:00", Reading { in_stock: false, ..r(999) }),
            pt("2026-10-02T10:00:00", r(650)),
        ];
        assert_eq!(max_since(&pts, "2026-09-08"), Some(700));
        assert_eq!(max_since(&pts, "2026-10-03"), None);
        assert_eq!(days_tracked(&pts, "2026-10-08"), 37);
        assert_eq!(days_tracked(&[], "2026-10-08"), 0);
    }

    fn r(price: i64) -> Reading {
        Reading { price: Some(price), list_price: None, in_stock: true, free_shipping: false }
    }
    fn pt(at: &str, reading: Reading) -> PricePoint {
        PricePoint { at: at.into(), reading }
    }

    #[test]
    fn grava_quando_muda_ou_quando_vira_o_dia() {
        let last = pt("2026-10-07T10:00:00", r(10000));
        assert!(should_record(None, &r(10000), "2026-10-07T11:00:00"));
        assert!(!should_record(Some(&last), &r(10000), "2026-10-07T13:00:00"));
        assert!(should_record(Some(&last), &r(9990), "2026-10-07T13:00:00"));
        assert!(should_record(Some(&last), &r(10000), "2026-10-08T08:00:00"));
        let sem_estoque = Reading { in_stock: false, ..r(10000) };
        assert!(should_record(Some(&last), &sem_estoque, "2026-10-07T13:00:00"));
    }

    #[test]
    fn melhor_preco_por_dia_entre_as_lojas() {
        let ml = vec![pt("2026-10-01T10:00:00", r(500)), pt("2026-10-03T10:00:00", r(450))];
        let amz = vec![
            pt("2026-10-02T09:00:00", r(480)),
            pt("2026-10-04T09:00:00", Reading { in_stock: false, ..r(480) }),
        ];
        let links = [ml, amz];
        let days = last_days("2026-10-04", 5);
        assert_eq!(days, ["2026-09-30", "2026-10-01", "2026-10-02", "2026-10-03", "2026-10-04"]);
        assert_eq!(daily_best(&links, &days), vec![None, Some(500), Some(480), Some(450), Some(450)]);
        assert_eq!(lowest_ever(&links), Some(450));
        assert_eq!(current_best(&links), Some((0, 450)));
        assert_eq!(first_best(&links), Some(500));
    }

    #[test]
    fn sem_pontos_nao_tem_preco() {
        let links: [Vec<PricePoint>; 1] = [vec![]];
        assert_eq!(current_best(&links), None);
        assert_eq!(lowest_ever(&links), None);
        assert_eq!(first_best(&links), None);
    }
}
