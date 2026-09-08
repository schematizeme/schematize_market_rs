//! A lista única do market, e a integração com o desktop.
//!
//! **O quê:** `list` mostra linguagens, ferramentas de dev e os apps da casa numa tela só;
//! `desktop` põe o ícone no menu de aplicativos.
//!
//! **Onde:** [`crate::cli::args::Cmd::List`] e [`crate::cli::args::Cmd::Desktop`].
//!
//! ## Por que uma lista e não duas
//!
//! O pedido foi *"deployer e optimizer deveriam aparecer no env para instalar"*. A leitura
//! rasa seria somar os apps à tabela do `env`. A leitura certa é que `env` e `apps` eram o
//! mesmo produto partido em dois comandos — e é por isso que este app existe (ADR-0012).

use market::appsdacasa::{descobrir_app, Estado, EXTERNOS};
use market::i18n::t;
use market::nucleo::{desktop, icone, util};

/// **O quê:** imprime tudo que este app instala, e o estado de cada coisa.
/// **Onde:** `schematize-market list`, e o lançador do menu de aplicativos (com `--wait`).
pub(crate) fn list_cmd(wait: bool) -> Result<(), String> {
    market::environments::list();
    println!();
    secao_apps();
    if wait {
        esperar_tecla();
    }
    Ok(())
}

/// **O quê:** a seção dos apps do ecossistema dentro da lista.
/// **Onde:** [`list_cmd`].
fn secao_apps() {
    println!("{}", t("market.apps_header"));
    for a in EXTERNOS {
        let estado = match descobrir_app(a.bin) {
            Estado::Instalado { versao, .. } => format!("{} {versao}", t("env.installed")),
            // Quebrado é DITO, não somado a "ausente": um binário que está lá e não responde
            // é um problema diferente de um que não existe, e a saída tem de separar os dois.
            Estado::Quebrado { .. } => t("market.broken"),
            Estado::Ausente => t("env.not_installed"),
        };
        println!("  {:<22} {:<24} {}", a.bin, estado, a.sobre);
    }
}

/// **O quê:** segura a janela aberta até uma tecla. **Onde:** `list --wait`, chamado pelo
/// `.desktop`. Sem isto, abrir pelo ícone mostraria a lista e fecharia o terminal na mesma
/// hora — o §37.48 chama isso de bug do software, não de erro de quem clicou.
fn esperar_tecla() {
    use std::io::{BufRead, Write};
    print!("\n{} ", t("market.press_enter"));
    let _ = std::io::stdout().flush();
    let mut l = String::new();
    let _ = std::io::stdin().lock().read_line(&mut l);
}

/// **O quê:** instala ou remove o ícone e a entrada no menu de aplicativos.
/// **Onde:** `schematize-market desktop --install|--remove`, e o `install.sh`.
pub(crate) fn desktop_cmd(install: bool, remove: bool) -> Result<(), String> {
    let home = util::home();
    if remove {
        let havia = desktop::remover(&home)?;
        println!("{}", if havia { "removido." } else { "não havia entrada." });
        return Ok(());
    }
    if !install {
        let f = desktop::arquivo_desktop(&home);
        println!("{}: {}", if f.exists() { "instalado" } else { "ausente" }, f.display());
        return Ok(());
    }
    icone::install_all(&home).map_err(|e| format!("não consegui gravar o ícone: {e}"))?;
    let f = desktop::instalar(&home, std::path::Path::new(&util::self_exe()))?;
    println!("✓ {}", f.display());
    Ok(())
}
