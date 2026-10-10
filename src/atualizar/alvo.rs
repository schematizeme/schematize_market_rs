//! ALVO — qual versão um `update` deve mirar, conforme COMO o componente foi instalado.
//!
//! **O quê:** decide a versão-alvo de um repo da casa. Quem instala por **binário** mira o
//! último release PUBLICADO que tem o asset desta plataforma; quem instala do **fonte** mira o
//! `version` do `Cargo.toml` no `main`.
//!
//! **Onde:** [`super::install_or_update`], [`super::atualizar_componente`] e o `status`.
//!
//! ## Por que isto existe (D7)
//!
//! O alvo era sempre o `Cargo.toml` do `main`. Com o hub em 0.66.0 no `main` e o último release
//! em v0.65.1, quem instalou por binário passava a mirar uma versão SEM asset: o download dava
//! 404, o código caía no build do fonte, e o build pedia `sudo` sem terminal. A pessoa não fez
//! nada de errado — o gestor mirou algo que não existe para ela (§37.48).

use super::versao;
use crate::nucleo::{plataforma, rede};

/// **O quê:** como este componente foi instalado nesta máquina.
///
/// **Onde:** [`escolher_alvo`] e quem decide se um alvo sem asset pode cair no fonte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metodo {
    /// Veio de um asset pré-compilado de release.
    Binario,
    /// Foi compilado do fonte (há checkout persistente), ou só existe esse caminho.
    Fonte,
}

/// **O quê:** o último release publicado e se ele tem o asset desta plataforma.
///
/// **Onde:** entrada de [`escolher_alvo`]; montado por [`consultar_publicado`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publicado {
    /// A versão do release, SEM o `v` da tag.
    pub versao: String,
    /// O asset desta plataforma existe nesse release?
    pub tem_asset: bool,
}

/// **O quê:** o veredito sobre qual versão mirar.
///
/// **Onde:** devolvido por [`escolher_alvo`] e [`alvo_de`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alvo {
    /// Mira esta versão — ela existe para este método de instalação.
    Versao(String),
    /// O último release existe, mas NÃO tem asset para esta plataforma. Nunca vira build
    /// implícito do fonte.
    SemAsset { versao: String },
    /// Não deu para saber (rede fora, cota, repo sem release). Texto acionável.
    Indisponivel(String),
}

/// **O quê:** decide o alvo. Função PURA — não toca em rede nem em disco.
///
/// **Onde:** [`alvo_de`], e os testes.
///
/// **A regra (D7):**
/// - **Fonte** -> o `Cargo.toml` do `main` (`main`); a consulta de release é ignorada, porque
///   quem compila não precisa de asset e uma falha nela não pode atrapalhar.
/// - **Binário** -> o último release publicado, SE tiver o asset desta plataforma. Sem asset
///   vira [`Alvo::SemAsset`]; falha na consulta vira [`Alvo::Indisponivel`] com a causa. O
///   `main` NUNCA entra aqui: é justamente a versão que pode não existir como binário.
pub fn escolher_alvo(
    metodo: Metodo,
    main: Option<&str>,
    publicado: &Result<Publicado, String>,
) -> Alvo {
    match metodo {
        Metodo::Fonte => match main {
            Some(v) => Alvo::Versao(v.to_string()),
            None => Alvo::Indisponivel(
                "não consegui ler a versão no Cargo.toml do repositório (rede fora?)".to_string(),
            ),
        },
        Metodo::Binario => match publicado {
            Ok(p) if p.tem_asset => Alvo::Versao(p.versao.clone()),
            Ok(p) => Alvo::SemAsset { versao: p.versao.clone() },
            Err(e) => Alvo::Indisponivel(e.clone()),
        },
    }
}

/// **O quê:** extrai a versão da URL para a qual `releases/latest` redireciona
/// (`.../releases/tag/v0.65.1` -> `0.65.1`). Função PURA.
///
/// **Onde:** [`consultar_publicado`].
///
/// **Exige `tag/v<dígito>`:** repo sem release redireciona para `/releases` (sem `tag/`), e
/// uma tag que não é versão (`nightly`) não pode virar alvo de download.
pub fn versao_da_url_de_release(url: &str) -> Option<String> {
    let tag = url.trim().rsplit_once("/releases/tag/")?.1;
    let v = tag.strip_prefix('v')?;
    v.chars().next().filter(char::is_ascii_digit)?;
    Some(v.to_string())
}

/// **O quê:** a mensagem de um alvo sem binário, com o comando exato de build do fonte.
///
/// **Onde:** [`super::atualizar_app`] e [`super::atualizar_componente`].
///
/// **Sem culpa (§37.48):** diz o que aconteceu, que nada mudou, e oferece o caminho — o build
/// do fonte instala pacotes de sistema e pede `sudo`, então é a pessoa quem decide.
pub fn mensagem_sem_binario(repo: &str, versao: &str) -> String {
    let comando = if repo == plataforma::MARKET_REPO {
        format!("cargo install --git https://github.com/{repo} --force")
    } else {
        format!(
            "curl -fsSL https://raw.githubusercontent.com/{repo}/main/install.sh | bash -s -- --from-source"
        )
    };
    format!(
        "A v{versao} de {repo} ainda não tem binário pronto para esta máquina, então não \
         vou compilar por conta própria (compilar instala pacotes do sistema e pede sudo). \
         Nada foi alterado: o que está instalado continua funcionando.\n  \
         Se quiser compilar do fonte, rode:\n    {comando}"
    )
}

