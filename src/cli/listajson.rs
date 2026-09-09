//! O `list --json` — o CONTRATO que a janela do market consome.
//!
//! **O quê:** a mesma lista do `list` humano, em JSON de chaves estáveis: linguagens,
//! ferramentas de dev e os apps do ecossistema, com o que está instalado e por onde.
//!
//! **Onde:** [`crate::cli::lista::list_cmd`] quando `--json`, e a janela
//! (`schematize-market-gui`), que desenha a partir disto.
//!
//! ## Por que a janela NÃO pode ler a saída humana
//!
//! Já foi tentado, e quebrou: a janela do gestor casava rótulos **em português** do `status`,
//! então lia certo num idioma e devolvia tudo vazio nos outros dezenove — **sem erro nenhum**.
//! Com os campos vazios ela afirmava "app não instalado" a quem tinha o app.
//!
//! A tabela humana passa pelo catálogo i18n: `plataforma` em português, `platform` em inglês,
//! `プラットフォーム` em japonês. **As chaves daqui nunca são traduzidas** — é isso que as torna
//! contrato. O que é prosa (o rótulo de status, o `install_hint`) viaja como valor, para a
//! janela ter o que mostrar sem reimplementar o catálogo.
//!
//! **JSON escrito à mão, não `serde::Serialize`:** o shape é o contrato, e escrevê-lo
//! explicitamente faz uma mudança nele aparecer no diff. Com `Serialize`, renomear um campo
//! mudaria o JSON em silêncio — e quem quebraria seria a janela de quem já atualizou.

use market::appsdacasa::{descobrir_app, Estado, EXTERNOS};
use market::environments::{self, LangEnv};

/// Escapa o que vai dentro de aspas em JSON. Barra invertida ANTES da aspa, senão a barra que
/// escapa a aspa seria ela mesma escapada duas vezes.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// `Some(v)` vira `"v"` escapado; `None` vira `null` — nunca `""`. A janela precisa distinguir
/// "não sei" de "vazio": foi confundir os dois que fez a versão anterior dizer "não instalado".
fn opt(v: Option<String>) -> String {
    v.map(|v| format!("\"{}\"", esc(&v))).unwrap_or_else(|| "null".into())
}

/// **O quê:** uma entrada de linguagem/ferramenta em JSON. **Onde:** [`imprimir`].
fn linha_env(le: &LangEnv) -> String {
    let metodos: Vec<String> =
        le.methods_available.iter().map(|m| format!("\"{}\"", esc(m.slug()))).collect();
    format!(
        "    {{\"slug\": \"{}\", \"display\": \"{}\", \"category\": \"{}\", \
         \"methods\": [{}], \"installed_via\": {}, \"present\": {}, \
         \"hint\": \"{}\", \"status\": \"{}\"}}",
        esc(le.lang),
        esc(le.display),
        esc(le.category),
        metodos.join(", "),
        opt(le.installed.map(|m| m.slug().to_string())),
        le.runtime_present,
        esc(&le.install_hint),
        esc(&environments::status_text(le)),
    )
}

/// **O quê:** uma entrada de app do ecossistema em JSON. **Onde:** [`imprimir`].
///
/// `state` é um enum de três valores e não um booleano de propósito: um binário que está lá e
/// não responde é problema DIFERENTE de um que não existe, e achatar os dois foi o que fez a
/// lista dizer "não instalado" sobre um deployer quebrado.
fn linha_app(a: &market::appsdacasa::AppExterno) -> String {
    let (estado, versao) = match descobrir_app(a.bin) {
        Estado::Instalado { versao, .. } => ("installed", Some(versao)),
        Estado::Quebrado { .. } => ("broken", None),
        Estado::Ausente => ("absent", None),
    };
    format!(
        "    {{\"bin\": \"{}\", \"about\": \"{}\", \"state\": \"{}\", \"version\": {}}}",
        esc(a.bin),
        esc(a.sobre),
        estado,
        opt(versao),
    )
}

/// **O quê:** imprime a lista inteira em JSON. **Onde:** `schematize-market list --json`.
pub(crate) fn imprimir() {
    let envs = environments::status();
    let (langs, tools): (Vec<_>, Vec<_>) = envs.iter().partition(|le| le.category != "tool");

    println!("{{");
    println!("  \"market\": \"{}\",", env!("CARGO_PKG_VERSION"));
    println!("  \"languages\": [");
    println!("{}", langs.iter().map(|le| linha_env(le)).collect::<Vec<_>>().join(",\n"));
    println!("  ],");
    println!("  \"tools\": [");
    println!("{}", tools.iter().map(|le| linha_env(le)).collect::<Vec<_>>().join(",\n"));
    println!("  ],");
    println!("  \"apps\": [");
    println!("{}", EXTERNOS.iter().map(linha_app).collect::<Vec<_>>().join(",\n"));
    println!("  ]");
    println!("}}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Aspas e barras dentro de um valor não podem quebrar o JSON. Um `install_hint` ou um
    /// caminho de procedência com aspas viraria um documento inválido, e a janela mostraria
    /// tela vazia sem dizer por quê.
    #[test]
    fn escapa_aspas_e_barras() {
        assert_eq!(esc(r#"a"b"#), r#"a\"b"#);
        assert_eq!(esc(r"a\b"), r"a\\b");
        // A barra é escapada ANTES da aspa; na ordem inversa a barra da aspa seria
        // re-escapada e o resultado teria uma barra a mais.
        assert_eq!(esc(r#"\""#), r#"\\\""#);
    }

    /// `None` vira `null`, NUNCA `""`. A janela precisa separar "não sei" de "vazio" — foi
    /// confundir os dois que a fez afirmar "app não instalado" a quem tinha o app.
    #[test]
    fn ausente_e_null_e_nao_string_vazia() {
        assert_eq!(opt(None), "null");
        assert_eq!(opt(Some(String::new())), r#""""#);
        assert_ne!(opt(None), opt(Some(String::new())));
    }

    /// **O contrato.** Estas chaves são lidas pela janela; renomear qualquer uma quebra a
    /// janela de quem já atualizou, e o JSON à mão existe para que isso apareça no diff.
    #[test]
    fn as_chaves_do_contrato_estao_todas_la() {
        let le = &environments::status()[0];
        let j = linha_env(le);
        for k in
            ["slug", "display", "category", "methods", "installed_via", "present", "hint", "status"]
        {
            assert!(j.contains(&format!("\"{k}\"")), "faltou a chave `{k}` em: {j}");
        }
    }

    /// Estado de app é enum de TRÊS valores, não booleano: "está lá e não responde" é problema
    /// diferente de "não existe", e achatar os dois já fez a lista mentir sobre um deployer.
    #[test]
    fn estado_de_app_tem_tres_valores_possiveis() {
        let j = linha_app(&EXTERNOS[0]);
        assert!(
            j.contains("\"installed\"") || j.contains("\"broken\"") || j.contains("\"absent\""),
            "{j}"
        );
        for k in ["bin", "about", "state", "version"] {
            assert!(j.contains(&format!("\"{k}\"")), "faltou `{k}`: {j}");
        }
    }
}
