//! O `list --json` é CONTRATO — e o que nele muda de idioma está DECLARADO no nome do campo.
//!
//! # Por que este arquivo existe
//!
//! A janela deste app já leu a saída HUMANA, e quebrou por isso: ela casava rótulos **em
//! português**, lia certo num idioma e devolvia tudo vazio nos outros dezenove, **sem erro
//! nenhum**. Com os campos vazios ela afirmava "app não instalado" a quem tinha o app, e a
//! pessoa clicava em "Instalar" sem que nada acontecesse.
//!
//! O `--json` nasceu dessa lição. Mas ele nasceu com meia lição aprendida: as CHAVES ficaram
//! estáveis, e um dos VALORES continuou sendo prosa traduzida. Escrever o `--json` do optimizer
//! e do deployer depois produziu a regra dura — documento byte a byte idêntico em qualquer
//! idioma — e ela reprovava este arquivo, que era o molde.
//!
//! # A regra, como ficou
//!
//! A prosa não some: a janela precisa de algo para mostrar, e reimplementar o catálogo de vinte
//! idiomas dentro dela seria pior que o problema. O que muda é que a natureza do campo passa a
//! estar no NOME:
//!
//! - **Campo de decisão** — sem sufixo. Estável em qualquer idioma. É sobre ele que a janela
//!   ramifica.
//! - **Campo `*_text`** — prosa para exibir. Muda com o idioma e com revisão de texto, e
//!   nenhuma decisão pode depender dele.
//!
//! Este teste é o que dá dentes à regra: mascarados os `*_text`, o documento tem de ser byte a
//! byte idêntico nos **vinte** idiomas do catálogo. Sem ele a regra seria um comentário.

use std::process::Command;

/// **O quê:** o caminho do binário compilado, ao lado do executável de teste.
fn bin() -> std::path::PathBuf {
    let mut p = std::env::current_exe().expect("o teste tem caminho");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("schematize-market")
}

