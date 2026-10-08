//! Banco SQLite local: produtos acompanhados, links por loja e histórico de preços.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;

use crate::alerts;
use crate::prices::{self, PricePoint, Reading};

/// Cada item roda uma vez, em ordem; `user_version` guarda quantos já rodaram.
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE products (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  target_price INTEGER,
  min_drop_pct REAL NOT NULL DEFAULT 5,
  notify_lowest INTEGER NOT NULL DEFAULT 1,
  archived INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);
CREATE TABLE links (
  id INTEGER PRIMARY KEY,
  product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  store TEXT NOT NULL,
  code TEXT NOT NULL,
  url TEXT NOT NULL,
  title TEXT NOT NULL,
  image TEXT,
  last_check TEXT,
  last_error TEXT,
  failures INTEGER NOT NULL DEFAULT 0,
  UNIQUE (store, code)
);
CREATE TABLE prices (
  id INTEGER PRIMARY KEY,
  link_id INTEGER NOT NULL REFERENCES links(id) ON DELETE CASCADE,
  at TEXT NOT NULL,
  price INTEGER,
  list_price INTEGER,
  in_stock INTEGER NOT NULL,
  free_shipping INTEGER NOT NULL
);
CREATE INDEX prices_link_at ON prices (link_id, at);
"#, r#"
CREATE TABLE alerts (
  id INTEGER PRIMARY KEY,
  product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  price_before INTEGER,
  price_after INTEGER NOT NULL,
  at TEXT NOT NULL,
  read INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX alerts_at ON alerts (at);
"#];

/// Dias do mini gráfico da lista.
pub const SPARK_DAYS: usize = 30;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let c = Connection::open(path)?;
    setup(&c)?;
    Ok(c)
}

#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    setup(&c).unwrap();
    c
}

fn setup(c: &Connection) -> rusqlite::Result<()> {
    c.pragma_update(None, "foreign_keys", true)?;
    let version: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        c.execute_batch(sql)?;
        c.pragma_update(None, "user_version", (i + 1) as i64)?;
    }
    Ok(())
}

/// Confere se o arquivo é um banco do Radar antes de restaurar.
pub fn check_backup(path: &Path) -> Result<(), String> {
    const NOT: &str = "Esse arquivo não é um backup do Radar de Preços";
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|_| NOT)?;
    let ok: bool = c
        .query_row(
            "SELECT count(*) = 3 FROM sqlite_master WHERE type = 'table' AND name IN ('products', 'links', 'prices')",
            [],
            |r| r.get(0),
        )
        .map_err(|_| NOT)?;
    if ok { Ok(()) } else { Err(NOT.into()) }
}

pub struct NewLink<'a> {
    pub store: &'a str,
    pub code: &'a str,
    pub url: &'a str,
    pub title: &'a str,
    pub image: Option<&'a str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkInfo {
    pub id: i64,
    pub store: String,
    pub code: String,
    pub url: String,
    pub title: String,
    pub image: Option<String>,
    pub last_check: Option<String>,
    pub last_error: Option<String>,
    pub failures: i64,
    /// Última leitura registrada.
    pub current: Option<Reading>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSummary {
    pub id: i64,
    pub name: String,
    pub target_price: Option<i64>,
    pub links: Vec<LinkInfo>,
    pub best_price: Option<i64>,
    pub best_store: Option<String>,
    pub lowest_ever: Option<i64>,
    /// Melhor preço no primeiro dia acompanhado.
    pub first_price: Option<i64>,
    /// Melhor preço de cada um dos últimos `SPARK_DAYS` dias.
    pub spark: Vec<Option<i64>>,
    pub last_check: Option<String>,
    pub min_drop_pct: f64,
    pub notify_lowest: bool,
    /// Maior preço da melhor oferta nos últimos 30 dias.
    pub max_30d: Option<i64>,
    /// O preço "de" da melhor oferta parece inflado (ver `alerts::inflated`).
    pub inflated: bool,
}

