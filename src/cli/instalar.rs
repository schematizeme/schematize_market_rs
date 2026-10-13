//! O despacho de `install` / `remove` / `switch` entre **apps da casa** e **linguagens**.
//!
//! **O quê:** decide, pelo nome, se o alvo é um app do ecossistema ou um runtime, e chama
//! quem sabe fazer aquilo.
//!
//! **Onde:** [`crate::cli::args::Cmd::Install`], `Remove` e `Switch`.
//!
//! Sem esta camada, `install schematize-deployer` cairia no instalador de linguagem e
//! terminaria num "não conheço a linguagem schematize-deployer" — verdade técnica e resposta
//! inútil.

use market::appsdacasa::{externo, instalar_app_da_casa, registrar_no_menu, EXTERNOS};
use market::atualizar::instalacao::Resultado;
use market::environments;

/// **O quê:** instala um app da casa, uma linguagem ou uma ferramenta de dev — ou, com
/// `--all`, todos os apps do ecossistema.
/// **Onde:** `schematize-market install <what>` / `install --all`.
pub(crate) fn install_cmd(
    what: Option<&str>,
    all: bool,
    method: Option<String>,
    dry_run: bool,
    yes: bool,
) -> Result<(), String> {
    match (what, all) {
        (_, true) => instalar_todos(dry_run, yes),
        (Some(w), false) if externo(w).is_some() => instalar_app(w, dry_run, yes),
        (Some(w), false) => environments::install(w, method, dry_run, yes),
        (None, false) => Err("diga o que instalar (ex.: `install go`) ou use `--all`".into()),
    }
}

/// **O quê:** os binários do conjunto `--all` — os apps da tabela [`EXTERNOS`].
/// **Onde:** [`instalar_todos`] e a suíte.
fn conjunto_completo() -> Vec<&'static str> {
    EXTERNOS.iter().map(|a| a.bin).collect()
}

/// **O quê:** instala todos os apps do ecossistema, pedindo UM consentimento (ou `-y`). Cada
/// app é independente: a falha de um não impede os outros, e o erro sobe no fim (exit != 0).
///
/// **Onde:** [`install_cmd`] com `--all` — o que a dica do `update` numa máquina limpa manda rodar.
fn instalar_todos(dry_run: bool, yes: bool) -> Result<(), String> {
    let nomes = conjunto_completo();
    println!("Vou instalar os {} apps do ecossistema: {}", nomes.len(), nomes.join(", "));
    if !dry_run && !environments::consentir(yes)? {
        println!("cancelado — nada foi feito.");
        return Ok(());
    }
    let mut falhas = Vec::new();
    for n in &nomes {
        if let Err(e) = instalar_app(n, dry_run, true) {
            eprintln!("erro em {n}: {e}");
            falhas.push(*n);
        }
    }
    if falhas.is_empty() {
        Ok(())
    } else {
        Err(format!("não consegui instalar: {}", falhas.join(", ")))
    }
}

/// **O quê:** remove uma linguagem ou ferramenta. **Onde:** `schematize-market remove`.
///
/// **App da casa NÃO se remove por aqui, e é deliberado:** cada app tem o próprio
/// desinstalador, que sabe da entrada de desktop, do ícone e do estado em `~`. Apagar o
/// binário daqui deixaria o ícone órfão no menu — e um ícone que abre nada é pior que um app
/// instalado.
pub(crate) fn remove_cmd(what: &str, method: Option<String>, dry_run: bool) -> Result<(), String> {
    if let Some(a) = externo(what) {
        return Err(format!(
            "`{}` é um app do ecossistema; remova-o por ele mesmo:\n    {} uninstall",
            a.bin, a.bin
        ));
    }
    environments::remove(what, method, dry_run)
}

