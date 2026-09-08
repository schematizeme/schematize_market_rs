//! Os comandos do caminho de atualização: `update`, `pin`, `unpin`, `status`, `run`.
//!
//! **O quê:** traduz o que a pessoa digitou em chamadas de [`market::atualizar`], e imprime o
//! `status` — a única saída deste módulo que não vem de lá.
//!
//! **Onde:** o despacho de [`crate::cli::args::Cmd`] no `main`.
//!
//! ## Procedência
//!
//! Sucede o `main.rs` do `schematize_updater_rs` (ADR-0013). O que era `schematize-updater
//! update | status | pin | unpin | run` passa a ser `schematize-market <o mesmo>` — mesmos
//! verbos, de propósito: quem tem o comando na mão só troca o nome do programa.

use market::atualizar::{self, versao};
use market::i18n::{t, tf};
use market::nucleo::plataforma;

/// **O quê:** `schematize-market update [--force] [--dry-run]`.
/// **Onde:** o despacho do `main`.
pub(crate) fn update_cmd(force: bool, dry_run: bool) -> Result<(), String> {
    atualizar::install_or_update(force, dry_run)
}

/// **O quê:** `schematize-market pin <versão>` — e `pin latest` DESAFIXA.
///
/// **Onde:** o despacho do `main`.
///
/// A tradução do que se digitou é de [`versao::interpretar_pin`], que é pura e testada: `pin
/// latest` gravava a string literal `"latest"` e o alvo virava a tag inexistente `vlatest`,
/// silenciosamente, até a próxima atualização.
pub(crate) fn pin_cmd(entrada: &str) -> Result<(), String> {
    match versao::interpretar_pin(entrada)? {
        Some(v) => versao::write_pin(Some(&v))
            .map(|()| println!("{}", tf("up.pin_set", &[("version", &v)]))),
        None => unpin_cmd(),
    }
}

/// **O quê:** `schematize-market unpin` — volta a seguir a última publicada.
/// **Onde:** o despacho do `main`, e [`pin_cmd`] quando a entrada significa "latest".
pub(crate) fn unpin_cmd() -> Result<(), String> {
    versao::write_pin(None).map(|()| println!("{}", t("up.pin_cleared")))
}

/// **O quê:** `schematize-market status` — versões, plataforma, pin e o estado de cada
/// componente gerido.
///
/// **Onde:** o despacho do `main`.
///
/// **Por que os componentes têm linha própria:** cada um tem versão e ciclo próprios. Sem
/// isso, quem roda `status` para ver "o que eu tenho e o que está disponível" simplesmente não
/// enxerga metade do que o programa gerencia — e um gestor de versão que esconde o que
/// gerencia manda a pessoa investigar por fora, que é o oposto de existir.
///
/// **Só aparece o que está instalado:** anunciar um app que a pessoa não pediu, num comando de
/// diagnóstico, seria propaganda no lugar errado.
pub(crate) fn status_cmd(json: bool) -> Result<(), String> {
    if json {
        return status_json();
    }
    let os_s = match plataforma::os() {
        plataforma::Os::Linux => format!("Linux ({:?})", plataforma::linux_family()),
        plataforma::Os::Mac => "macOS".to_string(),
        plataforma::Os::Windows => "Windows".to_string(),
    };
    let campo = |rotulo: String, valor: String| println!("{rotulo:<20}: {valor}");

    campo("schematize-market".into(), format!("v{}", env!("CARGO_PKG_VERSION")));
    campo(t("up.st_platform"), format!("{os_s} / {}", std::env::consts::ARCH));
    campo(
        t("up.st_prebuilt"),
        if plataforma::asset_names().is_some() {
            t("up.st_prebuilt_yes")
        } else {
            t("up.st_prebuilt_no")
        },
    );
    campo(t("up.st_app"), versionado(versao::installed_app_version()));
    campo(t("up.st_latest"), versionado(versao::latest_app_version()));

    for app in atualizar::estado_dos_apps() {
        // O próprio market já tem a primeira linha, com a versão compilada neste binário —
        // repeti-lo aqui diria a mesma coisa duas vezes.
        if app.politica == atualizar::Politica::Sempre {
            continue;
        }
        // `versao_instalada` e não o caminho direto: numa máquina que instalou antes da
        // renomeação do ADR-0012, o arquivo tem o nome ANTIGO — e dizer "nenhum" sobre um app
        // que está lá é a pior resposta possível num comando de diagnóstico.
        campo(app.bin.to_string(), versionado(app.versao_instalada()));
        campo(format!("  {}", t("up.st_latest")), versionado(versao::latest_version_of(app.repo)));
    }

    if let Some(p) = versao::read_pin() {
        campo(t("up.st_pinned"), p);
    }
    campo(t("up.st_dir"), plataforma::install_dir().display().to_string());
    Ok(())
}