/// **O quê:** roda `list --json` num idioma, num `HOME` isolado, e devolve o stdout.
///
/// **Onde:** os testes daqui.
///
/// **O `HOME` é temporário porque o idioma ativo NÃO vem só do ambiente:** este app lê primeiro
/// o `config.json` do usuário. Rodar sobre o `HOME` de quem executa a suíte faria a escolha de
/// idioma dele vencer a do teste — e o teste passaria comparando o mesmo idioma com ele mesmo,
/// vinte vezes.
fn rodar(home: &std::path::Path, lang: &str) -> String {
    let mut c = Command::new(bin());
    for v in ["SCHEMATIZE_LANG", "LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        c.env_remove(v);
    }
    let out = c
        .args(["list", "--json"])
        .env("HOME", home)
        .env("SCHEMATIZE_LANG", lang)
        .output()
        .unwrap_or_else(|e| panic!("não consegui executar {}: {e}", bin().display()));
    assert!(
        out.status.success(),
        "`list --json` saiu com {} — stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("o contrato é UTF-8")
}

/// **O quê:** um `HOME` temporário próprio deste teste.
fn casa(nome: &str) -> std::path::PathBuf {
    let h = std::env::temp_dir().join(format!("mkt-json-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&h);
    std::fs::create_dir_all(&h).expect("criar o HOME temporário");
    h
}

/// Os vinte idiomas do catálogo. A lista está aqui à mão de propósito: se alguém acrescentar
/// um idioma e não o puser aqui, o novo idioma fica sem esta prova — e o teste que garante que
/// a lista está completa é o [`todos_os_idiomas_do_catalogo_estao_cobertos`] abaixo.
const IDIOMAS: &[&str] = &[
    "en", "es", "it", "fr", "de", "pt", "ja", "zh", "ru", "ar", "hi", "ko", "tr", "vi", "id", "pl",
    "nl", "th", "fa", "bn",
];

/// **O quê:** substitui o valor de todo campo `"algo_text": "…"` por `"algo_text": "<MASCARADO>"`.
///
/// **Onde:** [`fora_dos_campos_text_o_documento_nao_muda_de_idioma`].
///
/// **Por que mascarar em vez de remover:** remover mudaria o tamanho e a pontuação da linha, e
/// um campo que sumisse de um idioma e não do outro passaria despercebido. Mascarado, a
/// ESTRUTURA continua sendo comparada — só o conteúdo da prosa sai da conta.
fn mascarar_text(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let mut resto = json;
    while let Some(i) = resto.find("_text\": \"") {
        let ini = i + "_text\": \"".len();
        out.push_str(&resto[..ini]);
        // Anda até a aspa de fechamento, respeitando escape.
        let bytes = resto.as_bytes();
        let mut j = ini;
        while j < bytes.len() {
            match bytes[j] {
                b'\\' => j += 2,
                b'"' => break,
                _ => j += 1,
            }
        }
        out.push_str("<MASCARADO>");
        resto = &resto[j.min(resto.len())..];
    }
    out.push_str(resto);
    out
}

/// **O TESTE QUE DÁ DENTES À REGRA.** Fora dos `*_text`, o documento é byte a byte idêntico
/// nos vinte idiomas.
///
/// Se alguém puser prosa traduzida num campo sem o sufixo — ou renomear um `*_text` para tirar
/// o sufixo —, isto falha na hora, e não seis meses depois na máquina de um usuário japonês.
#[test]
fn fora_dos_campos_text_o_documento_nao_muda_de_idioma() {
    let h = casa("idiomas");
    let referencia = mascarar_text(&rodar(&h, IDIOMAS[0]));
    for lang in &IDIOMAS[1..] {
        let outro = mascarar_text(&rodar(&h, lang));
        assert_eq!(
            referencia, outro,
            "o documento mudou entre `{}` e `{lang}` FORA dos campos `_text` — \
             prosa traduzida vazou para um campo de decisão",
            IDIOMAS[0]
        );
    }
    let _ = std::fs::remove_dir_all(&h);
}

/// **E a prosa REALMENTE muda** — senão o teste acima passaria por não haver nada traduzido, e
/// não por a separação estar certa. Um teste que passaria com o app quebrado não prova nada.
#[test]
fn o_campo_de_prosa_muda_mesmo_de_idioma() {
    let h = casa("prosa");
    let en = rodar(&h, "en");
    let ja = rodar(&h, "ja");
    assert_ne!(en, ja, "nada mudou entre inglês e japonês — o catálogo não está sendo aplicado");
    // E o que mudou está DENTRO dos `_text`: mascarados, os dois voltam a ser iguais.
    assert_eq!(mascarar_text(&en), mascarar_text(&ja));
    let _ = std::fs::remove_dir_all(&h);
}

/// A lista de idiomas deste arquivo cobre o catálogo inteiro. Acrescentar um idioma sem o pôr
/// aqui deixaria o novo sem a prova, em silêncio.
#[test]
fn todos_os_idiomas_do_catalogo_estao_cobertos() {
    let do_catalogo: Vec<&str> = market::nucleo::i18n::LANGS.iter().map(|(c, _, _)| *c).collect();
    for c in &do_catalogo {
        assert!(IDIOMAS.contains(c), "idioma `{c}` do catálogo não está coberto por este teste");
    }
    assert_eq!(do_catalogo.len(), IDIOMAS.len(), "a lista deste teste tem idioma que não existe");
}

/// O mascarador só toca no valor dos `*_text`, e nada mais. Um mascarador que comesse o
/// documento inteiro faria o teste de idioma passar sempre.
#[test]
fn o_mascarador_so_toca_no_valor_dos_campos_text() {
    let j = r#"{"slug": "rust", "status_text": "instalado via mise", "present": true}"#;
    let m = mascarar_text(j);
    assert!(m.contains(r#""slug": "rust""#), "comeu um campo de decisão: {m}");
    assert!(m.contains(r#""present": true"#), "comeu um campo de decisão: {m}");
    assert!(m.contains(r#""status_text": "<MASCARADO>""#), "não mascarou a prosa: {m}");
    assert!(!m.contains("mise"), "a prosa sobreviveu: {m}");
    // Aspas escapadas dentro da prosa não terminam o valor cedo demais.
    let j = r#"{"a_text": "diz \"oi\" assim", "b": 1}"#;
    let m = mascarar_text(j);
    assert!(m.contains(r#""b": 1"#), "a aspa escapada cortou o documento: {m}");
    assert!(!m.contains("oi"), "{m}");
}

// ---------------------------------------------------------------------------
// A brecha que o teste de idioma NÃO pega, e por que ela precisa de teste próprio.
// ---------------------------------------------------------------------------

/// **O quê:** todos os valores de uma chave de string, na ordem em que aparecem.
///
/// **Onde:** [`os_campos_de_decisao_so_tem_slug_de_conjunto_fechado`].
fn valores_de(json: &str, chave: &str) -> Vec<String> {
    let marca = format!("\"{chave}\": \"");
    let mut out = Vec::new();
    let mut resto = json;
    while let Some(i) = resto.find(&marca) {
        let ini = i + marca.len();
        let bytes = resto.as_bytes();
        let mut j = ini;
        while j < bytes.len() {
            match bytes[j] {
                b'\\' => j += 2,
                b'"' => break,
                _ => j += 1,
            }
        }
        out.push(resto[ini..j.min(resto.len())].to_string());
        resto = &resto[j.min(resto.len())..];
    }
    out
}

/// **A BRECHA, e este é o teste que a fecha.**
///
/// O teste de idioma compara o mesmo documento em vinte idiomas. Ele pega prosa que passa pelo
/// CATÁLOGO — e só essa. Prosa **codificada direto em português** num campo de decisão é
/// idêntica nos vinte idiomas, então atravessa o teste de idioma sem um arranhão.
///
/// Isso não é hipótese: ao montar este arquivo, trocar o `provenance` de `slug()` para
/// `rotulo()` (que devolve `"via distro (nodejs22)"`, em português cru) **passou** no teste de
/// idioma. Um campo de decisão carregando prosa, e a rede toda verde.
///
/// A prova que falta, então, não é "não muda de idioma": é "**é slug de um conjunto fechado**".
/// Um valor fora do conjunto é a janela recebendo algo que ela não sabe desenhar — e o silêncio
/// nesse caso é como uma tela passa a mentir.
#[test]
fn os_campos_de_decisao_so_tem_slug_de_conjunto_fechado() {
    let h = casa("slugs");
    let json = rodar(&h, "en");

    let provenances = valores_de(&json, "provenance");
    assert!(!provenances.is_empty(), "sem entradas o teste é vazio");
    for v in &provenances {
        assert!(
            ["mise", "docker", "distro", "official", "unknown", "absent"].contains(&v.as_str()),
            "`provenance` fora do conjunto fechado: {v:?} — prosa num campo de decisão"
        );
    }

    for v in valores_de(&json, "category") {
        assert!(
            ["language", "tool"].contains(&v.as_str()),
            "`category` fora do conjunto fechado: {v:?}"
        );
    }

    for v in valores_de(&json, "state") {
        assert!(
            ["installed", "broken", "absent"].contains(&v.as_str()),
            "`state` fora do conjunto fechado: {v:?}"
        );
    }

    let _ = std::fs::remove_dir_all(&h);
}

/// O extrator deste arquivo pega o valor certo, e não para na primeira aspa escapada.
#[test]
fn o_extrator_de_valores_le_o_que_deve() {
    let j = r#"{"a": "um", "b": 1, "a": "do\"is"}"#;
    assert_eq!(valores_de(j, "a"), vec!["um".to_string(), r#"do\"is"#.to_string()]);
    assert!(valores_de(j, "inexistente").is_empty());
    // `b` é número, não string: o extrator de string não o vê, e é isso que se quer.
    assert!(valores_de(j, "b").is_empty());
}
