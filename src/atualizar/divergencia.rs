//! DIVERGÊNCIA (D12) — mesma versão, build diferente: reinstala do release.
//!
//! **O quê:** compara o COMMIT do binário instalado (o `sha12` do `--version`,
//! `name x.y.z (sha12[-sujo])`) com o commit da tag do release, quando a VERSÃO instalada já é
//! a do alvo. Se diferem, o binário é um build de desenvolvimento que só se parece com o
//! release — e é trocado pelo release (não é rebaixamento).
//!
//! **Onde:** [`super::componentes`] (o `update` de cada app e o self-update do market),
//! [`super::install_or_update`] (o hub), [`super::instalacao`] (o `install`) e o `status --json`.
//!
//! ## Por que isto existe (D12)
//!
//! O `update` comparava SÓ a versão. Quatro apps desta máquina (deployer 0.9.0, database,
//! git, skills 0.1.0) eram builds de desenvolvimento com o MESMO número do release publicado,
//! de outro commit: "já está na versão-alvo", para sempre, e nada os trocava.
//!
//! ## As regras
//!
//! - **Exceção 1 — método `fonte` (D8):** build deliberado de quem compila; nunca é tocado.
//! - **Exceção 2 — sem proveniência** (binário antigo que só imprime a versão, ou `-sujo`):
//!   conta como divergente e reinstala, salvo `fonte`.
//! - **Falha ao descobrir o commit do release** (rede, cota, tag ausente): NÃO reinstala e NÃO
//!   é engolida — vira [`Decisao::NaoConfirmei`], que o chamador DIZ ("não consegui confirmar o
//!   commit do release") mantendo o binário atual. É o mesmo tri-estado de [`super::release`].
//!
//! ## Como o commit do release é obtido sem queimar a cota de 60/h da API
//!
//! `git ls-remote https://github.com/<repo>.git refs/tags/<tag>^{}` (peeled: o commit de uma
//! tag anotada), com queda para `refs/tags/<tag>` (tag leve). Só sem `git` no PATH se usa a
//! API. Com `git` presente e falhando, o erro sobe: não se troca de via para mascará-lo.

use super::alvo::Metodo;
use super::metodo::mesmo_commit;
use super::AppGerido;
use crate::ctx::Ctx;
use crate::nucleo::plataforma;
use crate::nucleo::rede::Causa;
use crate::nucleo::{bin, rede, util};
use std::path::Path;
use std::process::{Command, Stdio};

/// **O quê:** o que o `--version` de um binário diz sobre o commit de onde ele saiu.
///
/// **Onde:** [`decidir`], [`estado_build`] e o `install`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Proveniencia {
    /// O `sha12` (≥ 7 hex), ou `None` se o binário não informa (versão antiga, "fonte sem git").
    pub sha: Option<String>,
    /// Foi compilado de árvore com mudança não commitada (`-sujo`): o SHA sozinho mentiria.
    pub sujo: bool,
}

/// **O quê:** extrai a [`Proveniencia`] de uma linha de `--version`. Função PURA.
///
/// **Onde:** [`proveniencia_do_binario`] e os testes.
///
/// `schematize-git 0.1.0 (3daf499aabbc)` -> sha; `... (3daf499aabbc-sujo)` -> sha + sujo;
/// `... (fonte sem git)` ou só `x 0.1.0` -> sem sha.
pub fn proveniencia_da_linha(linha: &str) -> Proveniencia {
    let Some(dentro) = linha.rsplit_once('(').and_then(|(_, r)| r.split_once(')')).map(|(d, _)| d)
    else {
        return Proveniencia::default();
    };
    let dentro = dentro.trim();
    let (hex, sujo) = match dentro.strip_suffix("-sujo") {
        Some(h) => (h, true),
        None => (dentro, false),
    };
    let valido = (7..=40).contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit());
    if valido {
        Proveniencia { sha: Some(hex.to_string()), sujo }
    } else {
        Proveniencia::default()
    }
}

/// **O quê:** a [`Proveniencia`] de um binário instalado (`<bin> --version`). Roda o binário.
/// Não responde = sem proveniência. **Onde:** [`proveniencia_do_app`] e o `install`.
pub fn proveniencia_do_binario(caminho: &Path) -> Proveniencia {
    caminho
        .to_str()
        .and_then(|c| util::capture(c, &["--version"]))
        .map(|l| proveniencia_da_linha(&l))
        .unwrap_or_default()
}

