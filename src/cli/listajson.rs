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
//! contrato.
//!
//! ## A regra do sufixo `_text`, e por que ela precisou ser escrita
//!
//! Este arquivo é o molde que o optimizer e o deployer seguiram. Ao escrever o `--json`
//! daqueles dois, a regra que saiu foi mais dura: **documento byte a byte idêntico em qualquer
//! idioma**. Aplicada a este arquivo, ela reprovava — o campo `status` carregava prosa, parte
//! traduzida pelo catálogo (`env.installed_via`) e parte codificada em português direto no
//! `Procedencia::rotulo` ("via distro (nodejs22)").
//!
//! A saída não foi tirar a prosa: a janela precisa de algo para MOSTRAR, e reimplementar o
//! catálogo do market dentro dela seria pior. Foi separar as duas naturezas por NOME:
//!
//! - **Campo de DECISÃO** — `slug`, `category`, `methods`, `installed_via`, `present`,
//!   `provenance`, `state`, `version`. Estável, nunca traduzido, é sobre isto que a janela
//!   ramifica. É o `provenance` que foi acrescentado agora: a via existia só como prosa.
//! - **Campo `*_text`** — `status_text`, e só ele. É prosa para exibir, muda com o idioma e
//!   com revisão de texto, e **nenhuma decisão pode depender dele**.
//!
//! Assim o teste de idioma continua existindo e continua duro: mascarados os `*_text`, o
//! documento é byte a byte idêntico. O que varia está declarado no próprio nome do campo, em
//! vez de ficar implícito no bom senso de quem for mexer daqui a um ano.
//!
//! **O `hint` não é prosa** apesar do nome: é a lista de slugs de método separada por vírgula,
//! ou o `source_hint` estático da ferramenta. Fica sem sufixo por isso.
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
         \"provenance\": \"{}\", \"hint\": \"{}\", \"status_text\": \"{}\"}}",
        esc(le.lang),
        esc(le.display),
        esc(le.category),
        metodos.join(", "),
        opt(le.installed.map(|m| m.slug().to_string())),
        le.runtime_present,
        // A VIA, como slug estável. Antes ela só existia dentro do `status_text`, em prosa —
        // e uma janela que precisasse dela teria de casar string humana.
        le.procedencia.as_ref().map(|p| p.slug()).unwrap_or("absent"),
        esc(&le.install_hint),
        // O ÚNICO campo de prosa, e o nome diz. Muda com o idioma; nenhuma decisão da janela
        // pode depender dele.
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
        for k in [
            "slug",
            "display",
            "category",
            "methods",
            "installed_via",
            "present",
            "provenance",
            "hint",
            "status_text",
        ] {
            assert!(j.contains(&format!("\"{k}\"")), "faltou a chave `{k}` em: {j}");
        }
        // O campo de prosa se declara no NOME. Um `"status"` sem sufixo é a versão antiga,
        // em que a janela não tinha como saber que aquilo mudava de idioma.
        assert!(!j.contains("\"status\":"), "prosa sem o sufixo `_text`: {j}");
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
