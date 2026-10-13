//! Dica de PATH — diz, sem culpar ninguém, como pôr a pasta de instalação no PATH.
//!
//! **O quê:** funções PURAS que decidem se a pasta onde o market instala (`~/.cargo/bin`) está
//! no PATH e, se não está, qual é a ÚNICA linha que resolve no shell da pessoa.
//!
//! **Onde:** [`aviso_pos_instalacao`], chamado ao fim de `install`/`update`
//! (`atualizar::componentes::finish` e `cli::instalar::instalar_app`).
//!
//! **Por que existe (anti-padrão 48):** num CachyOS recém-instalado (rustup do pacman, shell
//! fish) a pasta nunca entrou no PATH e o aviso "reabra o terminal" não resolvia nada. Isto
//! não edita rc de shell: o fish não tem rc que o market conheça, então dizemos o comando.

use std::path::Path;

/// **O quê:** normaliza uma entrada de PATH para comparar: expande `~` e `$HOME`, tira barras
/// finais. **Onde:** [`esta_no_path`].
fn normalizar(entrada: &str, home: &str) -> String {
    let mut e = entrada.trim().to_string();
    for pre in ["~", "$HOME", "${HOME}"] {
        if e == pre || e.starts_with(&format!("{pre}/")) {
            e = format!("{home}{}", &e[pre.len()..]);
            break;
        }
    }
    while e.len() > 1 && e.ends_with('/') {
        e.pop();
    }
    e
}

/// **O quê:** `true` se `dir` aparece em `path` (lista separada por `:`), ignorando barra final
/// e a grafia `~`/`$HOME`/absoluto. **Onde:** [`orientar_path`].
pub fn esta_no_path(path: &str, dir: &Path, home: &Path) -> bool {
    let home = home.to_string_lossy();
    let alvo = normalizar(&dir.to_string_lossy(), &home);
    path.split(':').filter(|p| !p.trim().is_empty()).any(|p| normalizar(p, &home) == alvo)
}

/// **O quê:** a orientação para quando `dir` não está no PATH: a linha exata para o shell em
/// `shell` (o valor de `$SHELL`). `None` se já está no PATH. Nunca edita nada.
///
/// **Onde:** [`aviso_pos_instalacao`] e a suíte.
pub fn orientar_path(path: &str, dir: &Path, home: &Path, shell: &str) -> Option<String> {
    if esta_no_path(path, dir, home) {
        return None;
    }
    let nome = shell.rsplit('/').next().unwrap_or("");
    let (arquivo, comando) = match nome {
        "fish" => (None, "fish_add_path ~/.cargo/bin".to_string()),
        "bash" => (Some("~/.bashrc"), linha_export("~/.bashrc")),
        "zsh" => (Some("~/.zshrc"), linha_export("~/.zshrc")),
        _ => (Some("~/.profile"), linha_export("~/.profile")),
    };
    let onde = arquivo.map(|a| format!(" (fica salvo em {a})")).unwrap_or_default();
    Some(format!(
        "A pasta {} ainda não está no seu PATH — por isso o comando pode não ser achado, e \
         reabrir o terminal sozinho não resolve.\nPara resolver, rode esta linha uma vez{onde}:\n    {comando}",
        dir.display()
    ))
}

/// **O quê:** o comando que acrescenta `~/.cargo/bin` ao PATH no arquivo `rc` dado.
/// **Onde:** [`orientar_path`], para bash, zsh e o fallback `~/.profile`.
fn linha_export(rc: &str) -> String {
    format!("echo 'export PATH=\"$HOME/.cargo/bin:$PATH\"' >> {rc}")
}

/// **O quê:** lê PATH e SHELL do ambiente e devolve a orientação, se for o caso.
/// **Onde:** fim de `install`/`update`, que imprimem o resultado.
pub fn aviso_pos_instalacao(dir: &Path, home: &Path) -> Option<String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let shell = std::env::var("SHELL").unwrap_or_default();
    orientar_path(&path, dir, home, &shell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn home() -> PathBuf {
        PathBuf::from("/home/ana")
    }
    fn dir() -> PathBuf {
        PathBuf::from("/home/ana/.cargo/bin")
    }
    const SEM: &str = "/usr/bin:/bin";

    #[test]
    fn fish_recebe_fish_add_path() {
        let m = orientar_path(SEM, &dir(), &home(), "/usr/bin/fish").unwrap();
        assert!(m.contains("fish_add_path ~/.cargo/bin"), "{m}");
        assert!(!m.contains("reabra"), "reabrir não resolve: {m}");
    }

    #[test]
    fn bash_recebe_linha_do_bashrc() {
        let m = orientar_path(SEM, &dir(), &home(), "/bin/bash").unwrap();
        assert!(m.contains(">> ~/.bashrc") && m.contains("export PATH=\"$HOME/.cargo/bin:$PATH\""));
    }

    #[test]
    fn zsh_recebe_linha_do_zshrc() {
        let m = orientar_path(SEM, &dir(), &home(), "/usr/bin/zsh").unwrap();
        assert!(m.contains("~/.zshrc") && m.contains("export PATH=\"$HOME/.cargo/bin:$PATH\""));
    }

    #[test]
    fn shell_desconhecido_ou_vazio_cai_no_profile() {
        for s in ["", "/usr/bin/nu"] {
            let m = orientar_path(SEM, &dir(), &home(), s).unwrap();
            assert!(m.contains("~/.profile"), "{s}: {m}");
        }
    }

    #[test]
    fn ja_no_path_nao_diz_nada_em_todas_as_grafias() {
        for p in [
            "/usr/bin:/home/ana/.cargo/bin",
            "/home/ana/.cargo/bin/:/usr/bin",
            "~/.cargo/bin:/usr/bin",
            "/usr/bin:$HOME/.cargo/bin/",
        ] {
            assert_eq!(orientar_path(p, &dir(), &home(), "/usr/bin/fish"), None, "{p}");
        }
    }

    #[test]
    fn dir_parecido_nao_conta() {
        let p = "/home/ana/.cargo/bin-velho:/home/ana/.cargo";
        assert!(orientar_path(p, &dir(), &home(), "/bin/bash").is_some());
    }

    #[test]
    fn dir_com_barra_final_e_comparado_normalizado() {
        let d = PathBuf::from("/home/ana/.cargo/bin/");
        assert_eq!(orientar_path("/home/ana/.cargo/bin", &d, &home(), "/bin/bash"), None);
    }
}
