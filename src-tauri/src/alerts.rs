//! Regras de alerta e de leitura suspeita (puras, sem banco nem rede).

use serde::Serialize;

/// Como o produto estava antes da rodada de checagem.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub best: Option<i64>,
    pub lowest_ever: Option<i64>,
    pub has_history: bool,
}

/// Regras do produto.
#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    pub target: Option<i64>,
    pub min_drop_pct: f64,
    pub notify_lowest: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Target,
    Lowest,
    Drop,
    BackInStock,
}

/// Qual alerta (no máximo um) a rodada gera para o produto.
/// Prioridade: preço-alvo > menor já visto > queda > voltou a ter oferta. A primeira leitura nunca avisa.
pub fn evaluate(before: &Snapshot, after: Option<i64>, rules: &Rules) -> Option<Kind> {
    let after = after?;
    if !before.has_history {
        return None;
    }
    let hit_target = |was_above: bool| rules.target.is_some_and(|t| after <= t) && was_above;
    match before.best {
        None => Some(if hit_target(true) { Kind::Target } else { Kind::BackInStock }),
        Some(b) if after < b => {
            if hit_target(rules.target.is_some_and(|t| b > t)) {
                return Some(Kind::Target);
            }
            if rules.notify_lowest && before.lowest_ever.is_some_and(|l| after < l) {
                return Some(Kind::Lowest);
            }
            let drop = (b - after) as f64 * 100.0 / b as f64;
            (drop >= rules.min_drop_pct).then_some(Kind::Drop)
        }
        Some(_) => None,
    }
}

/// Queda de mais de 60% em relação à leitura anterior do mesmo link: pode ser erro de leitura.
pub fn suspicious(prev: Option<i64>, new: Option<i64>) -> bool {
    matches!((prev, new), (Some(p), Some(n)) if n * 100 < p * 40)
}

/// A segunda leitura confirma a suspeita se ficar a até 2% do preço suspeito.
pub fn confirms(pending: i64, new: Option<i64>) -> bool {
    new.is_some_and(|n| (n - pending).abs() * 100 <= pending * 2)
}

/// Promoção inflada: o preço "de" passa em mais de 5% o maior preço visto nos últimos 30 dias.
/// Só julga com pelo menos 7 dias de histórico.
pub fn inflated(list_price: Option<i64>, max_30d: Option<i64>, days_of_history: i64) -> bool {
    days_of_history >= 7 && matches!((list_price, max_30d), (Some(l), Some(m)) if l * 100 > m * 105)
}

/// Centavos → "R$ 1.234,56".
pub fn brl(cents: i64) -> String {
    let reais = (cents / 100).to_string();
    let mut grouped = String::new();
    for (i, c) in reais.chars().enumerate() {
        if i > 0 && (reais.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    format!("R$ {grouped},{:02}", cents % 100)
}

/// Texto da notificação.
pub fn message(kind: Kind, name: &str, before: Option<i64>, after: i64, rules: &Rules) -> String {
    match kind {
        Kind::Target => format!("{name} por {}: chegou ao seu preço-alvo ({})", brl(after), brl(rules.target.unwrap_or(after))),
        Kind::Lowest => format!("{name} por {}: o menor preço já visto", brl(after)),
        Kind::Drop => {
            let b = before.unwrap_or(after);
            let pct = ((b - after) as f64 * 100.0 / b as f64).round();
            format!("{name} caiu de {} para {} (-{pct}%)", brl(b), brl(after))
        }
        Kind::BackInStock => format!("{name} voltou a ter oferta: {}", brl(after)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: Rules = Rules { target: None, min_drop_pct: 5.0, notify_lowest: true };
    fn snap(best: Option<i64>, lowest: Option<i64>) -> Snapshot {
        Snapshot { best, lowest_ever: lowest, has_history: true }
    }

    #[test]
    fn primeira_leitura_e_subida_nao_avisam() {
        let first = Snapshot { best: None, lowest_ever: None, has_history: false };
        assert_eq!(evaluate(&first, Some(1000), &RULES), None);
        assert_eq!(evaluate(&snap(Some(1000), Some(900)), Some(1100), &RULES), None);
        assert_eq!(evaluate(&snap(Some(1000), Some(900)), Some(1000), &RULES), None);
        assert_eq!(evaluate(&snap(Some(1000), Some(900)), None, &RULES), None, "ficar sem oferta não avisa");
    }

    #[test]
    fn queda_respeita_o_minimo() {
        assert_eq!(evaluate(&snap(Some(10000), Some(8000)), Some(9600), &RULES), None, "4% é menos que 5%");
        assert_eq!(evaluate(&snap(Some(10000), Some(8000)), Some(9500), &RULES), Some(Kind::Drop));
    }

    #[test]
    fn prioridade_alvo_menor_queda() {
        let alvo = Rules { target: Some(9000), ..RULES };
        assert_eq!(evaluate(&snap(Some(10000), Some(9500)), Some(8900), &alvo), Some(Kind::Target));
        assert_eq!(evaluate(&snap(Some(8800), Some(8500)), Some(8400), &alvo), Some(Kind::Lowest), "já estava abaixo do alvo");
        assert_eq!(evaluate(&snap(Some(10000), Some(9500)), Some(9400), &RULES), Some(Kind::Lowest));
        let sem_menor = Rules { notify_lowest: false, ..RULES };
        assert_eq!(evaluate(&snap(Some(10000), Some(9500)), Some(9400), &sem_menor), Some(Kind::Drop));
    }

    #[test]
    fn voltou_a_ter_oferta() {
        assert_eq!(evaluate(&snap(None, Some(900)), Some(1000), &RULES), Some(Kind::BackInStock));
        let alvo = Rules { target: Some(1000), ..RULES };
        assert_eq!(evaluate(&snap(None, Some(900)), Some(1000), &alvo), Some(Kind::Target));
    }

    #[test]
    fn leitura_suspeita_e_confirmacao() {
        assert!(!suspicious(None, Some(100)));
        assert!(!suspicious(Some(10000), Some(4000)), "60% exatos ainda é normal");
        assert!(suspicious(Some(10000), Some(3999)));
        assert!(!suspicious(Some(10000), None));
        assert!(confirms(3999, Some(4050)));
        assert!(!confirms(3999, Some(4200)));
        assert!(!confirms(3999, None));
    }

    #[test]
    fn promocao_inflada() {
        assert!(inflated(Some(14900), Some(10000), 10));
        assert!(!inflated(Some(10400), Some(10000), 10), "até 5% acima é tolerado");
        assert!(!inflated(Some(14900), Some(10000), 6), "precisa de 7 dias de histórico");
        assert!(!inflated(None, Some(10000), 30));
    }

    #[test]
    fn textos_em_reais() {
        assert_eq!(brl(123456), "R$ 1.234,56");
        assert_eq!(brl(990), "R$ 9,90");
        assert_eq!(brl(100000000), "R$ 1.000.000,00");
        let alvo = Rules { target: Some(9000), ..RULES };
        assert_eq!(message(Kind::Target, "SSD", Some(10000), 8990, &alvo), "SSD por R$ 89,90: chegou ao seu preço-alvo (R$ 90,00)");
        assert_eq!(message(Kind::Lowest, "SSD", Some(10000), 8990, &RULES), "SSD por R$ 89,90: o menor preço já visto");
        assert_eq!(message(Kind::Drop, "SSD", Some(10000), 9000, &RULES), "SSD caiu de R$ 100,00 para R$ 90,00 (-10%)");
        assert_eq!(message(Kind::BackInStock, "SSD", None, 9000, &RULES), "SSD voltou a ter oferta: R$ 90,00");
    }
}