/// **O quê:** como `repo` foi instalado nesta máquina. Toca o disco.
///
/// **Onde:** [`alvo_de`].
///
/// **O sinal, e o que NÃO existe:** não há registro de "instalei por binário". O que há é o
/// checkout persistente que só o build do fonte cria (`build_src_dir`): com ele, o componente
/// é do fonte; sem ele, veio de binário (ou é instalação nova, que começa pelo binário). Só o
/// hub e o market publicam binário; os demais apps só têm o caminho do fonte neste código,
/// então para eles o `main` é a resposta certa.
pub fn metodo_de(repo: &str) -> Metodo {
    metodo_pelo_sinal(repo, plataforma::build_src_dir(repo).join(".git").is_dir())
}

/// **O quê:** a decisão de [`metodo_de`] sem tocar o disco. Função PURA.
///
/// **Onde:** [`metodo_de`] e os testes.
pub fn metodo_pelo_sinal(repo: &str, tem_checkout: bool) -> Metodo {
    let publica_binario = repo == plataforma::APP_REPO || repo == plataforma::MARKET_REPO;
    if publica_binario && !tem_checkout && nome_do_asset(repo).is_some() {
        Metodo::Binario
    } else {
        Metodo::Fonte
    }
}

/// **O quê:** o nome do asset de `repo` nesta plataforma (o CLI do hub, ou o do market).
/// `None` se a plataforma não tem binário pronto. **Onde:** [`metodo_pelo_sinal`] e
/// [`consultar_publicado`].
fn nome_do_asset(repo: &str) -> Option<String> {
    if repo == plataforma::MARKET_REPO {
        plataforma::market_asset_name()
    } else {
        plataforma::asset_names().map(|(cli, _)| cli)
    }
}

/// **O quê:** consulta o último release publicado de `repo` e se ele tem o asset desta
/// plataforma. Toca a rede. **Onde:** [`alvo_de`], só para instalações por binário.
pub fn consultar_publicado(repo: &str) -> Result<Publicado, String> {
    let asset = nome_do_asset(repo)
        .ok_or_else(|| format!("esta plataforma não tem binário pré-compilado de {repo}"))?;
    let url = rede::url_do_ultimo_release(repo)?;
    let versao = versao_da_url_de_release(&url).ok_or_else(|| {
        format!("{repo} ainda não tem nenhum release publicado (o GitHub apontou para {url})")
    })?;
    let tem_asset = rede::asset_existe(&rede::url_de_asset(repo, &format!("v{versao}"), &asset));
    Ok(Publicado { versao, tem_asset })
}

/// **O quê:** o alvo de `repo` agora: descobre o método, consulta o que o método precisa e
/// decide por [`escolher_alvo`]. **Onde:** o `update`, o plano e o `status`.
pub fn alvo_de(repo: &str) -> Alvo {
    let metodo = metodo_de(repo);
    match metodo {
        Metodo::Fonte => {
            let main = versao::latest_version_of(repo);
            escolher_alvo(metodo, main.as_deref(), &Err(String::new()))
        }
        Metodo::Binario => escolher_alvo(metodo, None, &consultar_publicado(repo)),
    }
}

/// **O quê:** o alvo do APP (hub): o pin se houver, senão [`alvo_de`].
///
/// **Onde:** [`super::install_or_update`].
///
/// **O pin vence e não é checado:** quem fixou uma versão pediu aquela, com ou sem asset. Se
/// não houver binário dela, o `update` diz isso — não troca por outra nem compila.
pub fn alvo_do_app() -> Alvo {
    match versao::read_pin() {
        Some(p) => Alvo::Versao(p),
        None => alvo_de(plataforma::APP_REPO),
    }
}

