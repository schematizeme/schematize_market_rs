//! RELEASE POR APP — baixar o binário pronto de QUALQUER app da casa, em vez de compilar.
//!
//! **O quê:** descobre se o último release de um app tem os assets desta plataforma
//! (`<bin>-<os>-<arch>[.exe]`, mais o da janela quando o app tem uma) e, havendo, baixa,
//! verifica e instala os dois em `~/.cargo/bin` com a MESMA mecânica do hub
//! ([`super::binario`]: `chmod`, `--version`, rename atômico).
//!
//! **Onde:** o `install` de um app ([`crate::appsdacasa::instalar_do_release_ou_fonte`]), o `update` de um
//! componente instalado por binário ([`super::componentes`]) e o alvo/`status`
//! ([`super::alvo::consultar_publicado`]).
//!
//! ## Por que isto existe (§37.48)
//!
//! Só o hub e o market baixavam binário pronto. Todo outro app compilava do fonte: rustup, libs
//! de build, `sudo` sem terminal — para um leigo, um bug do software. O CI de cada repo já
//! publica os assets; faltava o market aprender a pedi-los.
//!
//! ## Decisões
//!
//! - **Alvo = o ÚLTIMO release (D7).** Se ele não tem o asset (do CLI, ou da janela quando o app
//!   declara uma), é "sem binário" — não se anda para releases mais velhos (isso exigiria a API
//!   do GitHub, com cota de 60/h por IP). Release com CLI mas SEM a janela conta como
//!   NÃO-casando: instalar só metade do par deixaria o hub dizendo "janela não instalada" depois
//!   de a pessoa ver "instalado" — o mesmo defeito do release verde com um asset a menos.
//! - **Tri-estado (herdado do `status`):** `NotFound`/sem release = cai no fonte; rede, cota ou
//!   resposta estranha = ERRO dito, SEM compilar (a causa não é a ausência do binário).

use super::alvo::Publicado;
use super::{binario, AppGerido};
use crate::nucleo::plataforma::{self, Os};
use crate::nucleo::rede::{self, Causa};
use std::path::{Path, PathBuf};

/// **O quê:** a rede que o install por release usa — injetável para a suíte não tocar no GitHub.
///
/// **Onde:** [`consultar`], [`baixar_e_instalar`] e [`instalar_ou_fonte`]; a produção usa
/// [`RedeReal`].
pub trait Rede {
    /// **O quê:** a URL final de `releases/latest`, com a causa tipada da falha.
    fn ultimo_release(&self, repo: &str) -> Result<String, (Causa, String)>;
    /// **O quê:** o asset existe? `Ok(false)` só para 404 confirmado.
    fn asset_existe(&self, url: &str) -> Result<bool, String>;
    /// **O quê:** baixa `url` para `dest`; `true` se o arquivo ficou gravado.
    fn baixar(&self, url: &str, dest: &Path) -> bool;
}

/// **O quê:** a [`Rede`] de verdade (curl / PowerShell). **Onde:** o `install` e o `update`.
pub struct RedeReal;

impl Rede for RedeReal {
    fn ultimo_release(&self, repo: &str) -> Result<String, (Causa, String)> {
        rede::url_do_ultimo_release_tipado(repo)
    }
    fn asset_existe(&self, url: &str) -> Result<bool, String> {
        rede::asset_existe(url)
    }
    fn baixar(&self, url: &str, dest: &Path) -> bool {
        rede::download(url, dest)
    }
}

/// **O quê:** os nomes dos assets de um app nesta plataforma.
///
/// **Onde:** entrada de [`consultar`] e [`baixar_e_instalar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assets {
    /// O asset do binário headless (obrigatório).
    pub cli: String,
    /// O asset da janela, se o app tem uma no mesmo repo.
    pub gui: Option<String>,
}

/// **O quê:** o nome do asset de release de um binário da casa para um SO/arquitetura:
/// `<bin>-<os>-<arch>[.exe]`. `None` fora da matriz do CI. Função PURA.
///
/// **Onde:** [`assets_para`] e [`asset_do_binario`].
pub fn asset_do_binario_para(bin: &str, os: Os, arch: &str) -> Option<String> {
    let (alvo, sfx) = plataforma::alvo_de_asset_para(os, arch)?;
    Some(format!("{bin}-{alvo}{sfx}"))
}

/// **O quê:** [`asset_do_binario_para`] desta máquina. **Onde:** [`assets_do_app`].
pub fn asset_do_binario(bin: &str) -> Option<String> {
    asset_do_binario_para(bin, plataforma::os(), std::env::consts::ARCH)
}