/// Linha da tabela de produtos com as regras de alerta.
struct ProductRow {
    id: i64,
    name: String,
    target_price: Option<i64>,
    min_drop_pct: f64,
    notify_lowest: bool,
}

const PRODUCT_COLS: &str = "id, name, target_price, min_drop_pct, notify_lowest";

fn row_to_product(r: &rusqlite::Row) -> rusqlite::Result<ProductRow> {
    Ok(ProductRow { id: r.get(0)?, name: r.get(1)?, target_price: r.get(2)?, min_drop_pct: r.get(3)?, notify_lowest: r.get(4)? })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertRow {
    pub id: i64,
    pub product_id: i64,
    pub product_name: String,
    pub kind: String,
    pub price_before: Option<i64>,
    pub price_after: i64,
    pub at: String,
    pub read: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkHistory {
    pub link_id: i64,
    pub store: String,
    pub points: Vec<PricePoint>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductDetail {
    pub product: ProductSummary,
    pub history: Vec<LinkHistory>,
}

pub fn add_product(c: &Connection, name: &str, now: &str) -> rusqlite::Result<i64> {
    c.execute("INSERT INTO products (name, created_at) VALUES (?1, ?2)", params![name.trim(), now])?;
    Ok(c.last_insert_rowid())
}

pub fn add_link(c: &Connection, product_id: i64, l: &NewLink) -> Result<i64, String> {
    c.execute(
        "INSERT INTO links (product_id, store, code, url, title, image) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![product_id, l.store, l.code, l.url, l.title, l.image],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE => {
            "Esse anúncio já está sendo acompanhado".to_string()
        }
        e => e.to_string(),
    })?;
    Ok(c.last_insert_rowid())
}

fn row_to_point(r: &rusqlite::Row) -> rusqlite::Result<PricePoint> {
    Ok(PricePoint {
        at: r.get(0)?,
        reading: Reading { price: r.get(1)?, list_price: r.get(2)?, in_stock: r.get(3)?, free_shipping: r.get(4)? },
    })
}

const POINT_COLS: &str = "at, price, list_price, in_stock, free_shipping";

fn history(c: &Connection, link_id: i64) -> rusqlite::Result<Vec<PricePoint>> {
    let mut stmt = c.prepare(&format!("SELECT {POINT_COLS} FROM prices WHERE link_id = ?1 ORDER BY at, id"))?;
    let rows = stmt.query_map([link_id], row_to_point)?;
    rows.collect()
}

fn last_point(c: &Connection, link_id: i64) -> rusqlite::Result<Option<PricePoint>> {
    c.query_row(
        &format!("SELECT {POINT_COLS} FROM prices WHERE link_id = ?1 ORDER BY at DESC, id DESC LIMIT 1"),
        [link_id],
        row_to_point,
    )
    .optional()
}

/// Registra uma checagem bem-sucedida e zera as falhas.
/// Devolve true se a leitura mudou em relação à anterior (ou se é a primeira).
pub fn record_reading(c: &Connection, link_id: i64, r: &Reading, now: &str) -> rusqlite::Result<bool> {
    let last = last_point(c, link_id)?;
    if prices::should_record(last.as_ref(), r, now) {
        c.execute(
            "INSERT INTO prices (link_id, at, price, list_price, in_stock, free_shipping) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![link_id, now, r.price, r.list_price, r.in_stock, r.free_shipping],
        )?;
    }
    c.execute("UPDATE links SET last_check = ?2, last_error = NULL, failures = 0 WHERE id = ?1", params![link_id, now])?;
    Ok(last.map_or(true, |p| p.reading != *r))
}

pub fn record_failure(c: &Connection, link_id: i64, msg: &str, now: &str) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE links SET last_check = ?2, last_error = ?3, failures = failures + 1 WHERE id = ?1",
        params![link_id, now, msg],
    )?;
    Ok(())
}

