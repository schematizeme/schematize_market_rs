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

use market::appsdacasa::{como_instalar_app, descobrir_app, externo, Estado, EXTERNOS};
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

/// **O quê:** instala um app externo compilando do fonte, com aviso e confirmação.
/// **Onde:** [`install_cmd`], quando o alvo está na tabela [`EXTERNOS`].
///
/// **Herda o terminal de propósito:** a compilação leva minutos e o `install.sh` pode pedir
/// sudo para as libs de build. Capturar a saída deixaria a pessoa olhando um cursor parado, e
/// o pedido de senha não teria onde aparecer.
fn instalar_app(nome: &str, dry_run: bool, yes: bool) -> Result<(), String> {
    // Deny-by-default: só o que está na tabela. Nome desconhecido não vira flag inventada num
    // script que roda com sudo.
    let Some(a) = externo(nome) else {
        let nomes: Vec<&str> = EXTERNOS.iter().map(|x| x.bin).collect();
        return Err(format!("não conheço o app `{nome}`. Os que existem: {}", nomes.join(", ")));
    };
    let cmd = como_instalar_app(a.flag);

    if let Estado::Instalado { versao, caminho } = descobrir_app(a.bin) {
        println!("{} {versao} já está instalado em {}", a.bin, caminho.display());
        println!("Para atualizar, rode o mesmo comando — ele recompila do fonte:");
        println!("    {cmd}");
        return Ok(());
    }
    if dry_run {
        println!("(dry-run) rodaria: {cmd}");
        return Ok(());
    }

    println!("Vou instalar o `{}` — {}", a.bin, a.sobre);
    println!();
    println!("  Isto COMPILA do fonte e leva minutos. Precisa de rede, e o instalador");
    println!("  pode pedir sudo para as bibliotecas de build do sistema.");
    println!("  Comando: {cmd}");
    if !yes && !environments::confirm() {
        println!("cancelado — nada foi feito.");
        return Ok(());
    }

    let st = std::process::Command::new("bash")
        .arg("-c")
        .arg(&cmd)
        .status()
        .map_err(|e| format!("não consegui iniciar a instalação: {e}"))?;
    if !st.success() {
        return Err(format!(
            "a instalação terminou com erro ({}). O market segue funcionando; o `{}` é opcional.",
            st.code().map(|c| c.to_string()).unwrap_or_else(|| "sinal".into()),
            a.bin
        ));
    }
    // Veredito pelo ESTADO, não pelo código de saída: o `install.sh` é best-effort com os
    // apps opcionais, então ele pode sair 0 sem ter instalado nada.
    match descobrir_app(a.bin) {
        Estado::Instalado { versao, .. } => println!("\n✓ {} {versao} instalado.", a.bin),
        _ => println!(
            "\nO instalador terminou, mas o `{}` ainda não responde. Rode \
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
