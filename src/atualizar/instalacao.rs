//! INSTALAÇÃO CANÔNICA (D12) — o `install` olha a pasta de instalação do market, não o PATH.
//!
//! **O quê:** decide e executa `schematize-market install <app>` contra a cópia CANÔNICA
//! (`~/.cargo/bin/<bin>`, [`AppGerido::caminho`]): se ela não existe, é mais velha que o
//! release ou é de outro commit com a mesma versão ([`super::divergencia`]), instala do
//! release; se já é igual ao release, diz que está instalado. Nos dois casos roda a limpeza
//! compulsória (D11, [`super::limpeza`]) — cópias duplicadas em outros diretórios saem.
//!
//! **Onde:** [`crate::appsdacasa::instalar_app_da_casa`], chamado por `cli/instalar.rs`.
//!
//! ## O defeito que isto conserta
//!
//! Com um `schematize-overdev` 0.0.5 velho em `~/.local/bin` e nenhum em `~/.cargo/bin`, o
//! `install` achava a cópia pelo PATH (`descobrir_app`), respondia "já está instalado" e saía:
//! sem instalar, sem limpar. A pessoa seguia com a versão velha e o PATH escolhendo-a.
//!
//! ## Tudo injetável
//!
//! A rede ([`Rede`], [`Commits`]), o destino ([`Destino`]) e a limpeza ([`Ambiente`]/[`Alvo`])
//! entram por parâmetro: a suíte roda em diretórios temporários, nunca no `HOME` real.

use super::alvo::Metodo;
use super::divergencia::{self, Commits, Decisao};
use super::limpeza::{self, Alvo, Ambiente};
use super::release::{self, Assets, Consulta, Destino, Rede, Via};
use super::versao;
use crate::ctx::Ctx;
use crate::nucleo::util;
use std::path::PathBuf;

/// **O quê:** tudo que o `install` precisa saber de UM app. **Onde:** [`instalar_com`].
pub struct Entrada<'a> {
    /// `owner/repo` do app.
    pub repo: &'a str,
    /// Nome do binário (só para as mensagens).
    pub bin: &'a str,
    /// Os assets desta plataforma; `None` = sem binário pré-compilado aqui.
    pub assets: Option<&'a Assets>,
    /// Onde o app é instalado (a cópia canônica é `dest.cli`).
    pub dest: &'a Destino,
    /// Como o app foi instalado (D8). `Fonte` = build deliberado, nunca trocado.
    pub metodo: Metodo,
    /// Uma cópia achada FORA da pasta canônica (PATH), só para explicar na mensagem.
    pub fora: Option<PathBuf>,
}

/// **O quê:** o que a avaliação concluiu sobre a cópia canônica. **Onde:** [`avaliar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Avaliacao {
    /// A cópia canônica já é o que se queria (igual ao release, à frente dele, ou do fonte).
    JaInstalado { versao: String, nota: String },
    /// Falta instalar (ou trocar). O texto diz por quê.
    Instalar(String),
    /// Não deu para confirmar contra o release: nada é alterado.
    NaoConfirmado { versao: String, msg: String },
}

/// **O quê:** o desfecho de [`instalar_com`]. **Onde:** `cli/instalar.rs`.
#[derive(Debug, PartialEq, Eq)]
pub enum Resultado {
    /// Já estava instalado e igual; a limpeza rodou.
    JaInstalado { versao: String },
    /// Instalado agora, por esta via.
    Instalado(Via),
    /// Não consegui confirmar o commit do release; nada foi alterado.
    NaoConfirmado { versao: String },
    /// `--dry-run`: nada foi tocado.
    SoSimulou,
    /// A pessoa não confirmou.
    Cancelado,
}