/// Links de uma loja em produtos ativos, agrupados por produto: (produto, [(link, código na loja)]).
pub fn products_to_check(c: &Connection, store: &str) -> rusqlite::Result<Vec<(i64, Vec<(i64, String)>)>> {
    let mut stmt = c.prepare(
        "SELECT l.product_id, l.id, l.code FROM links l JOIN products p ON p.id = l.product_id
         WHERE l.store = ?1 AND p.archived = 0 ORDER BY l.product_id, l.id",
    )?;
    let rows = stmt.query_map([store], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
    let mut out: Vec<(i64, Vec<(i64, String)>)> = Vec::new();
    for row in rows {
        let (product, link, code) = row?;
        match out.last_mut() {
            Some((p, links)) if *p == product => links.push((link, code)),
            _ => out.push((product, vec![(link, code)])),
        }
    }
    Ok(out)
}

/// Preço da última leitura do link (None se não houver ou se estava sem oferta).
pub fn last_price(c: &Connection, link_id: i64) -> rusqlite::Result<Option<i64>> {
    Ok(last_point(c, link_id)?.filter(|p| p.reading.in_stock).and_then(|p| p.reading.price))
}

/// Estado e regras do produto antes de uma rodada de checagem: (nome, estado, regras).
pub fn alert_context(c: &Connection, id: i64, today: &str) -> rusqlite::Result<(String, alerts::Snapshot, alerts::Rules)> {
    let row = c.query_row(&format!("SELECT {PRODUCT_COLS} FROM products WHERE id = ?1"), [id], row_to_product)?;
    let rules = alerts::Rules { target: row.target_price, min_drop_pct: row.min_drop_pct, notify_lowest: row.notify_lowest };
    let (s, hist) = summarize(c, row, today)?;
    let snapshot = alerts::Snapshot { best: s.best_price, lowest_ever: s.lowest_ever, has_history: hist.iter().any(|h| !h.is_empty()) };
    Ok((s.name, snapshot, rules))
}

/// Melhor preço atual do produto (depois da rodada).
pub fn best_price(c: &Connection, id: i64, today: &str) -> rusqlite::Result<Option<i64>> {
    let row = c.query_row(&format!("SELECT {PRODUCT_COLS} FROM products WHERE id = ?1"), [id], row_to_product)?;
    Ok(summarize(c, row, today)?.0.best_price)
}

pub fn update_rules(c: &Connection, id: i64, target: Option<i64>, min_drop_pct: f64, notify_lowest: bool) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE products SET target_price = ?2, min_drop_pct = ?3, notify_lowest = ?4 WHERE id = ?1",
        params![id, target, min_drop_pct, notify_lowest],
    )?;
    Ok(())
}

pub fn add_alert(c: &Connection, product_id: i64, kind: alerts::Kind, before: Option<i64>, after: i64, now: &str) -> rusqlite::Result<i64> {
    let kind = serde_json::to_value(kind).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    c.execute(
        "INSERT INTO alerts (product_id, kind, price_before, price_after, at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![product_id, kind, before, after, now],
    )?;
    Ok(c.last_insert_rowid())
}

/// Alertas mais recentes primeiro.
pub fn list_alerts(c: &Connection, limit: i64) -> rusqlite::Result<Vec<AlertRow>> {
    let mut stmt = c.prepare(
        "SELECT a.id, a.product_id, p.name, a.kind, a.price_before, a.price_after, a.at, a.read
         FROM alerts a JOIN products p ON p.id = a.product_id ORDER BY a.at DESC, a.id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| {
        Ok(AlertRow {
            id: r.get(0)?,
            product_id: r.get(1)?,
            product_name: r.get(2)?,
            kind: r.get(3)?,
            price_before: r.get(4)?,
            price_after: r.get(5)?,
            at: r.get(6)?,
            read: r.get(7)?,
        })
    })?;
    rows.collect()
}

pub fn unread_alerts(c: &Connection) -> rusqlite::Result<i64> {
    c.query_row("SELECT count(*) FROM alerts WHERE read = 0", [], |r| r.get(0))
}