/// **O quê:** os [`Assets`] de `bin` (e da janela `gui`) para um SO/arquitetura. PURA.
/// **Onde:** [`assets_do_app`] e os testes.
pub fn assets_para(bin: &str, gui: Option<&str>, os: Os, arch: &str) -> Option<Assets> {
    let cli = asset_do_binario_para(bin, os, arch)?;
    let gui = match gui {
        Some(g) => Some(asset_do_binario_para(g, os, arch)?),
        None => None,
    };
    Some(Assets { cli, gui })
}

/// **O quê:** os [`Assets`] de um app da tabela nesta máquina. **Onde:** o install e o update.
pub fn assets_do_app(app: &AppGerido) -> Option<Assets> {
    assets_para(app.bin, app.gui, plataforma::os(), std::env::consts::ARCH)
}

/// **O quê:** o que a consulta ao GitHub concluiu sobre o último release de um repo.
///
/// **Onde:** [`consultar`]; lido por [`instalar_ou_fonte`] e por
/// [`super::alvo::consultar_publicado`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Consulta {
    /// O último release tem todos os assets desta plataforma.
    Achou { versao: String },
    /// O último release existe, mas falta algum asset (CLI ou janela).
    SemAsset { versao: String },
    /// O repo não tem release (404 ou sem tag de versão). Texto com a causa.
    SemRelease(String),
    /// Não deu para saber (rede, cota, resposta estranha). NUNCA vira build.
    Falhou(String),
}

/// **O quê:** consulta o último release de `repo` e confere os assets. Toca a [`Rede`].
///
/// **Onde:** [`instalar_ou_fonte`] e [`super::alvo::consultar_publicado`].
pub fn consultar<R: Rede>(rede: &R, repo: &str, assets: &Assets) -> Consulta {
    let url = match rede.ultimo_release(repo) {
        Ok(u) => u,
        Err((Causa::NotFound, msg)) => return Consulta::SemRelease(msg),
        Err((_, msg)) => return Consulta::Falhou(msg),
    };
    let Some(versao) = super::alvo::versao_da_url_de_release(&url) else {
        return Consulta::SemRelease(format!(
            "{repo} ainda não tem nenhum release publicado (o GitHub apontou para {url})"
        ));
    };
    let tag = format!("v{versao}");
    let nomes = std::iter::once(&assets.cli).chain(assets.gui.as_ref());
    for nome in nomes {
        match rede.asset_existe(&rede::url_de_asset(repo, &tag, nome)) {
            Ok(true) => {}
            Ok(false) => return Consulta::SemAsset { versao },
            Err(e) => return Consulta::Falhou(e),
        }
    }
    Consulta::Achou { versao }
}

/// **O quê:** [`consultar`] no formato antigo de [`super::alvo`]: `Err` para o que não é um
/// release utilizável. **Onde:** [`super::alvo::consultar_publicado`].
pub fn como_publicado(c: Consulta) -> Result<Publicado, String> {
    match c {
        Consulta::Achou { versao } => Ok(Publicado { versao, tem_asset: true }),
        Consulta::SemAsset { versao } => Ok(Publicado { versao, tem_asset: false }),
        Consulta::SemRelease(m) | Consulta::Falhou(m) => Err(m),
    }
}

/// **O quê:** para onde os binários baixados vão.
///
/// **Onde:** [`baixar_e_instalar`]; montado por [`destino_do_app`].
#[derive(Debug, Clone)]
pub struct Destino {
    /// Onde fica o binário headless.
    pub cli: PathBuf,
    /// Onde fica a janela (presente quando o app tem uma).
    pub gui: Option<PathBuf>,
    /// Diretório temporário do download (recriado a cada chamada).
    pub tmp: PathBuf,
}

/// **O quê:** o [`Destino`] real de um app da tabela. **Onde:** o install e o update.
pub fn destino_do_app(app: &AppGerido) -> Destino {
    Destino {
        cli: app.caminho(),
        gui: app.gui_nome_e_destino().map(|(_, dst)| dst),
        tmp: plataforma::state_dir().join("dl"),
    }
}

