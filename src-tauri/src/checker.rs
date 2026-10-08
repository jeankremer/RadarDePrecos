//! Checagem de preços: a rodada única (usada por "Checar agora" e pela automática), a leitura suspeita,
//! os alertas e o agendador em segundo plano.

use std::{
    collections::{HashMap, HashSet},
    sync::atomic::Ordering,
    time::Duration,
};

use rand::Rng;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::{alerts, db, err, lock, ml, now, now_secs, product_offers, today, AppState, CmdResult};

/// Pausa entre produtos na checagem automática (parece uso normal e reduz bloqueios).
const SPREAD_SECS: std::ops::RangeInclusive<u64> = 5..=15;
/// Espera antes de checar de novo um preço suspeito.
const RECHECK_SECS: i64 = 10 * 60;
/// Teto do intervalo quando a loja está falhando.
const MAX_INTERVAL_HOURS: u32 = 24;

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CheckSummary {
    pub checked: usize,
    pub changed: usize,
    pub failed: usize,
    pub alerts: usize,
    /// Leituras suspeitas guardadas para confirmar depois.
    pub suspicious: usize,
}

/// Preço suspeito à espera de confirmação: (preço, quando foi lido em segundos Unix).
pub type Pending = HashMap<i64, (i64, i64)>;

/// Uma rodada de checagem. `spread` põe pausas entre produtos; `only` limita a esses links.
pub async fn run_check(app: &AppHandle, spread: bool, only: Option<HashSet<i64>>) -> CmdResult<CheckSummary> {
    let state = app.state::<AppState>();
    if state.checking.swap(true, Ordering::SeqCst) {
        return Err("Já existe uma checagem em andamento".into());
    }
    let result = check_all(app, &state, spread, only.as_ref()).await;
    state.checking.store(false, Ordering::SeqCst);
    if let Ok(sum) = &result {
        let _ = app.emit("checked", sum);
        let unread = db::unread_alerts(&lock(&state.db)).unwrap_or(0);
        let _ = app.emit("alerts", unread);
        crate::update_tray(app);
    }
    result
}

async fn check_all(app: &AppHandle, state: &AppState, spread: bool, only: Option<&HashSet<i64>>) -> CmdResult<CheckSummary> {
    let mut groups = db::products_to_check(&lock(&state.db), ml::STORE).map_err(err)?;
    if let Some(only) = only {
        for (_, links) in &mut groups {
            links.retain(|(id, _)| only.contains(id));
        }
        groups.retain(|(_, links)| !links.is_empty());
    }
    let mut sum = CheckSummary::default();
    let notify = crate::load_config(app).notify;
    for (i, (product_id, links)) in groups.iter().enumerate() {
        if spread && i > 0 {
            let secs = rand::thread_rng().gen_range(SPREAD_SECS);
            tokio::time::sleep(Duration::from_secs(secs)).await;
        }
        let (name, before, rules) = db::alert_context(&lock(&state.db), *product_id, &today()).map_err(err)?;
        let mut any_ok = false;
        for (link_id, code) in links {
            match product_offers(app, code).await {
                Ok(best) => {
                    record(state, *link_id, ml::reading(best.as_ref()), &mut sum)?;
                    any_ok = true;
                }
                Err(e) => {
                    db::record_failure(&lock(&state.db), *link_id, &e.to_string(), &now()).map_err(err)?;
                    sum.failed += 1;
                }
            }
        }
        if !any_ok {
            continue;
        }
        let after = db::best_price(&lock(&state.db), *product_id, &today()).map_err(err)?;
        if let (Some(kind), Some(price)) = (alerts::evaluate(&before, after, &rules), after) {
            db::add_alert(&lock(&state.db), *product_id, kind, before.best, price, &now()).map_err(err)?;
            sum.alerts += 1;
            if notify {
                let _ = app
                    .notification()
                    .builder()
                    .title("Radar de Preços")
                    .body(alerts::message(kind, &name, before.best, price, &rules))
                    .show();
            }
        }
    }
    crate::after_change(app, state);
    Ok(sum)
}

