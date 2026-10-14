//! KIT COMPLETO — a lógica pura do `install --all`: o que cada item virou e o veredito final.
//!
//! **O quê:** [`Desfecho`] (o que aconteceu com um item), [`AcaoMarket`]/[`decidir_market`] (o que
//! fazer com o próprio market e a janela dele), [`resumo`] (a tabela final) e [`veredito`] (o
//! `Result` que vira o exit code).
//!
//! **Onde:** `cli/instalar.rs` (`install --all`). Tudo aqui é PURO — sem rede nem disco —, porque
//! o defeito que isto conserta (apps sem janela, falha de um item escondida no meio de muita
//! saída) só é pego por teste se a decisão for separável do I/O (§37.48).

use crate::nucleo::i18n::tf;
use crate::nucleo::util;

/// **O quê:** o que aconteceu com um item do `install --all`. **Onde:** [`resumo`] e [`veredito`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desfecho {
    /// Não existia e agora existe.
    Instalado,
    /// Já existia e foi trocado por uma versão nova (ou completado com a janela que faltava).
    Atualizado,
    /// Já estava na última versão, com a janela: nada foi refeito.
    EmDia,
    /// Não deu para confirmar contra o release; nada foi alterado (não é falha do item).
    NaoConfirmado,
    /// `--dry-run` ou a pessoa não confirmou: nada foi tocado.
    NadaFeito,
    /// Falhou. A causa vem junto (e já foi impressa quando aconteceu).
    Falhou(String),
}

impl Desfecho {
    /// **O quê:** a etiqueta curta do item no [`resumo`]. **Onde:** [`resumo`].
    fn rotulo(&self) -> &'static str {
        match self {
            Desfecho::Instalado => "instalado",
            Desfecho::Atualizado => "atualizado",
            Desfecho::EmDia => "já em dia",
            Desfecho::NaoConfirmado => "não confirmado (nada alterado)",
            Desfecho::NadaFeito => "nada feito",
            Desfecho::Falhou(_) => "FALHOU",
        }
    }
}

/// **O quê:** a tabela final do `install --all`, uma linha por item, mais a contagem. PURA.
/// **Onde:** `cli/instalar.rs`, depois do último item.
pub fn resumo(itens: &[(String, Desfecho)]) -> String {
    let largura = itens.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    let mut s = String::from("\nResumo:\n");
    for (nome, d) in itens {
        s.push_str(&format!("  {nome:<largura$}  {}\n", d.rotulo()));
        if let Desfecho::Falhou(causa) = d {
            let primeira = causa.lines().next().unwrap_or("").trim();
            s.push_str(&format!("  {:<largura$}    causa: {primeira}\n", ""));
        }
    }
    let conta = |f: fn(&Desfecho) -> bool| itens.iter().filter(|(_, d)| f(d)).count();
    s.push_str(&format!(
        "  {} instalado(s), {} atualizado(s), {} já em dia, {} falha(s).\n",
        conta(|d| *d == Desfecho::Instalado),
        conta(|d| *d == Desfecho::Atualizado),
        conta(|d| *d == Desfecho::EmDia),
        conta(|d| matches!(d, Desfecho::Falhou(_))),
    ));
    s
}

/// **O quê:** `Ok` se nenhum item falhou; senão `Err` acionável (os nomes e como repetir) — o
/// exit code ≠ 0. A falha de um item NÃO abortou os outros: este é só o veredito do conjunto.
/// **Onde:** o fim de `install --all`.
pub fn veredito(itens: &[(String, Desfecho)]) -> Result<(), String> {
    let falhas: Vec<&str> = itens
        .iter()
        .filter(|(_, d)| matches!(d, Desfecho::Falhou(_)))
        .map(|(n, _)| n.as_str())
        .collect();
    if falhas.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{}.\n  Os outros itens foram tratados normalmente. As causas estão \
         acima e no Resumo.\n  Corrija o que elas dizem (rede, espaço em disco, permissão) e rode \
         de novo:\n    schematize-market install --all -y\n  O que já está em dia não é refeito; \
         só o que falhou é tentado outra vez.",
        tf("install.all_failed", &[("apps", &falhas.join(", "))])
    ))
}