/// **O quê:** baixa os assets da `versao`, verifica o CLI (`--version`) e instala o par.
///
/// **Onde:** [`instalar_ou_fonte`] (install) e [`super::componentes`] (update).
///
/// **Reuso:** `make_executable`, `executa_e_reporta` e `place` de [`super::binario`]. A janela
/// NUNCA é executada (abriria uma tela) — vale o tamanho e o `chmod`. Qualquer falha é `Err`
/// com a causa; nada é instalado antes de o CLI passar na verificação.
pub fn baixar_e_instalar<R: Rede>(
    rede: &R,
    repo: &str,
    versao: &str,
    assets: &Assets,
    dest: &Destino,
) -> Result<(), String> {
    let (tmp, tag) = (&dest.tmp, format!("v{versao}"));
    if tmp.exists() {
        std::fs::remove_dir_all(tmp)
            .map_err(|e| format!("não consegui limpar {}: {e}", tmp.display()))?;
    }
    std::fs::create_dir_all(tmp)
        .map_err(|e| format!("não consegui criar {}: {e}", tmp.display()))?;
    let resultado = baixar_verificar_e_por(rede, repo, &tag, versao, assets, dest);
    if let Err(e) = std::fs::remove_dir_all(tmp) {
        eprintln!("aviso: não consegui apagar {}: {e}", tmp.display());
    }
    resultado?;
    println!("→ instalado do binário pré-compilado v{versao}.");
    Ok(())
}

/// **O quê:** o miolo de [`baixar_e_instalar`]: baixa, verifica o CLI e põe o par no destino.
/// **Onde:** só [`baixar_e_instalar`], que cuida do diretório temporário em volta.
fn baixar_verificar_e_por<R: Rede>(
    rede: &R,
    repo: &str,
    tag: &str,
    versao: &str,
    assets: &Assets,
    dest: &Destino,
) -> Result<(), String> {
    let baixa = |nome: &str| -> Result<PathBuf, String> {
        let alvo = dest.tmp.join(nome);
        if !rede.baixar(&rede::url_de_asset(repo, tag, nome), &alvo) {
            return Err(format!("não consegui baixar {nome} de {repo} {tag}"));
        }
        binario::make_executable(&alvo);
        Ok(alvo)
    };
    let cli_tmp = baixa(&assets.cli)?;
    if !binario::executa_e_reporta(&cli_tmp, versao) {
        return Err(format!(
            "o binário baixado ({}) não executa aqui ou não reporta a v{versao}; nada foi \
             instalado e o que já estava continua funcionando",
            assets.cli
        ));
    }
    let gui_tmp = match &assets.gui {
        Some(g) => Some(baixa(g)?),
        None => None,
    };
    binario::place(&cli_tmp, &dest.cli)?;
    match (gui_tmp, &dest.gui) {
        (Some(src), Some(dst)) => binario::place(&src, dst)
            .map_err(|e| format!("o binário está instalado, mas a janela não: {e}"))?,
        (Some(_), None) => return Err("o app tem janela mas não há destino para ela".to_string()),
        (None, _) => {}
    }
    Ok(())
}

/// **O quê:** como um app acabou instalado.
/// **Onde:** devolvido por [`instalar_ou_fonte`]; o chamador registra o método (D8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Via {
    /// Binário de release, nesta versão.
    Release(String),
    /// Compilado do fonte (o repo não tem release com os assets).
    Fonte,
}

