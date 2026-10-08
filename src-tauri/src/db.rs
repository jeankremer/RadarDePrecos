//! Banco SQLite local: produtos acompanhados, links por loja e histórico de preços.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;

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

/// Links de uma loja em produtos ativos: (id do link, código na loja).
pub fn links_to_check(c: &Connection, store: &str) -> rusqlite::Result<Vec<(i64, String)>> {
    let mut stmt = c.prepare(
        "SELECT l.id, l.code FROM links l JOIN products p ON p.id = l.product_id
         WHERE l.store = ?1 AND p.archived = 0 ORDER BY l.id",
    )?;
    let rows = stmt.query_map([store], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

fn summarize(
    c: &Connection,
    id: i64,
    name: String,
    target_price: Option<i64>,
    today: &str,
) -> rusqlite::Result<(ProductSummary, Vec<Vec<PricePoint>>)> {
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
    let summary = ProductSummary {
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
    let mut stmt = c.prepare("SELECT id, name, target_price FROM products WHERE archived = 0 ORDER BY name COLLATE NOCASE")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|(id, name, tp)| summarize(c, id, name, tp, today).map(|(s, _)| s)).collect()
}

pub fn product_detail(c: &Connection, id: i64, today: &str) -> rusqlite::Result<ProductDetail> {
    let (name, tp) = c.query_row("SELECT name, target_price FROM products WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let (product, hist) = summarize(c, id, name, tp, today)?;
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