/// **O quê:** compara a cópia canônica com o release. Toca a rede; NÃO altera o disco.
///
/// **Onde:** [`instalar_com`].
pub fn avaliar<R: Rede, C: Commits>(rede: &R, commits: &C, e: &Entrada) -> Avaliacao {
    let canon = &e.dest.cli;
    if !canon.is_file() {
        let onde = match &e.fora {
            Some(f) => format!("só há uma cópia fora da pasta do market: {}", f.display()),
            None => "não há cópia instalada".to_string(),
        };
        return Avaliacao::Instalar(format!("{onde}; instalo em {}", canon.display()));
    }
    // A janela faz parte do app: CLI em dia sem ela NÃO é "instalado" (kit completo, §37.48).
    if let Some(g) = e.dest.gui.as_ref().filter(|_| e.assets.is_some_and(|a| a.gui.is_some())) {
        if !g.is_file() {
            return Avaliacao::Instalar(format!(
                "a janela não está instalada ({} não existe)",
                g.display()
            ));
        }
    }
    let local = match versao::consultar_versao(canon) {
        Ok(v) => v,
        Err(causa) => {
            return Avaliacao::Instalar(format!(
                "{} não responde `--version` ({causa})",
                canon.display()
            ));
        }
    };
    if e.metodo == Metodo::Fonte {
        return Avaliacao::JaInstalado {
            versao: local,
            nota: "compilado do fonte (mantido)".into(),
        };
    }
    let Some(assets) = e.assets else {
        return Avaliacao::JaInstalado {
            versao: local,
            nota: "sem binário de release aqui".into(),
        };
    };
    let release_v = match release::consultar(rede, e.repo, assets) {
        Consulta::Achou { versao } => versao,
        Consulta::SemAsset { .. } | Consulta::SemRelease(_) => {
            return Avaliacao::JaInstalado {
                versao: local,
                nota: "sem release para comparar".into(),
            };
        }
        Consulta::Falhou(msg) => return Avaliacao::NaoConfirmado { versao: local, msg },
    };
    if util::semver_lt(&local, &release_v) {
        return Avaliacao::Instalar(format!(
            "a cópia instalada (v{local}) é mais antiga que o release v{release_v}"
        ));
    }
    if util::semver_lt(&release_v, &local) {
        return Avaliacao::JaInstalado {
            versao: local,
            nota: format!("à frente do release v{release_v} (mantido)"),
        };
    }
    if local != release_v {
        return Avaliacao::Instalar(format!("v{local} difere do release v{release_v}"));
    }
    let prov = divergencia::proveniencia_do_binario(canon);
    let tag = format!("v{release_v}");
    match divergencia::decidir(e.metodo, &prov, || {
        divergencia::commit_do_release(commits, e.repo, &tag)
    }) {
        Decisao::EmDia | Decisao::ManterFonte => {
            Avaliacao::JaInstalado { versao: local, nota: "igual ao release (mesmo commit)".into() }
        }
        Decisao::Reinstalar(m) => Avaliacao::Instalar(m.texto()),
        Decisao::NaoConfirmei { msg, .. } => Avaliacao::NaoConfirmado { versao: local, msg },
    }
}

