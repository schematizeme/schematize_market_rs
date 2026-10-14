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

use crate::appsdacasa::{externo, instalar_app_da_casa, registrar_no_menu, EXTERNOS};
use crate::atualizar::instalacao::Resultado;
use crate::atualizar::kit::{self, AcaoMarket, Desfecho};
use crate::atualizar::selfupdate;
use crate::ctx::Ctx;
use crate::environments::{self, DesfechoFerramenta};
use crate::nucleo::i18n::{t, tf};

/// **O quê:** instala um app da casa, uma linguagem ou uma ferramenta de dev — ou, com
/// `--all`, todos os apps do ecossistema.
/// **Onde:** `schematize-market install <what>` / `install --all`.
pub(crate) fn install_cmd(
    ctx: &Ctx,
    what: Option<&str>,
    all: bool,
    method: Option<String>,
    dry_run: bool,
    yes: bool,
) -> Result<(), String> {
    let r = match (what, all) {
        (_, true) => instalar_todos(ctx, dry_run, yes),
        (Some(w), false) if externo(w).is_some() => instalar_app(ctx, w, dry_run, yes).map(|_| ()),
        (Some(w), false) => environments::install(ctx, w, method, dry_run, yes),
        (None, false) => Err(t("install.what_missing")),
    };
    // O market com janela vem com QUALQUER coisa instalada; falhar aqui só avisa (uma linha).
    if r.is_ok() && !dry_run && !all {
        selfupdate::garantir_janela_do_market(ctx);
    }
    r
}

/// **O quê:** os binários do conjunto `--all` — os apps da tabela [`EXTERNOS`].
/// **Onde:** [`instalar_todos`] e a suíte.
fn conjunto_completo() -> Vec<&'static str> {
    EXTERNOS.iter().map(|a| a.bin).collect()
}

/// **O quê:** as ferramentas de IA que o `install --all` também instala (onde houver suporte).
/// **Onde:** [`instalar_todos`].
const FERRAMENTAS_DO_KIT: [&str; 3] = ["claude", "codex", "cursor"];

/// **O quê:** instala o KIT COMPLETO — os apps da tabela [`EXTERNOS`] (CLI + janela de cada um),
/// as ferramentas de IA ([`FERRAMENTAS_DO_KIT`]) e o próprio market com a janela dele — na última versão, pedindo UM consentimento (ou `-y`).
/// Idempotente: o que já está na última (com janela) não é refeito; o que está velho atualiza.
/// Cada item é independente: a falha de um não impede os outros; no fim vem o Resumo e, se algo
/// falhou, o erro (exit != 0) com o que fazer.
///
/// **Onde:** [`install_cmd`] com `--all` — o que a dica do `update` numa máquina limpa manda rodar.
fn instalar_todos(ctx: &Ctx, dry_run: bool, yes: bool) -> Result<(), String> {
    let mut nomes = conjunto_completo();
    nomes.extend(FERRAMENTAS_DO_KIT);
    nomes.push(MARKET);
    println!(
        "{}",
        tf("install.all_intro", &[("n", &nomes.len().to_string()), ("apps", &nomes.join(", "))])
    );
    if !dry_run && !environments::consentir(yes)? {
        println!("{}", t("install.cancelled"));
        return Ok(());
    }
    let mut itens: Vec<(String, Desfecho)> = Vec::new();
    for n in conjunto_completo() {
        let d = instalar_app(ctx, n, dry_run, true).unwrap_or_else(|e| {
            eprintln!("{}", tf("install.all_error", &[("app", n), ("error", &e)]));
            Desfecho::Falhou(e)
        });
        itens.push((n.to_string(), d));
    }
    for slug in FERRAMENTAS_DO_KIT {
        let d = match environments::instalar_ferramenta_no_kit(ctx, slug, dry_run) {
            Ok(DesfechoFerramenta::Instalada) => Desfecho::Instalado,
            Ok(DesfechoFerramenta::JaInstalada) => Desfecho::EmDia,
            Ok(DesfechoFerramenta::NadaFeito) => Desfecho::NadaFeito,
            Ok(DesfechoFerramenta::Pulada(motivo)) => {
                eprintln!("{}", tf("install.all_skipped", &[("app", slug), ("reason", &motivo)]));
                Desfecho::Pulado(motivo)
            }
            Err(e) => {
                eprintln!("{}", tf("install.all_error", &[("app", slug), ("error", &e)]));
                Desfecho::Falhou(e)
            }
        };
        itens.push((slug.to_string(), d));
    }
    let d = instalar_market(ctx, dry_run).unwrap_or_else(|e| {
        eprintln!("{}", tf("install.all_error", &[("app", MARKET), ("error", &e)]));
        Desfecho::Falhou(e)
    });
    itens.push((MARKET.to_string(), d));
    print!("{}", kit::resumo(&itens));
    kit::veredito(&itens)
}

