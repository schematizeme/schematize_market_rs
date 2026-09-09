//! **schematize-market** — instalar e remover programas.

mod cli;

use clap::Parser;
use cli::args::{Cli, Cmd};

/// Devolve o `SIGPIPE` ao padrão do Unix — sem isto, `… | head` panica. Ver o Deployer.
#[cfg(unix)]
fn restaurar_sigpipe() {
    // SAFETY: chamada única, antes de qualquer thread, restaurando o handler padrão do SO.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}
#[cfg(not(unix))]
fn restaurar_sigpipe() {}

fn main() {
    restaurar_sigpipe();
    let cli = Cli::parse();
    let r = match cli.cmd {
        Cmd::List { wait, json } => cli::lista::list_cmd(wait, json),
        Cmd::Install { what, method, dry_run, yes } => {
            cli::instalar::install_cmd(&what, method, dry_run, yes)
        }
        Cmd::Remove { what, method, dry_run } => cli::instalar::remove_cmd(&what, method, dry_run),
        Cmd::Switch { lang, to, dry_run, yes } => {
            market::environments::switch(&lang, &to, dry_run, yes)
        }
        Cmd::Update { force, dry_run } => cli::atualizar::update_cmd(force, dry_run),
        Cmd::Pin { version } => cli::atualizar::pin_cmd(&version),
        Cmd::Unpin => cli::atualizar::unpin_cmd(),
        Cmd::Status { json } => cli::atualizar::status_cmd(json),
        Cmd::Run => cli::atualizar::run_cmd(),
        Cmd::Desktop { install, remove } => cli::lista::desktop_cmd(install, remove),
    };
    if let Err(e) = r {
        eprintln!("erro: {e}");
        std::process::exit(1);
    }
}