/// Grava a leitura, a não ser que seja uma queda suspeita: essa fica guardada até uma segunda leitura confirmar.
fn record(state: &AppState, link_id: i64, reading: crate::prices::Reading, sum: &mut CheckSummary) -> CmdResult<()> {
    sum.checked += 1;
    let conn = lock(&state.db);
    let prev = db::last_price(&conn, link_id).map_err(err)?;
    let mut pending = lock(&state.pending);
    let confirmed = pending.remove(&link_id).is_some_and(|(price, _)| alerts::confirms(price, reading.price));
    if !confirmed && alerts::suspicious(prev, reading.price) {
        if let Some(price) = reading.price {
            pending.insert(link_id, (price, now_secs()));
        }
        sum.suspicious += 1;
        return Ok(());
    }
    if db::record_reading(&conn, link_id, &reading, &now()).map_err(err)? {
        sum.changed += 1;
    }
    Ok(())
}

/// Intervalo atual em horas: o configurado, dobrado a cada rodada que falhou inteira (até 24 h).
fn effective_interval(hours: u32, backoff: u32) -> u32 {
    hours.saturating_mul(1 << backoff.min(5)).min(MAX_INTERVAL_HOURS.max(hours))
}

/// Se a checagem automática está atrasada. `last` é a data e hora local da última.
fn due(last: Option<&str>, hours: u32, backoff: u32) -> bool {
    if hours == 0 {
        return false;
    }
    let Some(last) = last.and_then(|l| chrono::NaiveDateTime::parse_from_str(l, "%Y-%m-%dT%H:%M:%S").ok()) else {
        return true;
    };
    let next = last + chrono::Duration::hours(effective_interval(hours, backoff) as i64);
    chrono::Local::now().naive_local() >= next
}

/// Laço em segundo plano: a cada minuto vê se está na hora da checagem automática e se há suspeitas a confirmar.
pub async fn scheduler(app: AppHandle) {
    tokio::time::sleep(Duration::from_secs(30)).await;
    loop {
        let state = app.state::<AppState>();
        let cfg = crate::load_config(&app);
        let backoff = state.backoff.load(Ordering::SeqCst);
        if due(cfg.last_auto_check.as_deref(), cfg.check_interval_hours, backoff) {
            if let Ok(sum) = run_check(&app, true, None).await {
                let all_failed = sum.failed > 0 && sum.checked == 0;
                state.backoff.store(if all_failed { backoff + 1 } else { 0 }, Ordering::SeqCst);
                let mut cfg = crate::load_config(&app);
                cfg.last_auto_check = Some(now());
                let _ = crate::save_config(&app, &cfg);
            }
        }
        let ripe: HashSet<i64> = lock(&state.pending)
            .iter()
            .filter(|(_, (_, since))| now_secs() - since >= RECHECK_SECS)
            .map(|(id, _)| *id)
            .collect();
        if !ripe.is_empty() {
            let _ = run_check(&app, false, Some(ripe)).await;
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervalo_dobra_ate_24h() {
        assert_eq!(effective_interval(3, 0), 3);
        assert_eq!(effective_interval(3, 1), 6);
        assert_eq!(effective_interval(3, 3), 24);
        assert_eq!(effective_interval(3, 9), 24);
        assert_eq!(effective_interval(24, 2), 24);
    }

    #[test]
    fn hora_da_checagem() {
        assert!(due(None, 3, 0), "nunca checou");
        assert!(!due(None, 0, 0), "desligada");
        let agora = chrono::Local::now().naive_local();
        let fmt = |d: chrono::NaiveDateTime| d.format("%Y-%m-%dT%H:%M:%S").to_string();
        assert!(!due(Some(&fmt(agora - chrono::Duration::hours(1))), 3, 0));
        assert!(due(Some(&fmt(agora - chrono::Duration::hours(4))), 3, 0));
        assert!(!due(Some(&fmt(agora - chrono::Duration::hours(4))), 3, 1), "com espera, 6 h");
    }
}
