//! Segredos no Gerenciador de Credenciais do Windows: ficam fora do banco, do backup e do git.

use keyring::{Entry, Error};

const SERVICE: &str = "com.jeank.radar";
pub const ML_SECRET: &str = "ml-client-secret";
pub const ML_REFRESH: &str = "ml-refresh-token";

fn entry(key: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, key).map_err(|e| format!("Gerenciador de Credenciais do Windows indisponível: {e}"))
}

pub fn get(key: &str) -> Result<Option<String>, String> {
    match entry(key)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Não foi possível ler do Gerenciador de Credenciais: {e}")),
    }
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    entry(key)?.set_password(value).map_err(|e| format!("Não foi possível gravar no Gerenciador de Credenciais: {e}"))
}

pub fn delete(key: &str) -> Result<(), String> {
    match entry(key)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Não foi possível apagar do Gerenciador de Credenciais: {e}")),
    }
}