/// **O quê:** o `install` inteiro: avalia, confirma, instala do release (ou do fonte, só se
/// não há release) e limpa as cópias antigas. Devolve o desfecho.
///
/// **Onde:** [`crate::appsdacasa::instalar_app_da_casa`] e a suíte.
///
/// `confirmar` só é chamado quando há o que instalar; recebe o motivo. `fonte` só roda se o
/// repo não tem release com binário (ver [`release::instalar_ou_fonte`]).
#[allow(clippy::too_many_arguments)]
pub fn instalar_com<R: Rede, C: Commits>(
    ctx: &Ctx,
    rede: &R,
    commits: &C,
    e: &Entrada,
    amb: &Ambiente<'_>,
    alvo_limpeza: &dyn Fn(bool) -> Alvo,
    dry_run: bool,
    confirmar: impl FnOnce(&str) -> bool,
    fonte: impl FnOnce() -> Result<(), String>,
) -> Result<Resultado, String> {
    let limpar = |de_binario: bool| limpeza::limpar_em(amb, &alvo_limpeza(de_binario)).imprimir();
    let motivo = match avaliar(rede, commits, e) {
        Avaliacao::JaInstalado { versao, nota } => {
            println!("{} {versao} já está instalado em {} — {nota}.", e.bin, e.dest.cli.display());
            if dry_run {
                println!("(dry-run) removeria as cópias antigas, se houver.");
                return Ok(Resultado::SoSimulou);
            }
            limpar(e.metodo == Metodo::Binario);
            return Ok(Resultado::JaInstalado { versao });
        }
        Avaliacao::NaoConfirmado { versao, msg } => {
            println!("{}", divergencia::mensagem_nao_confirmei(e.bin, &versao, &msg));
            return Ok(Resultado::NaoConfirmado { versao });
        }
        Avaliacao::Instalar(motivo) => motivo,
    };
    println!("→ {}: {motivo}.", e.bin);
    if dry_run {
        println!("(dry-run) instalaria o `{}` do release (do fonte só se não houver).", e.bin);
        return Ok(Resultado::SoSimulou);
    }
    if !confirmar(&motivo) {
        return Ok(Resultado::Cancelado);
    }
    let via = release::instalar_ou_fonte(ctx, rede, e.repo, e.assets, e.dest, fonte)?;
    // D11: instalação concluída = cópias antigas saem (o `?` acima impede isto numa falha).
    limpar(matches!(via, Via::Release(_)));
    Ok(Resultado::Instalado(via))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atualizar::limpeza::Item;
    use crate::nucleo::rede::Causa;
    use std::cell::{Cell, RefCell};
    use std::path::Path;

    const BIN: &str = "schematize-overdev";
    const REPO: &str = "schematizeme/schematize_overdev_rs";
    const SHA: &str = "a36d638d233f0000111122223333444455556666";

    /// Rede falsa de release: o último é `versao`, com os assets; registra os downloads.
    struct Falsa {
        ultimo: Result<String, (Causa, String)>,
        baixados: RefCell<Vec<String>>,
        consultas: Cell<usize>,
    }
    impl Falsa {
        fn release(v: &str) -> Self {
            Falsa {
                ultimo: Ok(format!("https://github.com/o/r/releases/tag/v{v}")),
                baixados: RefCell::new(vec![]),
                consultas: Cell::new(0),
            }
        }
    }
    impl Rede for Falsa {
        fn ultimo_release(&self, _: &str) -> Result<String, (Causa, String)> {
            self.consultas.set(self.consultas.get() + 1);
            self.ultimo.clone()
        }
        fn asset_existe(&self, _: &str) -> Result<bool, String> {
            Ok(true)
        }
        fn baixar(&self, url: &str, dest: &Path) -> bool {
            self.baixados.borrow_mut().push(url.rsplit('/').next().unwrap_or("").to_string());
            let corpo = format!("#!/bin/sh\necho \"{BIN} 0.1.0 ({})\"\n", &SHA[..12]);
            std::fs::write(dest, corpo).is_ok()
        }
    }

    /// Commits falsos: o `ls-remote` devolve a tag anotada do release (ou falha).
    struct CommitsFalsos(Result<String, (Causa, String)>);
    impl Commits for CommitsFalsos {
        fn git_no_path(&self) -> bool {
            true
        }
        fn ls_remote(&self, _: &str, _: &[String]) -> Result<String, (Causa, String)> {
            self.0.clone()
        }
        fn api_commit(&self, _: &str, _: &str) -> Result<String, (Causa, String)> {
            panic!("a API não pode ser chamada com git presente")
        }
    }
    fn commits_ok() -> CommitsFalsos {
        CommitsFalsos(Ok(format!("{SHA}\trefs/tags/v0.1.0^{{}}\n")))
    }

    fn script(p: &Path, versao: &str, sha: Option<&str>) {
        use std::os::unix::fs::PermissionsExt;
        let linha = match sha {
            Some(s) => format!("{BIN} {versao} ({s})"),
            None => format!("{BIN} {versao}"),
        };
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, format!("#!/bin/sh\necho \"{linha}\"\n")).unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn nenhum_dono(_: &Path) -> Option<String> {
        None
    }

    /// Um mundo de sandbox: `.cargo/bin` (canônico), `.local/bin` (cópia velha) e o ambiente
    /// da limpeza apontando SÓ para eles.
    struct Mundo {
        base: PathBuf,
        canon: PathBuf,
        velha: PathBuf,
        dest: Destino,
        amb: Ambiente<'static>,
    }
    fn mundo(nome: &str) -> Mundo {
        let base = std::env::temp_dir().join(format!("market-inst-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (cargo, local) = (base.join(".cargo/bin"), base.join(".local/bin"));
        std::fs::create_dir_all(&local).unwrap();
        let amb = Ambiente {
            path_dirs: vec![local.clone(), cargo.clone()],
            dirs_usuario: vec![cargo.clone(), local.clone()],
            dirs_padrao: vec![],
            dirs_sistema: vec![],
            state_dir: base.join("estado"),
            protegidos: vec![],
            remover: Box::new(limpeza::remover_real),
            dono_do_pacote: nenhum_dono,
        };
        let dest = Destino { cli: cargo.join(BIN), gui: None, tmp: base.join("dl") };
        Mundo { canon: cargo.join(BIN), velha: local.join(BIN), base, dest, amb }
    }
    fn assets() -> Assets {
        Assets { cli: format!("{BIN}-linux-x86_64"), gui: None }
    }
    fn alvo_de(canon: &Path) -> Alvo {
        Alvo {
            itens: vec![Item {
                nome: BIN.into(),
                frescos: vec![canon.to_path_buf()],
                generico: false,
            }],
            ancora: vec![canon.to_path_buf()],
            repos_de_build: vec![],
            limpar_cache: false,
        }
    }
    fn rodar(
        m: &Mundo,
        rede: &Falsa,
        commits: &CommitsFalsos,
        metodo: Metodo,
        dry: bool,
    ) -> Result<Resultado, String> {
        let a = assets();
        let e = Entrada {
            repo: REPO,
            bin: BIN,
            assets: Some(&a),
            dest: &m.dest,
            metodo,
            fora: Some(m.velha.clone()),
        };
        let canon = m.canon.clone();
        instalar_com(
            &Ctx::de_teste(),
            rede,
            commits,
            &e,
            &m.amb,
            &move |_| alvo_de(&canon),
            dry,
            |_| true,
            || panic!("não pode compilar do fonte: há release"),
        )
    }

    /// **PROBLEMA 2:** só há uma cópia VELHA fora da pasta canônica. O install instala o
    /// release na pasta canônica e a limpeza remove a velha.
    #[cfg(unix)]
    #[test]
    fn so_copia_velha_fora_instala_no_canonico_e_remove_a_velha() {
        let m = mundo("so-velha");
        script(&m.velha, "0.0.5", None);
        let f = Falsa::release("0.1.0");
        let r = rodar(&m, &f, &commits_ok(), Metodo::Binario, false).unwrap();
        assert_eq!(r, Resultado::Instalado(Via::Release("0.1.0".into())));
        assert!(m.canon.is_file(), "instalou na pasta canônica");
        assert!(!m.velha.exists(), "a cópia velha tinha de sair");
        assert_eq!(f.baixados.borrow().len(), 1);
        assert_eq!(versao::installed_version_of(&m.canon).as_deref(), Some("0.1.0"));
        let _ = std::fs::remove_dir_all(&m.base);
    }

    /// A canônica JÁ é o release (mesma versão, mesmo commit): diz que está instalado, NÃO
    /// baixa nada — e a duplicata velha é limpa mesmo assim.
    #[cfg(unix)]
    #[test]
    fn canonica_igual_ao_release_diz_instalado_e_limpa_a_duplicata() {
        let m = mundo("igual");
        script(&m.canon, "0.1.0", Some(&SHA[..12]));
        script(&m.velha, "0.0.5", None);
        let f = Falsa::release("0.1.0");
        let r = rodar(&m, &f, &commits_ok(), Metodo::Binario, false).unwrap();
        assert_eq!(r, Resultado::JaInstalado { versao: "0.1.0".into() });
        assert!(f.baixados.borrow().is_empty(), "não reinstala o que já é o release");
        assert!(m.canon.is_file());
        assert!(!m.velha.exists(), "a duplicata velha tinha de sair");
        let _ = std::fs::remove_dir_all(&m.base);
    }

    /// Canônica com a MESMA versão e OUTRO commit (build de dev): reinstala do release.
    #[cfg(unix)]
    #[test]
    fn canonica_com_outro_commit_reinstala_do_release() {
        let m = mundo("outro-commit");
        script(&m.canon, "0.1.0", Some("000000000000"));
        let f = Falsa::release("0.1.0");
        let r = rodar(&m, &f, &commits_ok(), Metodo::Binario, false).unwrap();
        assert_eq!(r, Resultado::Instalado(Via::Release("0.1.0".into())));
        assert_eq!(proveniencia(&m.canon), Some(SHA[..12].to_string()), "agora é o release");
        let _ = std::fs::remove_dir_all(&m.base);
    }

    /// **Idempotência com janela (v0.4.6):** CLI já no release, mas a janela NÃO está em
    /// `~/.cargo/bin` -> instala (só o que falta é uma razão para baixar); com a janela
    /// presente a segunda rodada não baixa nada.
    #[cfg(unix)]
    #[test]
    fn cli_em_dia_sem_a_janela_instala_e_na_segunda_rodada_nao_baixa() {
        let mut m = mundo("sem-janela");
        let gui = m.canon.with_file_name("schematize-overdev-gui");
        m.dest.gui = Some(gui.clone());
        script(&m.canon, "0.1.0", Some(&SHA[..12]));
        let a = Assets {
            cli: format!("{BIN}-linux-x86_64"),
            gui: Some(format!("{BIN}-gui-linux-x86_64")),
        };
        let e = Entrada {
            repo: REPO,
            bin: BIN,
            assets: Some(&a),
            dest: &m.dest,
            metodo: Metodo::Binario,
            fora: None,
        };
        let canon = m.canon.clone();
        let rodar = |f: &Falsa| {
            instalar_com(
                &Ctx::de_teste(),
                f,
                &commits_ok(),
                &e,
                &m.amb,
                &|_| alvo_de(&canon),
                false,
                |_| true,
                || panic!("sem fonte"),
            )
            .unwrap()
        };
        let f1 = Falsa::release("0.1.0");
        assert_eq!(rodar(&f1), Resultado::Instalado(Via::Release("0.1.0".into())));
        assert!(gui.is_file(), "a janela tinha de ser instalada");
        let f2 = Falsa::release("0.1.0");
        assert_eq!(rodar(&f2), Resultado::JaInstalado { versao: "0.1.0".into() });
        assert!(f2.baixados.borrow().is_empty(), "2ª rodada não reinstala");
        let _ = std::fs::remove_dir_all(&m.base);
    }

    fn proveniencia(p: &Path) -> Option<String> {
        divergencia::proveniencia_do_binario(p).sha
    }

    /// Canônica MAIS VELHA que o release: reinstala.
    #[cfg(unix)]
    #[test]
    fn canonica_mais_velha_que_o_release_reinstala() {
        let m = mundo("mais-velha");
        script(&m.canon, "0.0.9", Some(&SHA[..12]));
        let f = Falsa::release("0.1.0");
        let r = rodar(&m, &f, &commits_ok(), Metodo::Binario, false).unwrap();
        assert_eq!(r, Resultado::Instalado(Via::Release("0.1.0".into())));
        assert_eq!(versao::installed_version_of(&m.canon).as_deref(), Some("0.1.0"));
        let _ = std::fs::remove_dir_all(&m.base);
    }

    /// Método `fonte`: a canônica é mantida SEM tocar na rede, mesmo com outro commit; a
    /// duplicata velha ainda sai.
    #[cfg(unix)]
    #[test]
    fn metodo_fonte_mantem_a_canonica_sem_rede() {
        let m = mundo("fonte");
        script(&m.canon, "0.1.0", Some("000000000000"));
        script(&m.velha, "0.0.5", None);
        let f = Falsa::release("0.1.0");
        let r = rodar(&m, &f, &commits_ok(), Metodo::Fonte, false).unwrap();
        assert_eq!(r, Resultado::JaInstalado { versao: "0.1.0".into() });
        assert_eq!(f.consultas.get(), 0, "o fonte não gasta rede");
        assert_eq!(proveniencia(&m.canon), Some("000000000000".into()), "o dev build ficou");
        assert!(!m.velha.exists());
        let _ = std::fs::remove_dir_all(&m.base);
    }

    /// Falha ao confirmar o release (rede): NÃO reinstala, NÃO limpa, e diz — o desfecho é
    /// `NaoConfirmado`, nunca um "pronto" mudo.
    #[cfg(unix)]
    #[test]
    fn rede_fora_mantem_o_binario_e_reporta_sem_limpar() {
        let m = mundo("rede");
        script(&m.canon, "0.1.0", Some("000000000000"));
        script(&m.velha, "0.0.5", None);
        let mut f = Falsa::release("0.1.0");
        f.ultimo = Err((Causa::Network, "sem rede".into()));
        let r = rodar(&m, &f, &commits_ok(), Metodo::Binario, false).unwrap();
        assert_eq!(r, Resultado::NaoConfirmado { versao: "0.1.0".into() });
        assert!(f.baixados.borrow().is_empty());
        assert!(m.velha.exists(), "sem confirmar, nada é apagado");
        assert_eq!(proveniencia(&m.canon), Some("000000000000".into()));
        let _ = std::fs::remove_dir_all(&m.base);
    }

    /// Mesmo com o release achado, o `ls-remote` falhando = `NaoConfirmado` (sem reinstalar).
    #[cfg(unix)]
    #[test]
    fn ls_remote_falhando_mantem_e_reporta() {
        let m = mundo("lsfalha");
        script(&m.canon, "0.1.0", Some("000000000000"));
        let f = Falsa::release("0.1.0");
        let c = CommitsFalsos(Err((Causa::Network, "git: sem rede".into())));
        let r = rodar(&m, &f, &c, Metodo::Binario, false).unwrap();
        assert_eq!(r, Resultado::NaoConfirmado { versao: "0.1.0".into() });
        assert!(f.baixados.borrow().is_empty());
        let _ = std::fs::remove_dir_all(&m.base);
    }

    /// `--dry-run` avalia e nada mais: nenhum download, nenhuma remoção.
    #[cfg(unix)]
    #[test]
    fn dry_run_nao_instala_nem_remove() {
        let m = mundo("dry");
        script(&m.velha, "0.0.5", None);
        let f = Falsa::release("0.1.0");
        let r = rodar(&m, &f, &commits_ok(), Metodo::Binario, true).unwrap();
        assert_eq!(r, Resultado::SoSimulou);
        assert!(!m.canon.exists() && m.velha.exists());
        assert!(f.baixados.borrow().is_empty());
        let _ = std::fs::remove_dir_all(&m.base);
    }

    /// Sem confirmação da pessoa, nada acontece.
    #[cfg(unix)]
    #[test]
    fn sem_confirmacao_nada_acontece() {
        let m = mundo("cancela");
        script(&m.velha, "0.0.5", None);
        let f = Falsa::release("0.1.0");
        let a = assets();
        let e = Entrada {
            repo: REPO,
            bin: BIN,
            assets: Some(&a),
            dest: &m.dest,
            metodo: Metodo::Binario,
            fora: None,
        };
        let canon = m.canon.clone();
        let r = instalar_com(
            &Ctx::de_teste(),
            &f,
            &commits_ok(),
            &e,
            &m.amb,
            &move |_| alvo_de(&canon),
            false,
            |_| false,
            || Ok(()),
        )
        .unwrap();
        assert_eq!(r, Resultado::Cancelado);
        assert!(!m.canon.exists() && m.velha.exists());
        let _ = std::fs::remove_dir_all(&m.base);
    }
}
