//! Sem terminal e sem `-y`, NENHUM comando com confirmação `[s/N]` cancela em silêncio.
//!
//! **O quê:** roda o binário real com stdin fechado (sem TTY) e confere que `remove` e `install`
//! saem com erro que nomeia o `-y` — em vez de "abortado" com exit 0.
//!
//! **Onde:** `cargo test`; complementa os testes unitários de `consentir_com`.

use std::process::{Command, Stdio};

/// **O quê:** o binário compilado, ao lado do executável de teste. **Onde:** [`rodar`].
fn bin() -> std::path::PathBuf {
    let mut p = std::env::current_exe().expect("o teste tem caminho");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("schematize-market")
}

/// **O quê:** roda o market sem TTY, num `HOME` descartável, e devolve (sucesso, stdout, stderr).
/// **Onde:** os testes daqui.
fn rodar(args: &[&str]) -> (bool, String, String) {
    let home = std::env::temp_dir().join(format!("market-consent-{}", std::process::id()));
    std::fs::create_dir_all(&home).expect("home temporário");
    let out = Command::new(bin())
        .args(args)
        .env("HOME", &home)
        .env("SCHEMATIZE_LANG", "pt")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("não consegui executar {}: {e}", bin().display()));
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `remove` sem `-y` e sem terminal erra pedindo `-y`; não diz "abortado" nem sai 0.
#[test]
fn remove_sem_tty_sem_yes_erra_pedindo_y() {
    let (ok, out, err) = rodar(&["remove", "claude"]);
    assert!(!ok, "deveria sair com erro; stdout: {out}");
    assert!(err.contains("-y"), "stderr deve nomear o -y: {err}");
    assert!(!out.contains("abortado"), "não pode cancelar em silêncio: {out}");
}

/// `install --all` sem `-y` e sem terminal também erra pedindo `-y` (nada é instalado).
#[test]
fn install_all_sem_tty_sem_yes_erra_pedindo_y() {
    let (ok, _out, err) = rodar(&["install", "--all"]);
    assert!(!ok);
    assert!(err.contains("-y"), "{err}");
}

/// `-y` aceito por `remove` (a flag existe); `--dry-run` nunca precisa de terminal.
#[test]
fn remove_dry_run_nao_pede_terminal() {
    let (ok, _out, err) = rodar(&["remove", "claude", "--dry-run"]);
    assert!(ok, "{err}");
}