/// **O quê:** a [`Proveniencia`] do app instalado. O market é o binário em execução: vale o
/// SHA compilado nele (a mesma razão de `AppGerido::versao_instalada`). Os demais, o
/// `--version` do arquivo (nome novo, senão o legado). **Onde:** o `update` e o `status`.
pub fn proveniencia_do_app(app: &AppGerido) -> Proveniencia {
    if app.politica == super::Politica::Sempre {
        let linha = crate::nucleo::procedencia::rotulo_versao();
        return proveniencia_da_linha(linha);
    }
    let novo = app.caminho();
    if novo.is_file() {
        return proveniencia_do_binario(&novo);
    }
    app.caminho_legado().map(|p| proveniencia_do_binario(&p)).unwrap_or_default()
}

/// **O quê:** por que um build instalado é tratado como divergente. **Onde:** [`Decisao`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Motivo {
    /// O binário não informa o commit (versão antiga, ou "fonte sem git").
    SemProveniencia,
    /// Compilado de árvore suja: não é nenhum commit do release.
    Sujo(String),
    /// Outro commit que o da tag do release.
    OutroCommit { instalado: String, release: String },
}

impl Motivo {
    /// **O quê:** a frase do motivo, sem culpa. **Onde:** [`mensagem_reinstala`].
    pub fn texto(&self) -> String {
        match self {
            Motivo::SemProveniencia => {
                "o binário instalado não informa de qual commit saiu".to_string()
            }
            Motivo::Sujo(s) => {
                format!("o binário instalado ({s}-sujo) saiu de árvore com mudança não commitada")
            }
            Motivo::OutroCommit { instalado, release } => {
                format!("o binário instalado é do commit {instalado}, o release é {release}")
            }
        }
    }
}

/// **O quê:** o veredito sobre um binário cuja VERSÃO já é a do alvo. **Onde:** [`decidir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decisao {
    /// Mesmo commit: nada a fazer.
    EmDia,
    /// Mesma versão, outro build: reinstala do release (não é rebaixamento).
    Reinstalar(Motivo),
    /// Método `fonte`: build deliberado, mantido (Exceção 1). Nem consulta a rede.
    ManterFonte,
    /// Não deu para saber o commit do release: mantém o binário e DIZ.
    NaoConfirmei { causa: Causa, msg: String },
}

/// **O quê:** decide o que fazer com um binário de versão igual à do alvo. Função PURA
/// (`release` só é chamado quando preciso: o `fonte` não toca a rede).
///
/// **Onde:** [`checar`], [`estado_build`] e o `install`.
pub fn decidir(
    metodo: Metodo,
    instalada: &Proveniencia,
    release: impl FnOnce() -> Result<String, (Causa, String)>,
) -> Decisao {
    if metodo == Metodo::Fonte {
        return Decisao::ManterFonte;
    }
    let commit = match release() {
        Ok(c) => c,
        Err((causa, msg)) => return Decisao::NaoConfirmei { causa, msg },
    };
    let Some(sha) = instalada.sha.as_deref() else {
        return Decisao::Reinstalar(Motivo::SemProveniencia);
    };
    if instalada.sujo {
        return Decisao::Reinstalar(Motivo::Sujo(sha.to_string()));
    }
    if mesmo_commit(sha, &commit) {
        Decisao::EmDia
    } else {
        Decisao::Reinstalar(Motivo::OutroCommit { instalado: sha.to_string(), release: commit })
    }
}

/// **O quê:** a fonte do commit de uma tag — injetável, para a suíte não tocar no GitHub.
///
/// **Onde:** [`commit_do_release`]; a produção usa [`CommitsReais`].
pub trait Commits {
    /// `git` está disponível? (decide entre `ls-remote` e a API.)
    fn git_no_path(&self) -> bool;
    /// A saída de `git ls-remote <repo> <refs…>` (linhas `sha<TAB>ref`).
    fn ls_remote(&self, repo: &str, refs: &[String]) -> Result<String, (Causa, String)>;
    /// O commit (40 hex) da tag pela API do GitHub — só sem `git`.
    fn api_commit(&self, repo: &str, tag: &str) -> Result<String, (Causa, String)>;
}

/// **O quê:** acha o commit de `tag` na saída do `ls-remote`. Prefere a linha peeled
/// (`refs/tags/<tag>^{}` = o commit de uma tag anotada); na falta, a leve. PURA.
///
/// **Onde:** [`commit_do_release`].
pub fn parse_ls_remote(saida: &str, tag: &str) -> Option<String> {
    let direta = format!("refs/tags/{tag}");
    let peeled = format!("{direta}^{{}}");
    let mut leve = None;
    for linha in saida.lines() {
        let Some((sha, nome)) = linha.split_once('\t') else { continue };
        let (sha, nome) = (sha.trim(), nome.trim());
        if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        if nome == peeled {
            return Some(sha.to_string());
        }
        if nome == direta {
            leve = Some(sha.to_string());
        }
    }
    leve
}