/// O nome do próprio gestor no resumo do `install --all`.
const MARKET: &str = "schematize-market";

/// **O quê:** mantém o PRÓPRIO market e a janela dele no `install --all`: self-update se há
/// versão nova (que já traz a janela), ou só a janela quando o CLI está em dia e ela falta.
/// **Onde:** [`instalar_todos`], depois dos apps.
fn instalar_market(ctx: &Ctx, dry_run: bool) -> Result<Desfecho, String> {
    use crate::atualizar::{alvo, APPS_GERIDOS};
    let app = APPS_GERIDOS
        .iter()
        .find(|a| a.bin == MARKET)
        .ok_or("o market não está na tabela de apps geridos")?;
    let publicada = match alvo::alvo_de(ctx, app.repo) {
        alvo::Alvo::Versao(v) | alvo::Alvo::SemAsset { versao: v } => Ok(v),
        alvo::Alvo::Indisponivel(e) => Err(e),
    };
    let acao = kit::decidir_market(
        env!("CARGO_PKG_VERSION"),
        publicada.as_deref().map_err(String::as_str),
        app.janela_ausente(),
    );
    match &acao {
        AcaoMarket::AtualizarCli(v) => {
            println!("→ {MARKET}: atualizando para v{v} (com a janela).")
        }
        AcaoMarket::SoJanela(v) => println!("→ {MARKET}: a janela falta — instalando a da v{v}."),
        AcaoMarket::EmDia => println!("{MARKET} e a janela dele já estão em dia."),
        AcaoMarket::Impossivel(_) => {}
    }
    match acao {
        AcaoMarket::EmDia => Ok(Desfecho::EmDia),
        AcaoMarket::Impossivel(causa) => Err(format!(
            "não consegui saber a última versão do market ({causa}). Nada foi alterado; \
             tente de novo em alguns minutos."
        )),
        _ if dry_run => Ok(Desfecho::NadaFeito),
        AcaoMarket::AtualizarCli(v) => {
            selfupdate::atualizar_a_si_mesmo(ctx, Some(&v), alvo::metodo_de(app.repo))?;
            Ok(Desfecho::Atualizado)
        }
        AcaoMarket::SoJanela(v) => {
            selfupdate::instalar_janela_do_market(ctx, &v)?;
            println!("✓ {}", crate::nucleo::plataforma::gestor_gui_bin());
            Ok(Desfecho::Instalado)
        }
    }
}

/// **O quê:** remove uma linguagem ou ferramenta. **Onde:** `schematize-market remove`.
///
/// **App da casa NÃO se remove por aqui, e é deliberado:** cada app tem o próprio
/// desinstalador, que sabe da entrada de desktop, do ícone e do estado em `~`. Apagar o
/// binário daqui deixaria o ícone órfão no menu — e um ícone que abre nada é pior que um app
/// instalado.
pub(crate) fn remove_cmd(
    ctx: &Ctx,
    what: &str,
    method: Option<String>,
    dry_run: bool,
    yes: bool,
) -> Result<(), String> {
    if let Some(a) = externo(what) {
        return Err(format!(
            "`{}` é um app do ecossistema; remova-o por ele mesmo:\n    {} uninstall",
            a.bin, a.bin
        ));
    }
    environments::remove(ctx, what, method, dry_run, yes)
}