/// **O quê:** o que fazer com o próprio market e a janela dele no `install --all`.
/// **Onde:** [`decidir_market`] (produz) e `cli/instalar.rs` (executa).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcaoMarket {
    /// O market está mais velho que o release: self-update para esta versão (traz a janela).
    AtualizarCli(String),
    /// O CLI está em dia mas falta a janela: baixa só ela, desta versão do release.
    SoJanela(String),
    /// CLI e janela em dia.
    EmDia,
    /// Não dá para decidir (sem versão publicada). Texto acionável.
    Impossivel(String),
}

/// **O quê:** decide a [`AcaoMarket`] a partir da versão em execução, da última publicada
/// (`None` = não consegui saber, com a causa) e de a janela faltar. PURA.
/// **Onde:** `cli/instalar.rs`.
pub fn decidir_market(
    atual: &str,
    publicada: Result<&str, &str>,
    janela_ausente: bool,
) -> AcaoMarket {
    match publicada {
        Err(causa) => AcaoMarket::Impossivel(causa.to_string()),
        Ok(v) if util::semver_lt(atual, v) => AcaoMarket::AtualizarCli(v.to_string()),
        Ok(v) if janela_ausente => AcaoMarket::SoJanela(v.to_string()),
        Ok(_) => AcaoMarket::EmDia,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(n: &str, d: Desfecho) -> (String, Desfecho) {
        (n.to_string(), d)
    }

    /// O resumo diz, por item, instalado/atualizado/em dia/falhou — e a causa da falha.
    #[test]
    fn resumo_lista_cada_item_e_a_causa_da_falha() {
        let r = resumo(&[
            item("schematize-git", Desfecho::Instalado),
            item("schematize-deployer", Desfecho::Atualizado),
            item("schematize-skills", Desfecho::EmDia),
            item("schematize-optimizer", Desfecho::Falhou("sem rede\nlinha 2".into())),
        ]);
        for esperado in [
            "schematize-git",
            "instalado",
            "atualizado",
            "já em dia",
            "FALHOU",
            "causa: sem rede",
            "1 instalado(s), 1 atualizado(s), 1 já em dia, 1 falha(s)",
        ] {
            assert!(r.contains(esperado), "falta `{esperado}` em:\n{r}");
        }
        assert!(!r.contains("linha 2"), "só a 1ª linha da causa entra no resumo");
    }

    /// Sem falha: `Ok`. Com falha: `Err` que nomeia SÓ os que falharam e diz como repetir.
    #[test]
    fn veredito_so_falha_quando_algum_item_falhou() {
        let bom = [item("a", Desfecho::EmDia), item("b", Desfecho::NaoConfirmado)];
        assert!(veredito(&bom).is_ok(), "não confirmado não é falha");
        let ruim = [
            item("a", Desfecho::Instalado),
            item("b", Desfecho::Falhou("x".into())),
            item("c", Desfecho::Falhou("y".into())),
        ];
        let e = veredito(&ruim).unwrap_err();
        assert!(e.contains("b, c") && !e.contains("a,"), "{e}");
        assert!(e.contains("schematize-market install --all -y"), "diz como repetir: {e}");
    }

    /// As quatro decisões do market: velho -> self-update; em dia sem janela -> só a janela;
    /// em dia completo -> nada; à frente do release (dev build) sem janela -> a janela do
    /// release; sem versão publicada -> impossível, dito.
    #[test]
    fn decide_market_cli_janela_e_indisponivel() {
        use AcaoMarket::*;
        assert_eq!(decidir_market("0.4.5", Ok("0.4.6"), false), AtualizarCli("0.4.6".into()));
        assert_eq!(decidir_market("0.4.5", Ok("0.4.6"), true), AtualizarCli("0.4.6".into()));
        assert_eq!(decidir_market("0.4.6", Ok("0.4.6"), true), SoJanela("0.4.6".into()));
        assert_eq!(decidir_market("0.4.6", Ok("0.4.6"), false), EmDia);
        assert_eq!(decidir_market("0.4.7", Ok("0.4.6"), true), SoJanela("0.4.6".into()));
        assert_eq!(decidir_market("0.4.6", Err("sem rede"), true), Impossivel("sem rede".into()));
    }
}