/// **O quê:** o commit (hex) da tag `tag` de `repo`. Via `ls-remote` com `git`; via API só sem
/// `git`. `Err` tipado: a tag não existir é `NotFound`, rede/cota têm a causa certa.
///
/// **Onde:** [`checar`], o `install` e o `status`.
pub fn commit_do_release<C: Commits>(
    c: &C,
    repo: &str,
    tag: &str,
) -> Result<String, (Causa, String)> {
    commit_do_release_em(c, repo, tag, plataforma::os())
}

/// **O quê:** [`commit_do_release`] para um SO dado: o commit da tag é lido do repo de ORIGEM do
/// release naquele SO ([`plataforma::repo_de_release_para`]) — no Windows, market e overdev vêm
/// do `schematize_windows_rs`, e o `.exe` embute o SHA desse repo. **Onde:** o de cima e os testes.
pub fn commit_do_release_em<C: Commits>(
    c: &C,
    repo: &str,
    tag: &str,
    os: plataforma::Os,
) -> Result<String, (Causa, String)> {
    let repo = plataforma::repo_de_release_para(repo, os);
    if !c.git_no_path() {
        let sha = c.api_commit(repo, tag)?;
        let sha = sha.trim();
        return if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
            Ok(sha.to_string())
        } else {
            Err((Causa::Unexpected, format!("resposta inesperada da API para {tag} em {repo}")))
        };
    }
    let refs = [format!("refs/tags/{tag}^{{}}"), format!("refs/tags/{tag}")];
    let saida = c.ls_remote(repo, &refs)?;
    parse_ls_remote(&saida, tag).ok_or_else(|| {
        (Causa::NotFound, format!("a tag {tag} não existe em {repo} (ou o GitHub não a listou)"))
    })
}

/// **O quê:** classifica a falha de um `git ls-remote` pela mensagem do `stderr`. PURA.
///
/// **Onde:** [`CommitsReais::ls_remote`]. Repositório/URL inexistente = `NotFound`; 403/429 =
/// cota; resolução/conexão/timeout = rede; o resto, inesperado — o texto do git vai junto.
fn classificar_falha_do_git(stderr: &str) -> Causa {
    let m = stderr.to_lowercase();
    if m.contains("not found") || m.contains("does not appear to be a git repository") {
        Causa::NotFound
    } else if m.contains("403") || m.contains("429") || m.contains("rate limit") {
        Causa::RateLimited
    } else if ["could not resolve", "timed out", "unable to access", "connection", "network"]
        .iter()
        .any(|k| m.contains(k))
    {
        Causa::Network
    } else {
        Causa::Unexpected
    }
}

/// **O quê:** a [`Commits`] de verdade: `git ls-remote` no GitHub e `curl` na API.
///
/// **Onde:** o `update`, o `install` e o `status`. `base` existe para o teste apontar para um
/// repositório local (`file://…`); em produção é `https://github.com`.
pub struct CommitsReais<'a> {
    base: String,
    ctx: Ctx<'a>,
}

impl<'a> CommitsReais<'a> {
    /// **O quê:** o GitHub de verdade. **Onde:** [`checar`] e o `status`.
    pub fn github(ctx: &Ctx<'a>) -> Self {
        CommitsReais { base: "https://github.com".to_string(), ctx: *ctx }
    }
    /// **O quê:** aponta para outro "GitHub" (um diretório `file://…`). **Onde:** os testes.
    #[cfg(test)]
    pub fn com_base(base: &str) -> Self {
        CommitsReais { base: base.to_string(), ctx: Ctx::de_teste() }
    }
}

impl Commits for CommitsReais<'_> {
    fn git_no_path(&self) -> bool {
        bin::binary_in_path("git")
    }
    fn ls_remote(&self, repo: &str, refs: &[String]) -> Result<String, (Causa, String)> {
        let url = format!("{}/{repo}.git", self.base);
        let out = Command::new("git")
            .args(["-c", "http.lowSpeedLimit=1000", "-c", "http.lowSpeedTime=20"])
            .args(["ls-remote", &url])
            .args(refs)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| (Causa::Unexpected, format!("não consegui executar o git: {e}")))?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        let erro = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err((classificar_falha_do_git(&erro), format!("git ls-remote {url} falhou: {erro}")))
    }
    fn api_commit(&self, repo: &str, tag: &str) -> Result<String, (Causa, String)> {
        let url = format!("https://api.github.com/repos/{repo}/commits/{tag}");
        let corpo =
            rede::get_text_com_cabecalho(&self.ctx, &url, "Accept: application/vnd.github.sha")
                .ok_or_else(|| {
                    (
                        Causa::Network,
                        format!(
                        "não consegui consultar a API do GitHub para o commit de {tag} em {repo} \
                         (rede fora ou cota de 60/h esgotada; instalar o git evita a cota)"
                    ),
                    )
                })?;
        Ok(corpo.trim().to_string())
    }
}

