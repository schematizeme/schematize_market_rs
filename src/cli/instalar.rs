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

use market::appsdacasa::{
    descobrir_app, externo, instalar_do_fonte, registrar_no_menu, Estado, EXTERNOS,
};
use market::environments;

/// **O quê:** instala um app da casa, uma linguagem ou uma ferramenta de dev.
/// **Onde:** `schematize-market install <what>`.
pub(crate) fn install_cmd(
    what: &str,
    method: Option<String>,
    dry_run: bool,
    yes: bool,
) -> Result<(), String> {
    match externo(what) {
        Some(_) => instalar_app(what, dry_run, yes),
        None => environments::install(what, method, dry_run, yes),
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

/// **O quê:** instala um app externo COMPILANDO do fonte, com aviso e confirmação.
///
/// **Onde:** [`install_cmd`], quando o alvo está na tabela [`EXTERNOS`].
///
/// **O que mudou no ADR-0013:** isto executava `curl -fsSL <install.sh> | bash -s -- --flag`.
/// Agora o market compila ele mesmo ([`instalar_do_fonte`]) — porque ele já sabe, e porque o
/// `install.sh` passou a delegar para cá: manter o `curl` faria os dois se chamarem em círculo.
fn instalar_app(nome: &str, dry_run: bool, yes: bool) -> Result<(), String> {
    // Deny-by-default: só o que está na tabela. Nome desconhecido não vira alvo de build.
    let Some(a) = externo(nome) else {
        let nomes: Vec<&str> = EXTERNOS.iter().map(|x| x.bin).collect();
        return Err(format!("não conheço o app `{nome}`. Os que existem: {}", nomes.join(", ")));
    };

    if let Estado::Instalado { versao, caminho } = descobrir_app(a.bin) {
        println!("{} {versao} já está instalado em {}", a.bin, caminho.display());
        println!("Para atualizar, rode:");
        println!("    schematize-market update");
        return Ok(());
    }
    if dry_run {
        println!("(dry-run) compilaria o `{}` do fonte e o poria no menu.", a.bin);
        return Ok(());
    }

    println!("Vou instalar o `{}` — {}", a.bin, a.sobre);
    println!();
    println!("  Isto COMPILA do fonte e leva minutos. Precisa de rede, e pode pedir sudo");
    println!("  para as bibliotecas de build do sistema.");
    if !yes && !environments::confirm() {
        println!("cancelado — nada foi feito.");
        return Ok(());
    }

    instalar_do_fonte(a.bin)?;
    registrar_no_menu(a.bin);

    // Veredito pelo ESTADO, não pelo código de saída: um build que termina 0 sem produzir um
    // binário que responde não instalou nada, e dizer "pronto" ali seria mentir.
    match descobrir_app(a.bin) {
        Estado::Instalado { versao, .. } => println!("\n✓ {} {versao} instalado.", a.bin),
        _ => println!(
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

    /// Nome fora da tabela não vira flag para um script com sudo.
    #[test]
    fn nome_desconhecido_nao_vira_flag() {
        let e = instalar_app("--rm-rf", false, true).unwrap_err();
        assert!(e.contains("não conheço o app"), "{e}");
    }
}