/// **O quê:** o mesmo estado do [`status_cmd`], em JSON com chaves ESTÁVEIS.
///
/// **Onde:** quem PARSEIA — hoje a janela do market (`schematize_updater_gui_rs`), amanhã
/// qualquer script ou painel.
///
/// ## Por que este comando existe, e por que ele não é enfeite
///
/// A tabela humana do `status` passa pelo catálogo i18n: os rótulos são `plataforma` em
/// português, `platform` em inglês, `プラットフォーム` em japonês. A janela lia aquela tabela
/// casando o rótulo em português — então ela funcionava **em um idioma e devolvia vazio nos
/// outros dezenove, sem erro nenhum**. Parsear saída feita para humano é contrato de mentira:
/// ele passa no teste de quem escreveu e falha na máquina de quem usa.
///
/// **As chaves aqui NUNCA são traduzidas.** É o que as torna contrato. Os VALORES que são
/// prosa (a plataforma, o "sim/não" do binário pronto) ficam de fora de propósito — quem
/// consome isto quer dados, e um booleano não precisa de tradução.
///
/// **Por que JSON escrito à mão e não `serde_json::to_string`:** o shape é o contrato, e
/// escrevê-lo explicitamente faz uma mudança nele aparecer no diff. Uma struct com `Serialize`
/// mudaria o JSON sempre que alguém renomeasse um campo — em silêncio.
fn status_json() -> Result<(), String> {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let campo = |v: Option<String>| match v {
        Some(v) => format!("\"{}\"", esc(&v)),
        None => "null".to_string(),
    };

    let mut apps = Vec::new();
    for app in atualizar::estado_dos_apps() {
        apps.push(format!(
            "    {{\"bin\": \"{}\", \"repo\": \"{}\", \"installed\": {}, \"latest\": {}}}",
            esc(app.bin),
            esc(app.repo),
            campo(app.versao_instalada()),
            campo(versao::latest_version_of(app.repo))
        ));
    }

    println!("{{");
    println!("  \"market\": \"{}\",", env!("CARGO_PKG_VERSION"));
    println!("  \"os\": \"{}\",", std::env::consts::OS);
    println!("  \"arch\": \"{}\",", std::env::consts::ARCH);
    println!("  \"prebuilt\": {},", plataforma::asset_names().is_some());
    println!("  \"app_installed\": {},", campo(versao::installed_app_version()));
    println!("  \"app_latest\": {},", campo(versao::latest_app_version()));
    println!("  \"pin\": {},", campo(versao::read_pin()));
    println!("  \"install_dir\": \"{}\",", esc(&plataforma::install_dir().display().to_string()));
    println!("  \"apps\": [");
    println!("{}", apps.join(",\n"));
    println!("  ]");
    println!("}}");
    Ok(())
}

/// **O quê:** `schematize-market run` — lança a GUI instalada pelo caminho absoluto.
/// **Onde:** o despacho do `main`, e o `.desktop` que o updater escrevia.
pub(crate) fn run_cmd() -> Result<(), String> {
    plataforma::launch_app()
}

/// **O quê:** uma versão para exibição, ou a palavra traduzida para "nenhum".
/// **Onde:** cada linha de [`status_cmd`].
fn versionado(v: Option<String>) -> String {
    v.map(|v| format!("v{v}")).unwrap_or_else(|| t("up.none"))
}

#[cfg(test)]
mod tests {
    /// **O SHAPE do `status --json` é CONTRATO**, e este teste é onde ele mora.
    ///
    /// **O que ele trava:** as chaves de topo e as de cada app. Quem consome isto — hoje a
    /// janela do market, amanhã um script — quebra em silêncio se uma chave sumir ou mudar de
    /// nome, e "em silêncio" é o ponto: um JSON sem a chave esperada devolve vazio, não erro.
    ///
    /// **Por que ler o próprio fonte:** executar o comando exigiria rede (ele consulta a última
    /// versão publicada) e uma máquina com apps instalados. O que precisa ser travado não é o
    /// VALOR, é o NOME — e esse está no código.
    #[test]
    fn o_shape_do_json_e_contrato() {
        let fonte = include_str!("atualizar.rs");
        let corpo = fonte.split("fn status_json").nth(1).expect("a função existe");
        let corpo = corpo.split("\n}\n").next().unwrap();

        for chave in [
            "market",
            "os",
            "arch",
            "prebuilt",
            "app_installed",
            "app_latest",
            "pin",
            "install_dir",
            "apps",
        ] {
            assert!(
                corpo.contains(&format!("\\\"{chave}\\\"")),
                "a chave de topo `{chave}` sumiu do contrato do `status --json`"
            );
        }
        for chave in ["bin", "repo", "installed", "latest"] {
            assert!(
                corpo.contains(&format!("\\\"{chave}\\\"")),
                "a chave `{chave}` sumiu de cada app no `status --json`"
            );
        }

        // E as chaves NUNCA passam pelo catálogo: se alguém envolver uma em `t()`/`tf()`, o
        // JSON volta a mudar de forma por idioma — que é exatamente o bug que este comando
        // existe para consertar.
        assert!(
            !corpo.contains("t(\"") && !corpo.contains("tf(\""),
            "o `status --json` não pode traduzir NADA — as chaves são o contrato"
        );
    }
}