/// **O quê:** a decisão de D12 para `repo` na versão `versao`, com a rede de verdade.
/// `None` quando não se aplica (a versão instalada não é a do alvo).
///
/// **Onde:** [`super::componentes`], [`super::install_or_update`] e o plano (`--dry-run`).
pub fn checar(
    ctx: &Ctx,
    repo: &str,
    instalada: Option<&str>,
    alvo: Option<&str>,
    prov: &Proveniencia,
) -> Option<Decisao> {
    let (Some(i), Some(a)) = (instalada, alvo) else { return None };
    if i != a {
        return None;
    }
    let metodo = super::alvo::metodo_de(repo);
    Some(decidir(metodo, prov, || {
        commit_do_release(&CommitsReais::github(ctx), repo, &format!("v{a}"))
    }))
}

/// **O quê:** a decisão de [`checar`] para um app da tabela (lê a proveniência do disco).
/// **Onde:** [`super::componentes`] e o plano.
pub fn checar_app(
    ctx: &Ctx,
    app: &AppGerido,
    instalada: Option<&str>,
    alvo: Option<&str>,
) -> Option<Decisao> {
    if instalada.is_none() || instalada != alvo {
        return None; // sem versão igual não há o que comparar (e não gasta processo nem rede).
    }
    checar(ctx, app.repo, instalada, alvo, &proveniencia_do_app(app))
}

/// **O quê:** a linha que explica por que se reinstala. **Onde:** o `update` e o `install`.
pub fn mensagem_reinstala(bin: &str, versao: &str, motivo: &Motivo) -> String {
    format!(
        "→ {bin} v{versao}: mesma versão do release, mas é outro build ({}). \
         Reinstalando do release.",
        motivo.texto()
    )
}

/// **O quê:** o aviso de que o commit do release não foi confirmado e nada foi alterado.
/// **Onde:** o `update` e o `install`.
pub fn mensagem_nao_confirmei(bin: &str, versao: &str, msg: &str) -> String {
    format!(
        "aviso: {bin} v{versao}: não consegui confirmar o commit do release ({msg}) — \
         mantive o binário atual."
    )
}

/// **O quê:** o que o `status` mostra sobre o build de um app. **Onde:** [`estado_build`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EstadoBuild {
    /// O sha do binário instalado.
    pub instalado: Option<String>,
    /// O commit da tag do release.
    pub release: Option<String>,
    /// Por que não se soube o commit do release (slug estável de [`Causa`]).
    pub erro: Option<&'static str>,
    /// Mesma versão e commit diferente (ou sem proveniência).
    pub diverge: bool,
}

/// **O quê:** monta o [`EstadoBuild`] de um app para o `status`. Toca a rede (1 `ls-remote`).
/// **Onde:** `status --json`.
pub fn estado_build<C: Commits>(
    c: &C,
    repo: &str,
    instalada: Option<&str>,
    release: Option<&str>,
    prov: &Proveniencia,
) -> EstadoBuild {
    let Some(rel) = release else {
        return EstadoBuild { instalado: prov.sha.clone(), ..Default::default() };
    };
    let commit = commit_do_release(c, repo, &format!("v{rel}"));
    let mut e = EstadoBuild { instalado: prov.sha.clone(), ..Default::default() };
    match commit {
        Ok(commit) => {
            e.release = Some(commit.clone());
            e.diverge = instalada == Some(rel)
                && matches!(decidir(Metodo::Binario, prov, || Ok(commit)), Decisao::Reinstalar(_));
        }
        Err((causa, _)) => e.erro = Some(causa.slug()),
    }
    e
}

