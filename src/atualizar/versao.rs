//! VERSÃO — o que está instalado, o que foi publicado, e o que o usuário fixou.
//!
//! **O quê:** resolve a última versão publicada de qualquer repo da casa (lendo o `version` do
//! `Cargo.toml` no `main`), lê a versão de um binário instalado, e gerencia o PIN — a versão
//! que a pessoa fixou de propósito.
//!
//! **Onde:** [`super::planejar`] (decidir se atualiza) e `schematize-market status`.
//!
//! ## Procedência
//!
//! Porte do `version.rs` do `schematize_updater_rs` (ADR-0013), sob o D4 — comportamento
//! idêntico, incluindo o formato e o CAMINHO do arquivo de pin. O acréscimo é o pin por
//! COMPONENTE: o updater tinha um pin só, do app, e o market gere seis binários.

use crate::nucleo::{plataforma, rede, util};
use std::path::{Path, PathBuf};

/// **O quê:** extrai o 1º `version = "x.y.z"` de um `Cargo.toml` — o do `[package]`, que vem
/// no topo. `None` se não houver.
///
/// **Onde:** [`latest_version_of`], sobre o texto que veio da rede.
///
/// **Por que ler o `Cargo.toml` e não a tag:** a tag é humana e pode atrasar (é literalmente
/// um dos itens `- [H ]` deste projeto). O `Cargo.toml` do `main` é o que o build produz, e é
/// a versão que o binário vai reportar em `--version` — comparar com ela é comparar com o
/// que de fato existe.
pub fn parse_cargo_version(toml: &str) -> Option<String> {
    for line in toml.lines() {
        let l = line.trim();
        let Some(rest) = l.strip_prefix("version") else { continue };
        let Some(rest) = rest.trim_start().strip_prefix('=') else { continue };
        let v = rest.trim().trim_matches('"').trim();
        if !v.is_empty() && v.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            return Some(v.to_string());
        }
    }
    None
}

/// **O quê:** última versão publicada de QUALQUER repo da casa. `None` se a rede falhar.
///
/// **Onde:** [`super::planejar`], uma vez por app gerido.
///
/// **Por que parametrizado pelo repo:** cada app tem **versão própria** e ciclo próprio.
/// Reusar a versão do schematize para decidir sobre o Deployer seria comparar duas coisas
/// diferentes — e é exatamente o defeito que esta função existe para não deixar acontecer de
/// novo.
pub fn latest_version_of(repo: &str) -> Option<String> {
    parse_cargo_version(&rede::get_text(&rede::url_do_cargo_toml(repo))?)
}

/// **O quê:** versão de um binário instalado (`<bin> --version` → último token que começa com
/// dígito). `None` se não está instalado ou se não responde.
///
/// **Onde:** [`super::estado_dos_apps`] e o `status`.
///
/// **Por que o ÚLTIMO token e não o segundo:** o app já trocou de nome uma vez (Overflow →
/// schematize). Ancorar na posição do nome quebraria de novo na próxima; ancorar no formato
/// (o número é o que termina a linha) sobreviveu à troca e sobrevive à seguinte.
pub fn installed_version_of(bin: &Path) -> Option<String> {
    let out = util::capture(bin.to_str()?, &["--version"])?;
    out.split_whitespace()
        .last()
        .map(|s| s.to_string())
        .filter(|s| s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
}

/// **O quê:** a última versão publicada do app (CLI). **Onde:** o `status`.
pub fn latest_app_version() -> Option<String> {
    latest_version_of(plataforma::APP_REPO)
}

/// **O quê:** a versão do app instalado nesta máquina. **Onde:** o `status` e o plano.
pub fn installed_app_version() -> Option<String> {
    installed_version_of(&plataforma::app_bin(false))
}

/// **O quê:** o arquivo de PIN do app. Vazio ou ausente = seguir a última publicada.
///
/// **Onde:** [`read_pin`] e [`write_pin`].
///
/// **É o MESMO caminho que o updater usava (`~/.schematize/updater/pin`), e isso é o item.**
/// Quem fixou uma versão fixou por um motivo; se o market lesse outro lugar, encontraria vazio
/// e voltaria a seguir `latest` — atualizando em silêncio a máquina de quem pediu
/// explicitamente para não atualizar. Ver o doc de [`crate::nucleo::plataforma::state_dir`].
fn pin_file() -> PathBuf {
    plataforma::state_dir().join("pin")
}

/// **O quê:** lê a versão fixada, se houver. **Onde:** [`target_version`] e o `status`.
pub fn read_pin() -> Option<String> {
    ler_pin_de(&pin_file())
}

/// **O quê:** a leitura do pin a partir de um CAMINHO. **Onde:** [`read_pin`] e o teste que
/// prova a compatibilidade com o arquivo gravado pelo updater antigo.
///
/// **Por que separada:** o `read_pin` do updater lia o caminho fixo, e por isso nenhum teste
/// conseguia provar que ele lê o que o antecessor gravou sem mexer no `~` de quem roda a
/// suíte. Com o caminho por parâmetro, o teste usa um sandbox — e a regra fica coberta.
pub fn ler_pin_de(p: &Path) -> Option<String> {
    let s = std::fs::read_to_string(p).ok()?;
    let s = s.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// **O quê:** fixa uma versão. `None` desafixa (volta a seguir a última publicada).
/// **Onde:** os comandos `pin` e `unpin`.
pub fn write_pin(v: Option<&str>) -> Result<(), String> {
    let _ = std::fs::create_dir_all(plataforma::state_dir());
    match v {
        Some(v) => std::fs::write(pin_file(), v)
            .map_err(|e| format!("não consegui gravar {}: {e}", pin_file().display())),
        None => {
            // Não existir já é o estado desejado — `remove_file` num arquivo ausente devolve
            // `NotFound`, e tratar isso como erro faria `unpin` falhar em quem nunca fixou.
            match std::fs::remove_file(pin_file()) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("não consegui remover {}: {e}", pin_file().display())),
            }
        }
    }
}

