//! Backup do banco. Uma cópia por dia (`radar-AAAA-MM-DD.db`), sobrescrita a cada mudança daquele dia,
//! mantendo as `KEEP` mais recentes. A chave do Mercado Livre não está no banco, então a cópia pode ir para a nuvem.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

pub const KEEP: usize = 30;
const PREFIX: &str = "radar-";
const EXT: &str = ".db";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub dir: Option<String>,
    /// Data (AAAA-MM-DD) do backup mais recente.
    pub last: Option<String>,
    pub count: usize,
    pub error: Option<String>,
    /// Pasta sugerida dentro do OneDrive, se ele estiver configurado no Windows.
    pub suggestion: Option<String>,
}

fn is_backup_name(name: &str) -> bool {
    name.len() == PREFIX.len() + 10 + EXT.len()
        && name.starts_with(PREFIX)
        && name.ends_with(EXT)
        && name[PREFIX.len()..PREFIX.len() + 10].chars().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() }
        })
}

/// Backups da pasta, do mais recente para o mais antigo.
pub fn list(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(is_backup_name))
        .collect();
    // O nome tem a data em formato ordenável.
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    Ok(files)
}

/// Copia o banco para `dir` com a data informada e apaga os backups além de `keep`.
/// `VACUUM INTO` faz uma cópia consistente mesmo com o banco aberto.
pub fn run(db: &rusqlite::Connection, dir: &Path, date: &str, keep: usize) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("Não foi possível acessar a pasta de backup: {e}"))?;
    let target = dir.join(format!("{PREFIX}{date}{EXT}"));
    let tmp = dir.join(format!("{PREFIX}{date}.tmp"));
    let _ = fs::remove_file(&tmp); // VACUUM INTO não sobrescreve
    db.execute("VACUUM INTO ?1", [tmp.to_string_lossy().into_owned()])
        .map_err(|e| format!("Não foi possível gravar o backup: {e}"))?;
    fs::rename(&tmp, &target).map_err(|e| format!("Não foi possível gravar o backup: {e}"))?;

    for old in list(dir).map_err(|e| e.to_string())?.into_iter().skip(keep) {
        let _ = fs::remove_file(old);
    }
    Ok(())
}

pub fn status(dir: Option<&str>) -> BackupStatus {
    let suggestion = std::env::var("OneDrive")
        .ok()
        .map(|od| Path::new(&od).join("Backups").join("Radar de Precos").display().to_string());
    let Some(d) = dir else {
        return BackupStatus { dir: None, last: None, count: 0, error: None, suggestion };
    };
    match list(Path::new(d)) {
        Ok(files) => BackupStatus {
            dir: Some(d.to_string()),
            last: files.first().and_then(|p| p.file_name()).and_then(|n| n.to_str()).map(|n| n[PREFIX.len()..PREFIX.len() + 10].to_string()),
            count: files.len(),
            error: None,
            suggestion,
        },
        Err(e) => BackupStatus {
            dir: Some(d.to_string()),
            last: None,
            count: 0,
            error: Some(format!("Pasta inacessível: {e}")),
            suggestion,
        },
    }
}

/// Garante que a pasta existe e aceita gravação.
pub fn check_writable(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("Não foi possível criar a pasta: {e}"))?;
    let probe = dir.join(".radar-teste");
    fs::write(&probe, b"ok").map_err(|e| format!("Sem permissão para gravar na pasta: {e}"))?;
    let _ = fs::remove_file(probe);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("radar-backup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reconhece_somente_nomes_de_backup() {
        assert!(is_backup_name("radar-2026-10-06.db"));
        assert!(!is_backup_name("radar-2026-10-6.db"));
        assert!(!is_backup_name("radar.db"));
        assert!(!is_backup_name("radar-antes-restauracao.db"));
        assert!(!is_backup_name("foto-2026-10-06.db"));
    }

    #[test]
    fn um_arquivo_por_dia_e_mantem_os_mais_recentes() {
        let base = temp_dir("rotacao");
        let dir = base.join("backups");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("outro-arquivo.txt"), b"nao mexer").unwrap();
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);").unwrap();

        for day in 1..=5 {
            run(&db, &dir, &format!("2026-10-0{day}"), 3).unwrap();
        }
        db.execute("INSERT INTO t VALUES (2)", []).unwrap();
        run(&db, &dir, "2026-10-05", 3).unwrap(); // mesmo dia: sobrescreve

        let names: Vec<String> = list(&dir).unwrap().iter().map(|p| p.file_name().unwrap().to_string_lossy().into()).collect();
        assert_eq!(names, ["radar-2026-10-05.db", "radar-2026-10-04.db", "radar-2026-10-03.db"]);
        let copy = rusqlite::Connection::open(dir.join("radar-2026-10-05.db")).unwrap();
        let n: i64 = copy.query_row("SELECT count(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "o backup do dia tem a versão mais recente");
        assert!(dir.join("outro-arquivo.txt").exists(), "não pode apagar arquivos que não são backup");
        assert_eq!(status(Some(dir.to_str().unwrap())).last.as_deref(), Some("2026-10-05"));
    }
}