/// **O quê:** instala por release; cai no `fonte` SÓ se o repo não tem release casando.
///
/// **Onde:** [`crate::appsdacasa::instalar_do_release_ou_fonte`].
///
/// - `Achou` -> baixa e instala; falha no download é `Err` (não compila).
/// - `SemAsset` / `SemRelease` / plataforma sem asset -> avisa e chama `fonte`.
/// - `Falhou` (rede, cota) -> `Err`, `fonte` NUNCA chamada: compilar por causa de uma rede
///   ruim instalaria rustup e pediria sudo por um problema que não é do binário.
pub fn instalar_ou_fonte<R: Rede>(
    rede: &R,
    repo: &str,
    assets: Option<&Assets>,
    dest: &Destino,
    fonte: impl FnOnce() -> Result<(), String>,
) -> Result<Via, String> {
    let Some(assets) = assets else {
        println!("→ sem binário pré-compilado para esta plataforma — compilando do fonte…");
        return fonte().map(|()| Via::Fonte);
    };
    match consultar(rede, repo, assets) {
        Consulta::Achou { versao } => {
            baixar_e_instalar(rede, repo, &versao, assets, dest)?;
            Ok(Via::Release(versao))
        }
        Consulta::SemAsset { versao } => {
            println!("→ a v{versao} de {repo} não tem binário completo para esta máquina — compilando do fonte…");
            fonte().map(|()| Via::Fonte)
        }
        Consulta::SemRelease(motivo) => {
            println!("→ {repo} não tem release com binário ({motivo}) — compilando do fonte…");
            fonte().map(|()| Via::Fonte)
        }
        Consulta::Falhou(causa) => Err(format!(
            "não consegui verificar se {repo} tem binário pronto: {causa}\n  Nada foi instalado. \
             Não vou compilar por conta própria (compilar instala pacotes do sistema e pede \
             sudo) por causa de um problema de rede: tente de novo em alguns minutos."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// Rede falsa: o que existe, o que falha e o que foi baixado.
    struct Falsa {
        ultimo: Result<String, (Causa, String)>,
        existem: Vec<String>,
        sonda_falha: Option<String>,
        baixados: RefCell<Vec<String>>,
        versao_do_cli: String,
        sondas: Cell<usize>,
    }

    impl Falsa {
        fn com_release(versao: &str, existem: &[&str]) -> Self {
            Falsa {
                ultimo: Ok(format!("https://github.com/o/r/releases/tag/v{versao}")),
                existem: existem.iter().map(|s| s.to_string()).collect(),
                sonda_falha: None,
                baixados: RefCell::new(vec![]),
                versao_do_cli: versao.to_string(),
                sondas: Cell::new(0),
            }
        }
        fn com_erro(causa: Causa, msg: &str) -> Self {
            let mut f = Self::com_release("0.0.0", &[]);
            f.ultimo = Err((causa, msg.to_string()));
            f
        }
    }

    impl Rede for Falsa {
        fn ultimo_release(&self, _: &str) -> Result<String, (Causa, String)> {
            self.ultimo.clone()
        }
        fn asset_existe(&self, url: &str) -> Result<bool, String> {
            self.sondas.set(self.sondas.get() + 1);
            if let Some(e) = &self.sonda_falha {
                return Err(e.clone());
            }
            Ok(self.existem.iter().any(|a| url.ends_with(&format!("/{a}"))))
        }
        fn baixar(&self, url: &str, dest: &Path) -> bool {
            let nome = url.rsplit('/').next().unwrap_or("").to_string();
            let existe = self.existem.contains(&nome);
            self.baixados.borrow_mut().push(nome.clone());
            if !existe {
                return false;
            }
            // O "binário": um script que responde `--version` (a janela nunca é executada).
            let corpo = format!("#!/bin/sh\necho \"x {} (abcdef123456)\"\n", self.versao_do_cli);
            std::fs::write(dest, corpo).is_ok()
        }
    }

    fn sandbox(n: &str) -> Destino {
        let base = std::env::temp_dir().join(format!("market-release-{n}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        Destino {
            cli: base.join("bin").join("schematize-overdev"),
            gui: Some(base.join("bin").join("schematize-overdev-gui")),
            tmp: base.join("dl"),
        }
    }

    fn assets() -> Assets {
        assets_para("schematize-overdev", Some("schematize-overdev-gui"), Os::Linux, "x86_64")
            .unwrap()
    }

    const REPO: &str = "schematizeme/schematize_overdev_rs";

    /// O asset de QUALQUER binário da casa segue `<bin>-<os>-<arch>[.exe]` — o que o
    /// `release.yml` de cada repo publica (overdev e a janela dele) — nas quatro plataformas;
    /// fora da matriz não há asset (cai no fonte, dito).
    #[test]
    fn asset_do_overdev_em_cada_plataforma() {
        let casos = [
            (Os::Linux, "x86_64", Some("linux-x86_64")),
            (Os::Mac, "aarch64", Some("macos-arm64")),
            (Os::Mac, "x86_64", Some("macos-x86_64")),
            (Os::Windows, "x86_64", Some("windows-x86_64.exe")),
            (Os::Linux, "aarch64", None),
        ];
        for (os, arch, sufixo) in casos {
            let a = assets_para("schematize-overdev", Some("schematize-overdev-gui"), os, arch);
            let esperado = sufixo.map(|s| Assets {
                cli: format!("schematize-overdev-{s}"),
                gui: Some(format!("schematize-overdev-gui-{s}")),
            });
            assert_eq!(a, esperado, "{os:?}/{arch}");
        }
        let sem_gui = assets_para("schematize-git", None, Os::Linux, "x86_64").unwrap();
        assert_eq!(sem_gui.gui, None);
    }

    /// Release com CLI e janela = achou; é a consulta que decide o download.
    #[test]
    fn consulta_acha_o_release_com_os_dois_assets() {
        let f = Falsa::com_release(
            "0.1.0",
            &["schematize-overdev-linux-x86_64", "schematize-overdev-gui-linux-x86_64"],
        );
        assert_eq!(consultar(&f, REPO, &assets()), Consulta::Achou { versao: "0.1.0".into() });
    }

    /// **Janela ausente num release que tem o CLI:** decisão documentada no cabeçalho — o
    /// release NÃO casa (`SemAsset`), em vez de instalar meio par.
    #[test]
    fn release_sem_a_janela_nao_casa() {
        let f = Falsa::com_release("0.1.0", &["schematize-overdev-linux-x86_64"]);
        assert_eq!(consultar(&f, REPO, &assets()), Consulta::SemAsset { versao: "0.1.0".into() });
        let g = Falsa::com_release("0.1.0", &["schematize-overdev-gui-linux-x86_64"]);
        assert_eq!(consultar(&g, REPO, &assets()), Consulta::SemAsset { versao: "0.1.0".into() });
    }

    /// Tri-estado: 404 = sem release; rede e cota = falha (nunca "sem release").
    #[test]
    fn consulta_distingue_not_found_de_rede_e_cota() {
        let nf = Falsa::com_erro(Causa::NotFound, "404");
        assert!(matches!(consultar(&nf, REPO, &assets()), Consulta::SemRelease(_)));
        for causa in [Causa::Network, Causa::RateLimited, Causa::Unexpected] {
            let f = Falsa::com_erro(causa, "causa-x");
            assert_eq!(consultar(&f, REPO, &assets()), Consulta::Falhou("causa-x".into()));
        }
        // Repo sem tag de versão: o GitHub redireciona para `/releases` (sem `tag/`).
        let mut sem_tag = Falsa::com_release("0.1.0", &[]);
        sem_tag.ultimo = Ok("https://github.com/o/r/releases".into());
        assert!(matches!(consultar(&sem_tag, REPO, &assets()), Consulta::SemRelease(_)));
        // A sonda do asset falhando por rede também é falha, não "sem asset".
        let mut sonda = Falsa::com_release("0.1.0", &[]);
        sonda.sonda_falha = Some("timeout".into());
        assert_eq!(consultar(&sonda, REPO, &assets()), Consulta::Falhou("timeout".into()));
    }

    /// O formato antigo do `alvo` continua o mesmo para hub/market.
    #[test]
    fn como_publicado_preserva_o_contrato_do_alvo() {
        let ok = como_publicado(Consulta::Achou { versao: "1.2.3".into() }).unwrap();
        assert_eq!(ok, Publicado { versao: "1.2.3".into(), tem_asset: true });
        let sem = como_publicado(Consulta::SemAsset { versao: "1.2.3".into() }).unwrap();
        assert_eq!(sem, Publicado { versao: "1.2.3".into(), tem_asset: false });
        assert_eq!(como_publicado(Consulta::SemRelease("m".into())).unwrap_err(), "m");
        assert_eq!(como_publicado(Consulta::Falhou("n".into())).unwrap_err(), "n");
    }

    /// **Release achado -> baixa os dois, instala, e NÃO compila.**
    #[cfg(unix)]
    #[test]
    fn release_achado_baixa_e_nao_compila() {
        let f = Falsa::com_release(
            "0.1.0",
            &["schematize-overdev-linux-x86_64", "schematize-overdev-gui-linux-x86_64"],
        );
        let d = sandbox("achou");
        let compilou = Cell::new(false);
        let via = instalar_ou_fonte(&f, REPO, Some(&assets()), &d, || {
            compilou.set(true);
            Ok(())
        })
        .unwrap();
        assert_eq!(via, Via::Release("0.1.0".into()));
        assert!(!compilou.get(), "com release não se compila");
        assert!(d.cli.is_file() && d.gui.as_ref().unwrap().is_file(), "os dois no destino");
        assert_eq!(
            f.baixados.borrow().as_slice(),
            ["schematize-overdev-linux-x86_64", "schematize-overdev-gui-linux-x86_64"]
        );
        assert!(binario::executa_e_reporta(&d.cli, "0.1.0"), "o instalado reporta a versão");
        assert!(!d.tmp.exists(), "o temporário do download não fica");
    }

    /// **Sem release (not_found) -> cai no fonte**, e só então.
    #[test]
    fn sem_release_cai_no_fonte() {
        let f = Falsa::com_erro(Causa::NotFound, "404");
        let d = sandbox("nf");
        let compilou = Cell::new(false);
        let via = instalar_ou_fonte(&f, REPO, Some(&assets()), &d, || {
            compilou.set(true);
            Ok(())
        })
        .unwrap();
        assert_eq!(via, Via::Fonte);
        assert!(compilou.get());
        assert!(f.baixados.borrow().is_empty(), "nada baixado");
    }

    /// Release sem a janela -> cai no fonte (não instala meio par).
    #[test]
    fn release_sem_janela_cai_no_fonte_sem_baixar() {
        let f = Falsa::com_release("0.1.0", &["schematize-overdev-linux-x86_64"]);
        let d = sandbox("semgui");
        let compilou = Cell::new(false);
        let via = instalar_ou_fonte(&f, REPO, Some(&assets()), &d, || {
            compilou.set(true);
            Ok(())
        })
        .unwrap();
        assert_eq!(via, Via::Fonte);
        assert!(compilou.get() && f.baixados.borrow().is_empty() && !d.cli.exists());
    }

    /// Plataforma fora da matriz: sem asset possível -> fonte.
    #[test]
    fn plataforma_sem_asset_vai_direto_ao_fonte() {
        let f = Falsa::com_release("0.1.0", &[]);
        let d = sandbox("semplat");
        let via = instalar_ou_fonte(&f, REPO, None, &d, || Ok(())).unwrap();
        assert_eq!(via, Via::Fonte);
        assert_eq!(f.sondas.get(), 0, "nem sonda a rede");
    }

    /// **Erro de rede/cota -> Err e NENHUM build.** Compilar por causa de uma rede ruim
    /// instalaria rustup e pediria sudo por um problema que não é do binário.
    #[test]
    fn erro_de_rede_ou_cota_e_err_e_nao_compila() {
        for causa in [Causa::Network, Causa::RateLimited, Causa::Unexpected] {
            let f = Falsa::com_erro(causa, "sem-rede-x");
            let d = sandbox("net");
            let compilou = Cell::new(false);
            let r = instalar_ou_fonte(&f, REPO, Some(&assets()), &d, || {
                compilou.set(true);
                Ok(())
            });
            let e = r.unwrap_err();
            assert!(e.contains("sem-rede-x"), "a causa tem de aparecer: {e}");
            assert!(!compilou.get(), "{causa:?}: não pode compilar");
            assert!(!d.cli.exists(), "nada instalado");
        }
    }

    /// Um binário baixado que não reporta a versão do release NÃO é instalado, e não vira build.
    #[cfg(unix)]
    #[test]
    fn binario_que_nao_reporta_a_versao_nao_e_instalado() {
        let mut f = Falsa::com_release(
            "0.1.0",
            &["schematize-overdev-linux-x86_64", "schematize-overdev-gui-linux-x86_64"],
        );
        f.versao_do_cli = "9.9.9".into();
        let d = sandbox("ruim");
        let compilou = Cell::new(false);
        let e = instalar_ou_fonte(&f, REPO, Some(&assets()), &d, || {
            compilou.set(true);
            Ok(())
        })
        .unwrap_err();
        assert!(e.contains("0.1.0"), "{e}");
        assert!(!compilou.get() && !d.cli.exists() && !d.gui.as_ref().unwrap().exists());
    }

    /// Falha do download depois de a sonda dizer que existe é erro (não build).
    #[test]
    fn download_que_falha_e_err() {
        let mut f = Falsa::com_release(
            "0.1.0",
            &["schematize-overdev-linux-x86_64", "schematize-overdev-gui-linux-x86_64"],
        );
        // A sonda diz que existe, o download não entrega.
        f.existem.push("fantasma".into());
        let d = sandbox("dl");
        let mut a = assets();
        a.cli = "fantasma-x".into();
        f.existem.push(a.cli.clone());
        let r = baixar_e_instalar(&FalsaSemDownload(f), REPO, "0.1.0", &a, &d);
        assert!(r.unwrap_err().contains("baixar"));
    }

    /// Envolve a [`Falsa`] negando todo download.
    struct FalsaSemDownload(Falsa);
    impl Rede for FalsaSemDownload {
        fn ultimo_release(&self, r: &str) -> Result<String, (Causa, String)> {
            self.0.ultimo_release(r)
        }
        fn asset_existe(&self, u: &str) -> Result<bool, String> {
            self.0.asset_existe(u)
        }
        fn baixar(&self, _: &str, _: &Path) -> bool {
            false
        }
    }
}
