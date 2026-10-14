//! **schematize-market** — instalar e remover programas. O binário só monta a plataforma Unix e
//! as receitas Unix e entrega à lib (`market::cli::run`); o Windows tem o seu `main` em
//! `schematize_windows_rs` (ADR-0026).

/// Devolve o `SIGPIPE` ao padrão do Unix — sem isto, `… | head` panica. Ver o Deployer.
#[cfg(unix)]
fn restaurar_sigpipe() {
    // SAFETY: chamada única, antes de qualquer thread, restaurando o handler padrão do SO.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

/// Ponto de entrada Unix: costuras reais + receitas Unix, e o código de saída da CLI.
#[cfg(unix)]
fn main() {
    restaurar_sigpipe();
    let plat = schematize_plataforma::seams::Plataforma::unix();
    let code = market::cli::run(std::env::args_os(), &plat, &market::receitas_unix::ReceitasUnix);
    std::process::exit(code);
}

/// Fora do Unix este binário não existe: o Windows usa o `schematize-market.exe` do repo
/// `schematize_windows_rs`, que fornece o próprio `Plataforma` e `Receitas`.
#[cfg(not(unix))]
fn main() {
    eprintln!("erro: este binário é Unix-only; no Windows use o schematize_windows_rs.");
    std::process::exit(2);
}