pub fn mark_alerts_read(c: &Connection) -> rusqlite::Result<()> {
    c.execute("UPDATE alerts SET read = 1 WHERE read = 0", [])?;
    Ok(())
}

fn summarize(c: &Connection, row: ProductRow, today: &str) -> rusqlite::Result<(ProductSummary, Vec<Vec<PricePoint>>)> {
    let ProductRow { id, name, target_price, min_drop_pct, notify_lowest } = row;
    let mut stmt = c.prepare(
        "SELECT id, store, code, url, title, image, last_check, last_error, failures
         FROM links WHERE product_id = ?1 ORDER BY id",
    )?;
    let mut links: Vec<LinkInfo> = stmt
        .query_map([id], |r| {
            Ok(LinkInfo {
                id: r.get(0)?,
                store: r.get(1)?,
                code: r.get(2)?,
                url: r.get(3)?,
                title: r.get(4)?,
                image: r.get(5)?,
                last_check: r.get(6)?,
                last_error: r.get(7)?,
                failures: r.get(8)?,
                current: None,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let hist = links.iter().map(|l| history(c, l.id)).collect::<rusqlite::Result<Vec<_>>>()?;
    for (l, h) in links.iter_mut().zip(&hist) {
        l.current = h.last().map(|p| p.reading.clone());
    }
    let best = prices::current_best(&hist);
    // Promoção inflada: o "de" da melhor oferta contra o que essa oferta já cobrou nos últimos 30 dias.
    let (max_30d, inflated) = match best {
        Some((i, _)) => {
            let since = prices::last_days(today, 31)[0].clone();
            let max = prices::max_since(&hist[i], &since);
            let list = hist[i].last().and_then(|p| p.reading.list_price);
            (max, alerts::inflated(list, max, prices::days_tracked(&hist[i], today)))
        }
        None => (None, false),
    };
    let summary = ProductSummary {
        min_drop_pct,
        notify_lowest,
        max_30d,
        inflated,
        id,
        name,
        target_price,
        best_price: best.map(|(_, p)| p),
        best_store: best.map(|(i, _)| links[i].store.clone()),
        lowest_ever: prices::lowest_ever(&hist),
        first_price: prices::first_best(&hist),
        spark: prices::daily_best(&hist, &prices::last_days(today, SPARK_DAYS)),
        last_check: links.iter().filter_map(|l| l.last_check.clone()).max(),
        links,
    };
    Ok((summary, hist))
}

pub fn list_products(c: &Connection, today: &str) -> rusqlite::Result<Vec<ProductSummary>> {
    let mut stmt = c.prepare(&format!("SELECT {PRODUCT_COLS} FROM products WHERE archived = 0 ORDER BY name COLLATE NOCASE"))?;
    let rows = stmt.query_map([], row_to_product)?.collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|row| summarize(c, row, today).map(|(s, _)| s)).collect()
}

pub fn product_detail(c: &Connection, id: i64, today: &str) -> rusqlite::Result<ProductDetail> {
    let row = c.query_row(&format!("SELECT {PRODUCT_COLS} FROM products WHERE id = ?1"), [id], row_to_product)?;
    let (product, hist) = summarize(c, row, today)?;
    let history = product
        .links
        .iter()
        .zip(hist)
        .map(|(l, points)| LinkHistory { link_id: l.id, store: l.store.clone(), points })
        .collect();
    Ok(ProductDetail { product, history })
}

pub fn rename_product(c: &Connection, id: i64, name: &str) -> rusqlite::Result<()> {
    c.execute("UPDATE products SET name = ?2 WHERE id = ?1", params![id, name.trim()])?;
    Ok(())
}

pub fn delete_product(c: &Connection, id: i64) -> rusqlite::Result<()> {
    c.execute("DELETE FROM products WHERE id = ?1", [id])?;
    Ok(())
}

/// Remove um link. Se era o último do produto, o produto sai também.
pub fn delete_link(c: &Connection, link_id: i64) -> rusqlite::Result<()> {
    let product: Option<i64> = c.query_row("SELECT product_id FROM links WHERE id = ?1", [link_id], |r| r.get(0)).optional()?;
    c.execute("DELETE FROM links WHERE id = ?1", [link_id])?;
    if let Some(p) = product {
        c.execute("DELETE FROM products WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM links WHERE product_id = ?1)", [p])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(price: i64) -> Reading {
        Reading { price: Some(price), list_price: None, in_stock: true, free_shipping: true }
    }
    fn link(code: &'static str) -> NewLink<'static> {
        NewLink { store: "ml", code, url: "https://produto.mercadolivre.com.br/x", title: "SSD 1TB", image: None }
    }
    fn count(c: &Connection, table: &str) -> i64 {
        c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn guarda_historico_so_quando_muda() {
        let c = open_in_memory();
        let p = add_product(&c, "SSD", "2026-10-01T08:00:00").unwrap();
        let l = add_link(&c, p, &link("MLB1")).unwrap();
        assert!(record_reading(&c, l, &r(500), "2026-10-01T08:00:00").unwrap());
        assert!(!record_reading(&c, l, &r(500), "2026-10-01T11:00:00").unwrap());
        assert!(record_reading(&c, l, &r(450), "2026-10-01T14:00:00").unwrap());
        assert!(!record_reading(&c, l, &r(450), "2026-10-02T08:00:00").unwrap()); // mesmo preço, mas é o ponto do dia
        assert_eq!(count(&c, "prices"), 3);
    }

    #[test]
    fn resumo_do_produto_com_dois_links() {
        let c = open_in_memory();
        let p = add_product(&c, "SSD Kingston", "2026-10-01T08:00:00").unwrap();
        let a = add_link(&c, p, &link("MLB1")).unwrap();
        let b = add_link(&c, p, &link("MLB2")).unwrap();
        record_reading(&c, a, &r(500), "2026-10-01T08:00:00").unwrap();
        record_reading(&c, b, &r(480), "2026-10-01T08:00:00").unwrap();
        record_reading(&c, a, &r(450), "2026-10-02T08:00:00").unwrap();

        let list = list_products(&c, "2026-10-02").unwrap();
        assert_eq!(list.len(), 1);
        let s = &list[0];
        assert_eq!((s.best_price, s.best_store.as_deref()), (Some(450), Some("ml")));
        assert_eq!((s.lowest_ever, s.first_price), (Some(450), Some(480)));
        assert_eq!(s.spark.len(), SPARK_DAYS);
        assert_eq!(&s.spark[SPARK_DAYS - 2..], &[Some(480), Some(450)]);
        assert_eq!(s.links[0].current.as_ref().and_then(|r| r.price), Some(450));

        let d = product_detail(&c, p, "2026-10-02").unwrap();
        assert_eq!(d.history.len(), 2);
        assert_eq!(d.history[0].points.len(), 2);
    }

    #[test]
    fn link_repetido_falhas_e_exclusao() {
        let c = open_in_memory();
        let p = add_product(&c, "SSD", "2026-10-01T08:00:00").unwrap();
        let l = add_link(&c, p, &link("MLB1")).unwrap();
        assert_eq!(add_link(&c, p, &link("MLB1")).unwrap_err(), "Esse anúncio já está sendo acompanhado");

        record_failure(&c, l, "fora do ar", "2026-10-01T09:00:00").unwrap();
        record_failure(&c, l, "fora do ar", "2026-10-01T10:00:00").unwrap();
        let s = &list_products(&c, "2026-10-01").unwrap()[0];
        assert_eq!((s.links[0].failures, s.links[0].last_error.as_deref()), (2, Some("fora do ar")));
        record_reading(&c, l, &r(500), "2026-10-01T11:00:00").unwrap();
        assert_eq!(list_products(&c, "2026-10-01").unwrap()[0].links[0].failures, 0);

        delete_link(&c, l).unwrap();
        assert_eq!((count(&c, "products"), count(&c, "prices")), (0, 0), "sem links, o produto some junto com o histórico");
    }

    #[test]
    fn regras_alertas_e_agrupamento() {
        let c = open_in_memory();
        let p = add_product(&c, "SSD", "2026-10-01T08:00:00").unwrap();
        let q = add_product(&c, "Air Fryer", "2026-10-01T08:00:00").unwrap();
        let a = add_link(&c, p, &link("MLB1")).unwrap();
        let b = add_link(&c, p, &link("MLB2")).unwrap();
        let x = add_link(&c, q, &link("MLB3")).unwrap();
        let groups = products_to_check(&c, "ml").unwrap();
        assert_eq!(groups, vec![(p, vec![(a, "MLB1".into()), (b, "MLB2".into())]), (q, vec![(x, "MLB3".into())])]);

        let (_, snap, rules) = alert_context(&c, p, "2026-10-01").unwrap();
        assert!(!snap.has_history);
        assert_eq!((rules.target, rules.min_drop_pct, rules.notify_lowest), (None, 5.0, true));
        update_rules(&c, p, Some(40000), 10.0, false).unwrap();
        record_reading(&c, a, &r(45000), "2026-10-01T08:00:00").unwrap();
        let (name, snap, rules) = alert_context(&c, p, "2026-10-01").unwrap();
        assert_eq!((name.as_str(), snap.best, snap.has_history), ("SSD", Some(45000), true));
        assert_eq!((rules.target, rules.min_drop_pct, rules.notify_lowest), (Some(40000), 10.0, false));
        assert_eq!(last_price(&c, a).unwrap(), Some(45000));
        assert_eq!(last_price(&c, b).unwrap(), None);

        add_alert(&c, p, alerts::Kind::Drop, Some(50000), 45000, "2026-10-01T09:00:00").unwrap();
        add_alert(&c, q, alerts::Kind::BackInStock, None, 30000, "2026-10-01T10:00:00").unwrap();
        assert_eq!(unread_alerts(&c).unwrap(), 2);
        let list = list_alerts(&c, 10).unwrap();
        assert_eq!((list[0].product_name.as_str(), list[0].kind.as_str()), ("Air Fryer", "back_in_stock"));
        assert_eq!((list[1].kind.as_str(), list[1].price_before, list[1].read), ("drop", Some(50000), false));
        mark_alerts_read(&c).unwrap();
        assert_eq!(unread_alerts(&c).unwrap(), 0);
        delete_product(&c, q).unwrap();
        assert_eq!(list_alerts(&c, 10).unwrap().len(), 1, "alertas saem junto com o produto");
    }

    #[test]
    fn detecta_promocao_inflada() {
        let c = open_in_memory();
        let p = add_product(&c, "SSD", "2026-09-20T08:00:00").unwrap();
        let l = add_link(&c, p, &link("MLB1")).unwrap();
        record_reading(&c, l, &r(50000), "2026-09-20T08:00:00").unwrap();
        record_reading(&c, l, &Reading { list_price: Some(79900), ..r(49900) }, "2026-10-01T08:00:00").unwrap();
        let s = &list_products(&c, "2026-10-01").unwrap()[0];
        assert_eq!((s.max_30d, s.inflated), (Some(50000), true), "de R$ 799 nunca foi cobrado");
        record_reading(&c, l, &Reading { list_price: Some(52000), ..r(49900) }, "2026-10-01T09:00:00").unwrap();
        assert!(!list_products(&c, "2026-10-01").unwrap()[0].inflated, "4% acima é tolerado");
    }

    #[test]
    fn reconhece_um_backup_valido() {
        let c = open_in_memory();
        let dir = std::env::temp_dir().join(format!("radar-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("ok.db");
        let _ = std::fs::remove_file(&ok);
        c.execute("VACUUM INTO ?1", [ok.to_string_lossy().into_owned()]).unwrap();
        assert!(check_backup(&ok).is_ok());
        let bad = dir.join("bad.db");
        std::fs::write(&bad, b"nao sou um banco").unwrap();
        assert!(check_backup(&bad).is_err());
    }
}