/// **O quê:** os campos JSON do [`EstadoBuild`], com `prefixo` nas chaves (`""` por app,
/// `"app_"` para o hub). Chaves nunca traduzidas; só ACRESCENTAM ao contrato. PURA.
/// **Onde:** `status --json`.
pub fn campos_json(e: &EstadoBuild, prefixo: &str) -> String {
    let txt = |o: Option<&str>| match o {
        Some(x) => format!("\"{}\"", x.replace('\\', "\\\\").replace('"', "\\\"")),
        None => "null".to_string(),
    };
    format!(
        "\"{p}installed_commit\": {}, \"{p}release_commit\": {}, \"{p}release_commit_error\": {}, \
         \"{p}build_diverge\": {}",
        txt(e.instalado.as_deref()),
        txt(e.release.as_deref()),
        txt(e.erro),
        e.diverge,
        p = prefixo
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    const SHA_REL: &str = "a36d638d233fdeadbeefdeadbeefdeadbeef0000";

    fn prov(sha: Option<&str>) -> Proveniencia {
        Proveniencia { sha: sha.map(String::from), sujo: false }
    }

    /// Mesma versão, commit DIFERENTE do release: reinstala (o bug do relato).
    #[test]
    fn mesma_versao_com_sha_diferente_reinstala() {
        let d = decidir(Metodo::Binario, &prov(Some("ddd9e48aaaaa")), || Ok(SHA_REL.to_string()));
        assert_eq!(
            d,
            Decisao::Reinstalar(Motivo::OutroCommit {
                instalado: "ddd9e48aaaaa".into(),
                release: SHA_REL.into()
            })
        );
    }

    /// Exceção 1: método `fonte` não é tocado — e nem consulta a rede.
    #[test]
    fn mesma_versao_com_metodo_fonte_nao_reinstala_nem_consulta_a_rede() {
        let chamou = Cell::new(false);
        let d = decidir(Metodo::Fonte, &prov(Some("ddd9e48aaaaa")), || {
            chamou.set(true);
            Ok(SHA_REL.to_string())
        });
        assert_eq!(d, Decisao::ManterFonte);
        assert!(!chamou.get(), "o fonte não pode gastar rede");
        // Nem sem proveniência o fonte é tocado.
        assert_eq!(
            decidir(Metodo::Fonte, &prov(None), || Ok(SHA_REL.into())),
            Decisao::ManterFonte
        );
    }

    /// Exceção 2: binário que só imprime a versão é tratado como divergente.
    #[test]
    fn mesma_versao_sem_sha_reinstala() {
        let d = decidir(Metodo::Binario, &prov(None), || Ok(SHA_REL.to_string()));
        assert_eq!(d, Decisao::Reinstalar(Motivo::SemProveniencia));
    }

    /// Mesmo commit (sha12 é prefixo do commit inteiro): nada a fazer.
    #[test]
    fn mesma_versao_com_o_mesmo_sha_nao_faz_nada() {
        let d = decidir(Metodo::Binario, &prov(Some("a36d638d233f")), || Ok(SHA_REL.to_string()));
        assert_eq!(d, Decisao::EmDia);
    }

    /// Árvore suja nunca é "o commit": reinstala mesmo com o sha igual.
    #[test]
    fn build_sujo_com_o_mesmo_sha_reinstala() {
        let p = Proveniencia { sha: Some("a36d638d233f".into()), sujo: true };
        let d = decidir(Metodo::Binario, &p, || Ok(SHA_REL.to_string()));
        assert_eq!(d, Decisao::Reinstalar(Motivo::Sujo("a36d638d233f".into())));
    }

    /// Falha ao descobrir o commit: NÃO reinstala, NÃO engole — vira `NaoConfirmei` com a causa
    /// e o aviso diz "não consegui confirmar o commit do release".
    #[test]
    fn falha_do_ls_remote_nao_reinstala_e_reporta() {
        for causa in [Causa::Network, Causa::RateLimited, Causa::NotFound, Causa::Unexpected] {
            let d = decidir(Metodo::Binario, &prov(Some("ddd9e48aaaaa")), || {
                Err((causa, "sem rede".to_string()))
            });
            assert_eq!(d, Decisao::NaoConfirmei { causa, msg: "sem rede".into() });
        }
        // Sem proveniência + falha de rede também NÃO reinstala (o download falharia igual).
        let d = decidir(Metodo::Binario, &prov(None), || Err((Causa::Network, "x".into())));
        assert!(matches!(d, Decisao::NaoConfirmei { .. }));
        let m = mensagem_nao_confirmei("schematize-git", "0.1.0", "sem rede");
        assert!(m.contains("não consegui confirmar o commit do release"), "{m}");
        assert!(m.contains("sem rede") && m.contains("schematize-git"), "{m}");
    }

    /// A linha do `--version` vira sha + sujo; versão antiga e "fonte sem git" não têm sha.
    #[test]
    fn proveniencia_da_linha_cobre_os_formatos() {
        assert_eq!(
            proveniencia_da_linha("schematize-git 0.1.0 (73847f7aabbc)"),
            prov(Some("73847f7aabbc"))
        );
        let s = proveniencia_da_linha("schematize-git 0.1.0 (73847f7aabbc-sujo)");
        assert_eq!(s, Proveniencia { sha: Some("73847f7aabbc".into()), sujo: true });
        assert_eq!(proveniencia_da_linha("schematize-git 0.1.0 (fonte sem git)"), prov(None));
        assert_eq!(proveniencia_da_linha("schematize-overdev 0.0.5"), prov(None));
        assert_eq!(proveniencia_da_linha(""), prov(None));
        assert_eq!(proveniencia_da_linha("x 1.0.0 (xyz)"), prov(None), "não é hex");
    }

    /// Tag ANOTADA: o ls-remote traz o objeto da tag e, na linha `^{}`, o COMMIT. Vale a peeled.
    #[test]
    fn tag_anotada_resolve_para_o_commit_peeled() {
        let saida = "1111111111111111111111111111111111111111\trefs/tags/v0.1.0\n\
                     2222222222222222222222222222222222222222\trefs/tags/v0.1.0^{}\n";
        assert_eq!(
            parse_ls_remote(saida, "v0.1.0").as_deref(),
            Some("2222222222222222222222222222222222222222")
        );
        // A ordem das linhas não decide.
        let invertida = "2222222222222222222222222222222222222222\trefs/tags/v0.1.0^{}\n\
                         1111111111111111111111111111111111111111\trefs/tags/v0.1.0\n";
        assert_eq!(
            parse_ls_remote(invertida, "v0.1.0").as_deref(),
            Some("2222222222222222222222222222222222222222")
        );
    }

    /// Tag LEVE: só há a linha direta, e ela já é o commit.
    #[test]
    fn tag_leve_resolve_para_a_linha_direta() {
        let saida = "3333333333333333333333333333333333333333\trefs/tags/v0.1.0\n";
        assert_eq!(
            parse_ls_remote(saida, "v0.1.0").as_deref(),
            Some("3333333333333333333333333333333333333333")
        );
    }

    /// Outra tag de prefixo parecido não casa; saída vazia/lixo é `None`.
    #[test]
    fn parse_nao_confunde_tags_parecidas_nem_aceita_lixo() {
        let saida = "4444444444444444444444444444444444444444\trefs/tags/v0.1.01\n\
                     5555555555555555555555555555555555555555\trefs/tags/v0.1.0-rc1^{}\n";
        assert_eq!(parse_ls_remote(saida, "v0.1.0"), None);
        assert_eq!(parse_ls_remote("", "v0.1.0"), None);
        assert_eq!(parse_ls_remote("nao-e-sha\trefs/tags/v0.1.0\n", "v0.1.0"), None);
    }

    /// Fonte falsa de commits: registra o que foi chamado.
    struct Falsa {
        git: bool,
        ls: Result<String, (Causa, String)>,
        api: Result<String, (Causa, String)>,
        chamadas: RefCell<Vec<String>>,
    }

    impl Commits for Falsa {
        fn git_no_path(&self) -> bool {
            self.git
        }
        fn ls_remote(&self, repo: &str, refs: &[String]) -> Result<String, (Causa, String)> {
            self.chamadas.borrow_mut().push(format!("ls:{repo}:{}", refs.join(",")));
            self.ls.clone()
        }
        fn api_commit(&self, repo: &str, tag: &str) -> Result<String, (Causa, String)> {
            self.chamadas.borrow_mut().push(format!("api:{repo}:{tag}"));
            self.api.clone()
        }
    }

    fn falsa(git: bool) -> Falsa {
        Falsa {
            git,
            ls: Ok(format!("{SHA_REL}\trefs/tags/v0.1.0^{{}}\n")),
            api: Ok(SHA_REL.to_string()),
            chamadas: RefCell::new(vec![]),
        }
    }

    /// Com git: `ls-remote` pedindo a peeled E a leve numa chamada só; a API não é tocada.
    #[test]
    fn com_git_usa_ls_remote_e_nunca_a_api() {
        let f = falsa(true);
        assert_eq!(commit_do_release(&f, "o/r", "v0.1.0").unwrap(), SHA_REL);
        let c = f.chamadas.borrow();
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0], "ls:o/r:refs/tags/v0.1.0^{},refs/tags/v0.1.0");
    }

    /// Sem git (e só então): API.
    #[test]
    fn sem_git_cai_na_api() {
        let f = falsa(false);
        assert_eq!(commit_do_release(&f, "o/r", "v0.1.0").unwrap(), SHA_REL);
        assert_eq!(f.chamadas.borrow().as_slice(), ["api:o/r:v0.1.0"]);
    }

    /// Com git falhando, o erro SOBE — não troca de via para mascará-lo (e não gasta a cota).
    #[test]
    fn git_que_falha_nao_cai_na_api() {
        let mut f = falsa(true);
        f.ls = Err((Causa::Network, "sem rede".into()));
        let e = commit_do_release(&f, "o/r", "v0.1.0").unwrap_err();
        assert_eq!(e, (Causa::Network, "sem rede".to_string()));
        assert_eq!(f.chamadas.borrow().len(), 1, "a API não pode ser chamada");
    }

    /// Tag que o remoto não tem: `NotFound` com a tag no texto — nunca um commit inventado.
    #[test]
    fn tag_ausente_no_remoto_e_not_found() {
        let mut f = falsa(true);
        f.ls = Ok(String::new());
        let (causa, msg) = commit_do_release(&f, "o/r", "v9.9.9").unwrap_err();
        assert_eq!(causa, Causa::NotFound);
        assert!(msg.contains("v9.9.9") && msg.contains("o/r"), "{msg}");
    }

    /// A API devolvendo lixo não vira commit.
    #[test]
    fn api_com_resposta_estranha_e_erro() {
        let mut f = falsa(false);
        f.api = Ok("<html>rate limit</html>".into());
        let (causa, _) = commit_do_release(&f, "o/r", "v0.1.0").unwrap_err();
        assert_eq!(causa, Causa::Unexpected);
    }

    /// `git` DE VERDADE contra um repositório local com uma tag anotada e outra leve: a anotada
    /// resolve para o COMMIT (não para o objeto da tag) e a leve para o commit direto.
    #[cfg(unix)]
    #[test]
    fn git_real_resolve_tag_anotada_e_leve_para_o_mesmo_commit() {
        let base = std::env::temp_dir().join(format!("market-lsremote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let work = base.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let git = |dir: &Path, args: &[&str]| -> String {
            let o = Command::new("git")
                .current_dir(dir)
                .args(["-c", "user.name=t", "-c", "user.email=t@test.invalid"])
                .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
                .args(args)
                .output()
                .expect("git roda");
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        git(&work, &["init", "-q"]);
        std::fs::write(work.join("a"), "1").unwrap();
        git(&work, &["add", "a"]);
        git(&work, &["commit", "-q", "-m", "um"]);
        let commit = git(&work, &["rev-parse", "HEAD"]);
        git(&work, &["tag", "-a", "v0.1.0", "-m", "anotada"]);
        git(&work, &["tag", "v0.2.0"]);
        let bare = base.join("o").join("r.git");
        std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
        git(&base, &["clone", "-q", "--bare", work.to_str().unwrap(), bare.to_str().unwrap()]);
        let tag_obj = git(&work, &["rev-parse", "v0.1.0"]);
        assert_ne!(tag_obj, commit, "o cenário precisa de uma tag anotada de verdade");

        let c = CommitsReais::com_base(&format!("file://{}", base.display()));
        assert_eq!(commit_do_release(&c, "o/r", "v0.1.0").unwrap(), commit, "anotada -> commit");
        assert_eq!(commit_do_release(&c, "o/r", "v0.2.0").unwrap(), commit, "leve -> commit");
        let (causa, _) = commit_do_release(&c, "o/r", "v7.7.7").unwrap_err();
        assert_eq!(causa, Causa::NotFound);
        let (causa, msg) = commit_do_release(&c, "nao/existe", "v0.1.0").unwrap_err();
        assert_eq!(causa, Causa::NotFound, "repo inexistente é 404, como no tri-estado do release");
        assert!(msg.contains("nao/existe"), "a causa nomeia o repo: {msg}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `estado_build`: divergente só com a MESMA versão; versão diferente é só "desatualizado".
    #[test]
    fn estado_build_marca_divergencia_apenas_na_mesma_versao() {
        let f = falsa(true);
        let e = estado_build(&f, "o/r", Some("0.1.0"), Some("0.1.0"), &prov(Some("ddd9e48aaaaa")));
        assert!(e.diverge);
        assert_eq!(e.instalado.as_deref(), Some("ddd9e48aaaaa"));
        assert_eq!(e.release.as_deref(), Some(SHA_REL));
        assert_eq!(e.erro, None);
        let ok = estado_build(&f, "o/r", Some("0.1.0"), Some("0.1.0"), &prov(Some("a36d638d233f")));
        assert!(!ok.diverge);
        let outra =
            estado_build(&f, "o/r", Some("0.0.9"), Some("0.1.0"), &prov(Some("ddd9e48aaaaa")));
        assert!(!outra.diverge, "versão menor é desatualizada, não 'build diferente'");
        let sem = estado_build(&f, "o/r", Some("0.1.0"), Some("0.1.0"), &prov(None));
        assert!(sem.diverge && sem.instalado.is_none(), "sem proveniência diverge");
    }

    /// Falha no commit: `erro` leva o slug, `diverge` é false (não se afirma o que não se sabe).
    #[test]
    fn estado_build_com_falha_reporta_o_slug_e_nao_afirma_divergencia() {
        let mut f = falsa(true);
        f.ls = Err((Causa::RateLimited, "cota".into()));
        let e = estado_build(&f, "o/r", Some("0.1.0"), Some("0.1.0"), &prov(Some("ddd9e48aaaaa")));
        assert_eq!(e.erro, Some("rate_limited"));
        assert!(!e.diverge && e.release.is_none());
    }

    /// O JSON só ACRESCENTA chaves; sem prefixo para apps, `app_` para o hub; null onde não há.
    #[test]
    fn campos_json_tem_as_chaves_novas_e_prefixo() {
        let e = EstadoBuild {
            instalado: Some("ddd9e48aaaaa".into()),
            release: Some(SHA_REL.into()),
            erro: None,
            diverge: true,
        };
        let j = campos_json(&e, "");
        assert!(j.contains("\"installed_commit\": \"ddd9e48aaaaa\""), "{j}");
        assert!(j.contains(&format!("\"release_commit\": \"{SHA_REL}\"")), "{j}");
        assert!(j.contains("\"release_commit_error\": null"), "{j}");
        assert!(j.contains("\"build_diverge\": true"), "{j}");
        let h = campos_json(&EstadoBuild { erro: Some("network"), ..Default::default() }, "app_");
        assert!(h.contains("\"app_installed_commit\": null"), "{h}");
        assert!(h.contains("\"app_release_commit_error\": \"network\""), "{h}");
        assert!(h.contains("\"app_build_diverge\": false"), "{h}");
    }

    /// A mensagem de reinstalar diz o que difere e que NÃO é rebaixamento.
    #[test]
    fn mensagem_de_reinstalar_explica_o_motivo() {
        let m = mensagem_reinstala(
            "schematize-git",
            "0.1.0",
            &Motivo::OutroCommit { instalado: "73847f7aabbc".into(), release: SHA_REL.into() },
        );
        assert!(m.contains("schematize-git") && m.contains("0.1.0"), "{m}");
        assert!(m.contains("73847f7aabbc") && m.contains("a36d638d233f"), "{m}");
        assert!(m.contains("release"), "{m}");
    }

    /// No Windows, market e overdev comparam com a tag do `schematize_windows_rs`; no resto, e
    /// para os demais apps, com o próprio repo.
    #[test]
    fn commit_do_release_roteia_o_repo_por_so() {
        use std::cell::RefCell;
        struct Grava(RefCell<Vec<String>>);
        impl Commits for Grava {
            fn git_no_path(&self) -> bool {
                true
            }
            fn ls_remote(&self, repo: &str, _: &[String]) -> Result<String, (Causa, String)> {
                self.0.borrow_mut().push(repo.to_string());
                Ok(format!("{}\trefs/tags/v1.0.0^{{}}\n", "a".repeat(40)))
            }
            fn api_commit(&self, _: &str, _: &str) -> Result<String, (Causa, String)> {
                unreachable!()
            }
        }
        let g = Grava(RefCell::new(vec![]));
        let casos = [
            (plataforma::MARKET_REPO, plataforma::Os::Windows, plataforma::WINDOWS_REPO),
            (plataforma::OVERDEV_REPO, plataforma::Os::Windows, plataforma::WINDOWS_REPO),
            (plataforma::MARKET_REPO, plataforma::Os::Linux, plataforma::MARKET_REPO),
            (plataforma::DEPLOYER_REPO, plataforma::Os::Windows, plataforma::DEPLOYER_REPO),
        ];
        for (repo, os, esperado) in casos {
            commit_do_release_em(&g, repo, "v1.0.0", os).unwrap();
            assert_eq!(g.0.borrow().last().unwrap(), esperado, "{repo} {os:?}");
        }
    }
}
