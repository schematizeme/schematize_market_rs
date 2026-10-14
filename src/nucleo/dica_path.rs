//! Dica de PATH — diz, sem culpa, como pôr a pasta de instalação no PATH.
//!
//! **O quê:** renderiza, no idioma da pessoa (20 línguas, catálogo i18n do market), a
//! [`Orientacao`] estruturada que a costura `plat.path` devolve: qual é a ÚNICA linha que
//! resolve no shell dela. A DECISÃO ("está no PATH?", "qual shell?", "qual linha?") é da
//! costura (`schematize_plataforma::seams::PathSetup`); aqui só mora o TEXTO.
//!
//! **Onde:** [`aviso_pos_instalacao`], chamado ao fim de `install`/`update`
//! (`atualizar::componentes::finish` e `cli::instalar::instalar_app`).
//!
//! **Por que existe (anti-padrão 48):** num CachyOS recém-instalado (rustup do pacman, shell
//! fish) a pasta nunca entrou no PATH e o aviso "reabra o terminal" não resolvia nada. Isto
//! não edita rc de shell: o fish não tem rc que o market conheça, então dizemos o comando.

use crate::ctx::Ctx;
use crate::nucleo::i18n::tf;
use schematize_plataforma::seams::pathsetup::{Orientacao, TipoShell};
use std::path::Path;

/// **O quê:** o texto da orientação para `dir`, no idioma ativo: a linha exata para o shell
/// detectado e, quando há `rc`, onde ela fica salva (`~/.bashrc`, `~/.zshrc`, `~/.profile`).
/// O fish não tem `rc` que o market conheça: só o comando. PURA (lê só o catálogo i18n).
///
/// **Onde:** [`aviso_pos_instalacao`] e a suíte.
pub fn renderizar(o: &Orientacao, dir: &Path) -> String {
    let arquivo = match o.shell {
        TipoShell::Fish => None,
        TipoShell::Bash => Some("~/.bashrc"),
        TipoShell::Zsh => Some("~/.zshrc"),
        TipoShell::Outro => Some("~/.profile"),
    };
    let salvo =
        arquivo.map(|a| format!(" {}", tf("path.saved_in", &[("file", a)]))).unwrap_or_default();
    tf(
        "path.hint",
        &[("dir", &dir.display().to_string()), ("saved", &salvo), ("command", &o.linha)],
    )
}

/// **O quê:** pergunta à costura se `dir` já está no PATH e, se não está, devolve a orientação
/// renderizada. `None` se já está. Nunca edita nada.
/// **Onde:** fim de `install`/`update`, que imprimem o resultado.
pub fn aviso_pos_instalacao(ctx: &Ctx, dir: &Path) -> Option<String> {
    ctx.plat.path.orientacao(dir).map(|o| renderizar(&o, dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use schematize_plataforma::seams::pathsetup::orientacao_em;
    use std::path::PathBuf;

    /// A orientação renderizada (a costura decide; este módulo escreve o texto).
    fn orientar_path(path: &str, dir: &Path, home: &Path, shell: &str) -> Option<String> {
        orientacao_em(path, dir, home, shell).map(|o| renderizar(&o, dir))
    }

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