/// **O quê:** instala um app externo — binário pronto do release; compila só se não houver —,
/// com aviso e confirmação.
///
/// **Onde:** [`install_cmd`], quando o alvo está na tabela [`EXTERNOS`].
///
/// **O que mudou no ADR-0013:** isto executava `curl -fsSL <install.sh> | bash -s -- --flag`.
/// Agora o market instala ele mesmo ([`crate::appsdacasa::instalar_app_da_casa`]): release primeiro, fonte só sem binário — e porque o
/// `install.sh` passou a delegar para cá: manter o `curl` faria os dois se chamarem em círculo.
fn instalar_app(ctx: &Ctx, nome: &str, dry_run: bool, yes: bool) -> Result<Desfecho, String> {
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
    let existia = crate::atualizar::APPS_GERIDOS.iter().any(|x| x.bin == a.bin && x.instalado());
    let resultado = instalar_app_da_casa(ctx, a.bin, dry_run, banner)?;
    // Sem terminal e sem `-y`: não é cancelamento da pessoa, é falta de como perguntar.
    if let Some(m) = sem_terminal.take() {
        return Err(m);
    }
    match resultado {
        Resultado::JaInstalado { .. } => {
            println!("Para atualizar, rode:");
            println!("    schematize-market update");
            return Ok(Desfecho::EmDia);
        }
        Resultado::Cancelado => {
            println!("{}", t("install.cancelled"));
            return Ok(Desfecho::NadaFeito);
        }
        Resultado::NaoConfirmado { .. } => return Ok(Desfecho::NaoConfirmado),
        Resultado::SoSimulou => return Ok(Desfecho::NadaFeito),
        Resultado::Instalado(_) => {}
    }
    registrar_no_menu(a.bin);

    // Veredito pelo ESTADO, não pelo código de saída: um build que termina 0 sem produzir um
    // binário que responde não instalou nada, e dizer "pronto" ali seria mentir. Olha a cópia
    // CANÔNICA — o PATH ainda pode apontar para outra até o próximo shell.
    let canonico = crate::atualizar::APPS_GERIDOS.iter().find(|x| x.bin == a.bin);
    match canonico.and_then(|x| x.versao_instalada()) {
        Some(versao) => {
            println!("\n✓ {} {versao} instalado.", a.bin);
            let dir = crate::nucleo::plataforma::install_dir();
            if let Some(aviso) = crate::nucleo::dica_path::aviso_pos_instalacao(ctx, &dir) {
                println!("\n{aviso}");
            }
        }
        None => {
            return Err(format!(
                "o build terminou, mas o `{}` ainda não responde. Rode \
                 `schematize-market list` para ver o estado.",
                a.bin
            ));
        }
    }
    Ok(if existia { Desfecho::Atualizado } else { Desfecho::Instalado })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O despacho é o ponto do módulo: nome de app vai para o caminho de app, e nome de
    /// linguagem NÃO. Testado pela borda que não toca rede — `remove`, que recusa app.
    #[test]
    fn remove_recusa_app_da_casa_e_diz_por_onde() {
        let e = remove_cmd(&Ctx::de_teste(), "schematize-deployer", None, true, false).unwrap_err();
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
        // E o kit completo soma o próprio market (CLI + janela) — ver `instalar_todos`.
        assert_eq!(MARKET, "schematize-market");
        let n = conjunto_completo();
        assert_eq!(n.len(), 6, "{n:?}");
        for b in ["deployer", "optimizer", "database", "git", "skills", "overdev"] {
            assert!(n.contains(&format!("schematize-{b}").as_str()), "falta {b}");
        }
    }

    /// Sem alvo e sem `--all` é erro acionável, não pânico nem no-op.
    #[test]
    fn sem_alvo_e_sem_all_diz_o_que_fazer() {
        let e = install_cmd(&Ctx::de_teste(), None, false, None, true, true).unwrap_err();
        assert!(e.contains("--all"), "{e}");
    }

    /// Nome fora da tabela não vira flag para um script com sudo.
    #[test]
    fn nome_desconhecido_nao_vira_flag() {
        let e = instalar_app(&Ctx::de_teste(), "--rm-rf", false, true).unwrap_err();
        assert!(e.contains("não conheço o app"), "{e}");
    }
}
