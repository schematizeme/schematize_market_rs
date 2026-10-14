//! A camada de CLI do market, exposta como biblioteca (ADR-0026 F4a).
//!
//! **O quê:** [`run`] é o `main` inteiro: parse dos argumentos, despacho, a linha `erro:` e o
//! código de saída. Quem fornece o SO (`Plataforma` + `Receitas`) é o binário — o do Unix
//! neste repo, o do Windows em `schematize_windows_rs`.
//!
//! **Onde:** `src/main.rs` e o `main` do repo Windows.
pub(crate) mod args;
pub(crate) mod atualizar;
pub(crate) mod instalar;
pub(crate) mod lista;
pub(crate) mod listajson;

use crate::ctx::Ctx;
use crate::receitas::Receitas;
use args::{Cli, Cmd};
use clap::Parser;
use schematize_plataforma::seams::Plataforma;
use std::ffi::OsString;

/// **O quê:** executa a CLI do market com `args` (o primeiro é o nome do programa, como em
/// `std::env::args_os()`) sobre a plataforma e as receitas dadas, e devolve o código de saída:
/// `0` ok, `1` erro do comando (impresso como `erro: <causa>` no stderr), e o código do clap
/// para uso inválido (`2`) — `--help`/`--version` saem com `0` pelo stdout.
///
/// **Onde:** o `main` de cada binário do market (Unix aqui; Windows em outro repo).
pub fn run(
    args: impl IntoIterator<Item = OsString>,
    plat: &Plataforma,
    receitas: &dyn Receitas,
) -> i32 {
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => {
            // `print` manda ajuda/versão ao stdout e erro de uso ao stderr, como o `parse`.
            if let Err(io) = e.print() {
                eprintln!("erro: não consegui imprimir a mensagem do clap: {io}");
            }
            return e.exit_code();
        }
    };
    let ctx = Ctx::novo(plat, receitas);
    match despachar(&ctx, cli.cmd) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("erro: {e}");
            1
        }
    }
}

/// **O quê:** liga cada subcomando à função que o executa. **Onde:** [`run`].
fn despachar(ctx: &Ctx, cmd: Cmd) -> Result<(), String> {
    match cmd {
        Cmd::List { wait, json } => lista::list_cmd(ctx, wait, json),
        Cmd::Install { what, all, method, dry_run, yes } => {
            instalar::install_cmd(ctx, what.as_deref(), all, method, dry_run, yes)
        }
        Cmd::Remove { what, method, dry_run, yes } => {
            instalar::remove_cmd(ctx, &what, method, dry_run, yes)
        }
        Cmd::Switch { lang, to, dry_run, yes } => {
            crate::environments::switch(ctx, &lang, &to, dry_run, yes)
        }
        Cmd::Update { force, dry_run } => atualizar::update_cmd(ctx, force, dry_run),
        Cmd::Pin { version } => atualizar::pin_cmd(&version),
        Cmd::Unpin => atualizar::unpin_cmd(),
        Cmd::Status { json } => atualizar::status_cmd(ctx, json),
        Cmd::Run => atualizar::run_cmd(),
        Cmd::Desktop { install, remove } => lista::desktop_cmd(ctx, install, remove),
    }
}