/// **O quê:** a versão publicada de `repo` para EXIBIR e comparar, ou `None` se não se sabe.
///
/// **Onde:** o plano, o `status` e [`super::atualizar_componente`] — no lugar do antigo
/// `latest_version_of` direto. `SemAsset` devolve a versão do release: a pessoa vê o que há
/// de novo, e o `update` explica por que não instala.
pub fn ultima_publicada(repo: &str) -> Option<String> {
    match alvo_de(repo) {
        Alvo::Versao(v) | Alvo::SemAsset { versao: v } => Some(v),
        Alvo::Indisponivel(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pub_(v: &str, tem: bool) -> Result<Publicado, String> {
        Ok(Publicado { versao: v.into(), tem_asset: tem })
    }

    /// **O BUG DO RELATO:** hub com `main` = 0.66.0 e release v0.65.1 com asset. Quem instalou
    /// por binário mira 0.65.1 — nunca a versão do `main`, que não tem asset.
    #[test]
    fn binario_mira_o_release_publicado_e_nao_o_main() {
        let a = escolher_alvo(Metodo::Binario, Some("0.66.0"), &pub_("0.65.1", true));
        assert_eq!(a, Alvo::Versao("0.65.1".into()));
    }

    /// Release existe mas sem asset desta plataforma: `SemAsset`, nunca um alvo "buildável".
    #[test]
    fn release_sem_asset_nao_vira_alvo() {
        let a = escolher_alvo(Metodo::Binario, Some("0.66.0"), &pub_("0.65.1", false));
        assert_eq!(a, Alvo::SemAsset { versao: "0.65.1".into() });
    }

    /// Quem instala do fonte continua mirando o `main`, mesmo com release mais velho.
    #[test]
    fn fonte_continua_mirando_o_main() {
        let a = escolher_alvo(Metodo::Fonte, Some("0.66.0"), &pub_("0.65.1", true));
        assert_eq!(a, Alvo::Versao("0.66.0".into()));
        // E nem consulta o release: falha na consulta não atrapalha quem compila.
        let b = escolher_alvo(Metodo::Fonte, Some("0.66.0"), &Err("cota".into()));
        assert_eq!(b, Alvo::Versao("0.66.0".into()));
    }

    /// Rede/cota fora para quem instala por binário: erro EXPLÍCITO, nunca cair no `main`.
    #[test]
    fn falha_na_consulta_do_release_e_erro_explicito_nao_fallback_para_o_main() {
        let a = escolher_alvo(Metodo::Binario, Some("0.66.0"), &Err("HTTP 403 (cota)".into()));
        match a {
            Alvo::Indisponivel(m) => assert!(m.contains("403"), "a causa tem de aparecer: {m}"),
            outro => panic!("devia ser Indisponivel, veio {outro:?}"),
        }
    }

    /// Sem `Cargo.toml` legível, quem compila do fonte também fica sem alvo — dito, não chutado.
    #[test]
    fn fonte_sem_main_legivel_e_indisponivel() {
        let a = escolher_alvo(Metodo::Fonte, None, &pub_("0.65.1", true));
        assert!(matches!(a, Alvo::Indisponivel(m) if !m.is_empty()));
    }

    /// O sinal de instalação: só hub e market publicam binário, e só sem checkout do fonte.
    #[test]
    fn metodo_vem_do_checkout_e_so_hub_e_market_publicam_binario() {
        use crate::nucleo::plataforma::{APP_REPO, DEPLOYER_REPO, MARKET_REPO};
        if plataforma::asset_names().is_some() {
            assert_eq!(metodo_pelo_sinal(APP_REPO, false), Metodo::Binario);
            assert_eq!(metodo_pelo_sinal(MARKET_REPO, false), Metodo::Binario);
        }
        assert_eq!(metodo_pelo_sinal(APP_REPO, true), Metodo::Fonte, "há checkout: é do fonte");
        // Os demais só têm o caminho do fonte aqui: o `main` é o alvo certo.
        assert_eq!(metodo_pelo_sinal(DEPLOYER_REPO, false), Metodo::Fonte);
    }

    /// A URL de redirecionamento do `releases/latest` vira versão sem o `v`.
    #[test]
    fn versao_vem_da_url_do_redirect() {
        let base = "https://github.com/schematizeme/schematize-cli/releases/tag";
        assert_eq!(versao_da_url_de_release(&format!("{base}/v0.65.1")).as_deref(), Some("0.65.1"));
        assert_eq!(
            versao_da_url_de_release(&format!("{base}/v1.0.0-rc.2\n")).as_deref(),
            Some("1.0.0-rc.2")
        );
        // Repo sem release redireciona para `/releases` (sem `tag/`): não há versão.
        let sem = "https://github.com/schematizeme/schematize-cli/releases";
        assert_eq!(versao_da_url_de_release(sem), None);
        assert_eq!(versao_da_url_de_release(""), None);
        assert_eq!(versao_da_url_de_release(&format!("{base}/nightly")), None, "tag sem versão");
    }

    /// A mensagem diz o que houve, o que NÃO foi feito, e o comando exato — sem culpar.
    #[test]
    fn mensagem_sem_binario_e_acionavel_e_sem_culpa() {
        let m = mensagem_sem_binario("schematizeme/schematize-cli", "0.65.1");
        assert!(m.contains("v0.65.1"), "{m}");
        assert!(m.contains("install.sh") && m.contains("--from-source"), "o comando exato: {m}");
        assert!(m.contains("Nada foi alterado"), "{m}");
        let mk = mensagem_sem_binario("schematizeme/schematize_market_rs", "0.3.1");
        assert!(
            mk.contains("cargo install --git https://github.com/schematizeme/schematize_market_rs"),
            "{mk}"
        );
        for t in [&m, &mk] {
            assert!(!t.to_lowercase().contains("você errou") && !t.contains("culpa"), "{t}");
        }
    }
}