/// **O quê:** a versão-ALVO do app: o pin se houver, senão a última publicada.
/// **Onde:** [`super::planejar`].
pub fn target_version() -> Option<String> {
    read_pin().or_else(latest_app_version)
}

/// **O quê:** interpreta o que a pessoa digitou em `pin <x>`. Função PURA — decide, não grava.
///
/// **Onde:** o comando `pin`.
///
/// **Por que existe:** `pin latest` gravava a string `"latest"` no arquivo. O
/// [`target_version`] é `read_pin().or_else(latest_app_version)`, então o alvo passava a ser
/// literalmente `"latest"` — e o updater tentava baixar `releases/download/vlatest`, que não
/// existe. Silenciosamente, e só na próxima atualização.
///
/// E `latest` é **a palavra que a pessoa naturalmente digita** para desafixar: o `status`
/// mostra "seguindo latest", então `pin latest` parece a forma de voltar a isso. Havia um
/// `unpin`, mas ninguém adivinha um comando que não errou. §37.48: edge case que um leigo
/// atinge é bug do software, não erro de quem digitou.
pub fn interpretar_pin(entrada: &str) -> Result<Option<String>, String> {
    let v = entrada.trim();
    // As palavras que significam "volte a seguir a última" — todas desafixam.
    if v.is_empty() || matches!(v.to_lowercase().as_str(), "latest" | "none" | "nenhum" | "-") {
        return Ok(None);
    }
    // Tolera o `v` da tag: quem copia de um release cola `v0.57.0`.
    let limpo = v.strip_prefix('v').unwrap_or(v);
    if !limpo.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        return Err(format!(
            "`{v}` não parece uma versão. Use algo como `0.57.0`, \
             ou `pin latest` para voltar a seguir a última publicada"
        ));
    }
    Ok(Some(limpo.to_string()))
}