/// **O quê:** instala um app externo — binário pronto do release; compila só se não houver —,
/// com aviso e confirmação.
///
/// **Onde:** [`install_cmd`], quando o alvo está na tabela [`EXTERNOS`].
///
/// **O que mudou no ADR-0013:** isto executava `curl -fsSL <install.sh> | bash -s -- --flag`.
/// Agora o market instala ele mesmo ([`crate::appsdacasa::instalar_app_da_casa`]): release primeiro, fonte só sem binário — e porque o
/// `install.sh` passou a delegar para cá: manter o `curl` faria os dois se chamarem em círculo.
fn instalar_app(nome: &str, dry_run: bool, yes: bool) -> Result<(), String> {
    // Deny-by-default: só o que está na tabela. Nome desconhecido não vira alvo de build.
    let Some(a) = externo(nome) else {
        let nomes: Vec<&str> = EXTERNOS.iter().map(|x| x.bin).collect();
        return Err(format!("não conheço o app `{nome}`. Os que existem: {}", nomes.join(", ")));
    };

    // D12: quem decide é a cópia CANÔNICA (`~/.cargo/bin`) contra o release — uma cópia velha
    // achada pelo PATH não conta como instalada. O banner e a confirmação só aparecem se há o
    // que instalar.
    let sem_terminal: std::cell::Cell<Option<String>> = std::cell::Cell::new(None);
    let banner = |_motivo: &str| {
        println!("Vou instalar o `{}` — {}", a.bin, a.sobre);
        println!();
        println!("  Baixo o binário pronto do release (segundos, sem compilar). Só se o app");
        println!("  ainda não tiver binário para esta máquina é que eu compilo do fonte —");
        println!("  e aí leva minutos e pode pedir sudo para as bibliotecas de build.");
        match environments::consentir(yes) {
            Ok(c) => c,
            Err(m) => {
                sem_terminal.set(Some(m));
                false
            }
        }
    };
    let resultado = instalar_app_da_casa(a.bin, dry_run, banner)?;
    // Sem terminal e sem `-y`: não é cancelamento da pessoa, é falta de como perguntar.
    if let Some(m) = sem_terminal.take() {
        return Err(m);
    }
    match resultado {
        Resultado::JaInstalado { .. } => {
            println!("Para atualizar, rode:");
            println!("    schematize-market update");
            return Ok(());
        }
        Resultado::Cancelado => {
            println!("cancelado — nada foi feito.");
            return Ok(());
        }
        Resultado::NaoConfirmado { .. } | Resultado::SoSimulou => return Ok(()),
        Resultado::Instalado(_) => {}
    }
    registrar_no_menu(a.bin);

    // Veredito pelo ESTADO, não pelo código de saída: um build que termina 0 sem produzir um
    // binário que responde não instalou nada, e dizer "pronto" ali seria mentir. Olha a cópia
    // CANÔNICA — o PATH ainda pode apontar para outra até o próximo shell.
    let canonico = market::atualizar::APPS_GERIDOS.iter().find(|x| x.bin == a.bin);
    match canonico.and_then(|x| x.versao_instalada()) {
        Some(versao) => {
            println!("\n✓ {} {versao} instalado.", a.bin);
            let dir = market::nucleo::plataforma::install_dir();
            if let Some(aviso) =
                market::nucleo::dica_path::aviso_pos_instalacao(&dir, &market::util::home())
            {
                println!("\n{aviso}");
            }
        }
        None => println!(
            "\nO build terminou, mas o `{}` ainda não responde. Rode \
             `schematize-market list` para ver o estado.",
            a.bin
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O despacho é o ponto do módulo: nome de app vai para o caminho de app, e nome de
    /// linguagem NÃO. Testado pela borda que não toca rede — `remove`, que recusa app.
    #[test]
    fn remove_recusa_app_da_casa_e_diz_por_onde() {
        let e = remove_cmd("schematize-deployer", None, true).unwrap_err();
        assert!(e.contains("schematize-deployer uninstall"), "{e}");
    }

    /// Linguagem não é confundida com app: `externo()` não a reconhece, então ela segue para
    /// o instalador de ambientes.
    #[test]
    fn linguagem_nao_e_tratada_como_app() {
        assert!(externo("go").is_none());
        assert!(externo("rust").is_none());
    }

    /// `--all` cobre exatamente os 6 apps do ecossistema, todos instaláveis pelo nome.
    #[test]
    fn all_cobre_os_seis_apps_do_ecossistema() {
        let n = conjunto_completo();
        assert_eq!(n.len(), 6, "{n:?}");
        for b in ["deployer", "optimizer", "database", "git", "skills", "overdev"] {
            assert!(n.contains(&format!("schematize-{b}").as_str()), "falta {b}");
        }
    }

    /// Sem alvo e sem `--all` é erro acionável, não pânico nem no-op.
    #[test]
    fn sem_alvo_e_sem_all_diz_o_que_fazer() {
        let e = install_cmd(None, false, None, true, true).unwrap_err();
        assert!(e.contains("--all"), "{e}");
    }

    /// Nome fora da tabela não vira flag para um script com sudo.
    #[test]
    fn nome_desconhecido_nao_vira_flag() {
        let e = instalar_app("--rm-rf", false, true).unwrap_err();
        assert!(e.contains("não conheço o app"), "{e}");
    }
}