/// **O quê:** este componente precisa de update? Compara a versão DELE com a do repo DELE.
///
/// **Onde:** [`super::planejar`], uma vez por app gerido.
///
/// **A regra, e os três casos que não são óbvios:**
/// - **Sem saber a última** (rede fora) → **não** recompila. O que está instalado funciona, e
///   queimar minutos de CPU por causa de um GitHub indisponível é o oposto de útil.
/// - **Instalado mas sem responder `--version`** → reconstrói. É instalação quebrada, e
///   reconstruir é o conserto provável; não fazer nada deixaria a pessoa presa.
/// - **Diferentes** → atualiza, em qualquer direção. Não é `<`: quem tem uma versão à frente
///   da publicada (um build local) e pede `update` está pedindo para voltar ao publicado.
pub fn desatualizado(instalada: Option<&str>, ultima: Option<&str>) -> bool {
    match (instalada, ultima) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(a), Some(b)) => a != b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_do_cargo() {
        let toml = "[package]\nname = \"schematize\"\nversion = \"0.34.1\"\nedition = \"2021\"\n";
        assert_eq!(parse_cargo_version(toml).as_deref(), Some("0.34.1"));
    }

    /// O `version` do `[package]` vem primeiro; um `version` de dependência mais abaixo NÃO
    /// pode ganhar. Sem esta asserção, o alvo viraria a versão de uma dep qualquer.
    #[test]
    fn pega_a_versao_do_package_e_nao_a_de_uma_dependencia() {
        let toml = "[package]\nname = \"x\"\nversion = \"1.2.3\"\n\n\
                    [dependencies]\nclap = { version = \"4\" }\n";
        assert_eq!(parse_cargo_version(toml).as_deref(), Some("1.2.3"));
        // E um Cargo.toml sem `version` não inventa um.
        assert_eq!(parse_cargo_version("[package]\nname = \"x\"\n"), None);
        assert_eq!(parse_cargo_version(""), None);
    }

    /// **O bug que esta função existe para não repetir:** `pin latest` gravava a string
    /// "latest" como se fosse número de versão, e o alvo virava `vlatest`.
    #[test]
    fn latest_desafixa_em_vez_de_virar_versao() {
        assert_eq!(interpretar_pin("latest").unwrap(), None);
        assert_eq!(interpretar_pin("LATEST").unwrap(), None);
        assert_eq!(interpretar_pin("  latest  ").unwrap(), None);
        // Os outros jeitos de dizer a mesma coisa.
        for x in ["", "none", "nenhum", "-"] {
            assert_eq!(interpretar_pin(x).unwrap(), None, "{x:?} devia desafixar");
        }
    }

    /// Versão de verdade passa — com ou sem o `v` que se copia de uma tag.
    #[test]
    fn versao_passa_com_ou_sem_o_v_da_tag() {
        assert_eq!(interpretar_pin("0.57.0").unwrap(), Some("0.57.0".into()));
        assert_eq!(interpretar_pin("v0.57.0").unwrap(), Some("0.57.0".into()), "o `v` da tag");
    }

    /// Qualquer outra coisa é RECUSADA com uma mensagem que ensina o formato — nunca gravada
    /// para explodir depois, na próxima atualização.
    #[test]
    fn lixo_e_recusado_com_mensagem_acionavel() {
        for x in ["abacaxi", "main", "HEAD", "--force"] {
            let e = interpretar_pin(x).unwrap_err();
            assert!(e.contains("0.57.0"), "a mensagem tem de dar o formato: {e}");
            assert!(e.contains("pin latest"), "e a saída: {e}");
        }
    }

    /// **R2, o risco que este teste cobre:** o market LÊ o arquivo de pin GRAVADO pelo updater
    /// antigo. Mesmo caminho (`~/.schematize/updater/pin`, ver `plataforma::state_dir`) e
    /// mesmo formato (a versão nua, sem `v`, sem JSON, sem cabeçalho).
    ///
    /// A fixture é escrita EXATAMENTE como o `version::write_pin` do updater escrevia:
    /// `fs::write(pin_file(), v)` — sem newline no fim. O teste também cobre a variante com
    /// newline, porque um editor de texto acrescenta uma e o arquivo continua sendo o mesmo
    /// pin para quem o abriu à mão.
    #[test]
    fn le_o_pin_gravado_pelo_updater_antigo() {
        let d = std::env::temp_dir().join(format!("market-pin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();

        let p = d.join("pin");
        // Byte a byte o que o updater gravava.
        std::fs::write(&p, "0.55.7").unwrap();
        assert_eq!(ler_pin_de(&p).as_deref(), Some("0.55.7"), "o pin do updater tem de ser lido");

        // Com newline (alguém abriu no editor) continua sendo o mesmo pin.
        std::fs::write(&p, "0.55.7\n").unwrap();
        assert_eq!(ler_pin_de(&p).as_deref(), Some("0.55.7"));

        // Arquivo vazio e só-espaço significam "não há pin" — nunca uma versão vazia, que
        // viraria o alvo `v` e um 404 silencioso.
        std::fs::write(&p, "").unwrap();
        assert_eq!(ler_pin_de(&p), None);
        std::fs::write(&p, "  \n ").unwrap();
        assert_eq!(ler_pin_de(&p), None);

        // Arquivo ausente é o caso comum: quem nunca fixou.
        std::fs::remove_file(&p).unwrap();
        assert_eq!(ler_pin_de(&p), None);

        let _ = std::fs::remove_dir_all(&d);
    }

    /// O caminho do pin é o do updater — a asserção que trava a compatibilidade de R2 no
    /// lugar onde ela pode ser quebrada por descuido (mudar `state_dir`).
    #[test]
    fn o_pin_mora_onde_o_updater_o_deixou() {
        assert!(
            pin_file().ends_with(".schematize/updater/pin"),
            "o pin mudou de lugar — quem fixou versão perderia o pin: {}",
            pin_file().display()
        );
    }

    /// **O defeito que esta função existe para não repetir:** decidir sobre um app pela versão
    /// de OUTRO. Aqui a comparação é entre as versões do mesmo componente.
    #[test]
    fn compara_a_versao_do_proprio_componente() {
        assert!(desatualizado(Some("0.2.1"), Some("0.3.0")), "mais novo lá → atualiza");
        assert!(!desatualizado(Some("0.3.0"), Some("0.3.0")), "igual → não mexe");
    }

    /// Rede fora não pode virar recompilação.
    #[test]
    fn sem_saber_a_ultima_versao_nao_recompila() {
        assert!(!desatualizado(Some("0.2.1"), None));
        assert!(!desatualizado(None, None));
    }

    /// Binário presente que não responde `--version` é instalação quebrada — reconstruir é o
    /// conserto provável.
    #[test]
    fn binario_que_nao_responde_e_reconstruido() {
        assert!(desatualizado(None, Some("0.3.0")));
    }
}
