//! EXIBIR — o que o `update` mostra: o plano (`--dry-run`) e o aviso de "à frente do publicado".
//!
//! **O quê:** textos do caminho de atualização, movidos de `mod.rs` só por tamanho (teto de 750
//! linhas) e estendidos com o veredito do D8.
//!
//! **Onde:** [`super::install_or_update`] e [`super::componentes`].

use super::alvo::Metodo;
use super::metodo::Origem;
use super::metodo::{self, Veredito};
use super::{alvo, ou_nenhum, Plano, Politica, APPS_GERIDOS};
use crate::ctx::Ctx;
use crate::nucleo::i18n::{t, tf};
use crate::nucleo::rede::Causa;

/// **O quê:** a dica de "nenhum app do ecossistema instalado", ou `None` se há ao menos um.
/// Função PURA sobre o [`Plano`]: o market (`Politica::Sempre`) está sempre no plano e não
/// conta como "ecossistema".
///
/// **Onde:** [`super::install_or_update`], logo após o plano/`--dry-run` e ao fim do `update`.
/// **Por que só dica e não instalar:** `update` não instala o que ninguém pediu
/// ([`Politica::SeInstalado`]); quem quer o conjunto todo pede `install --all`.
pub(super) fn dica_ecossistema_vazio(plano: &Plano) -> Option<String> {
    let ha_app = plano.componentes.iter().any(|c| c.politica == Politica::SeInstalado);
    (!ha_app).then(|| t("up.no_ecosystem"))
}

/// **O quê:** imprime o que aconteceria, sem mexer em nada. **Onde:** `install_or_update`
/// com `--dry-run`.
pub(super) fn imprimir_plano(ctx: &Ctx, plano: &Plano, instalada: Option<&str>, alvo: &str) {
    println!("{}\n", t("up.plan_title"));
    let acao_app = if plano.app {
        Veredito::Atualizar
    } else {
        metodo::veredito_do_app(instalada, Some(alvo))
    };
    println!(
        "  {:<24} {}",
        t("up.plan_app"),
        linha_do_plano(&versionado(Some(alvo)), instalada, acao_app)
    );
    for app in &plano.componentes {
        let inst = app.versao_instalada();
        let al = alvo::alvo_de(ctx, app.repo);
        let ult = match &al {
            alvo::Alvo::Versao(v) | alvo::Alvo::SemAsset { versao: v } => Some(v.clone()),
            alvo::Alvo::Indisponivel(_) => None,
        };
        let ver =
            metodo::veredito(alvo::metodo_de(app.repo), false, inst.as_deref(), ult.as_deref());
        // D12: mesma versão + outro commit também é "atualizar" — o plano não pode mentir.
        let build = match al {
            alvo::Alvo::Versao(_) => {
                super::divergencia::checar_app(ctx, app, inst.as_deref(), ult.as_deref())
            }
            _ => None,
        };
        let ver = super::componentes::veredito_com_build(ver, build.as_ref());
        println!(
            "  {:<24} {}",
            app.bin,
            linha_do_plano(&versionado(ult.as_deref()), inst.as_deref(), ver)
        );
    }
    // Os que NÃO entraram também aparecem — um plano que só mostra o que faz esconde a
    // pergunta "e o deployer, por que não?".
    for app in APPS_GERIDOS.iter().filter(|a| !plano.componentes.iter().any(|c| c.bin == a.bin)) {
        println!("  {:<24} {}", app.bin, t("up.plan_skipped"));
    }
    if plano.vazio() {
        println!("\n{}", t("up.nothing"));
    }
}

/// **O quê:** diz que `bin` está à frente do publicado e que nada foi trocado.
/// **Onde:** `install_or_update` (app) e `atualizar_componente` (market).
pub(super) fn avisar_a_frente(bin: &str, instalada: Option<&str>, publicada: &str) {
    println!(
        "{}",
        tf(
            "up.ahead",
            &[("bin", bin), ("installed", &ou_nenhum(instalada)), ("published", publicada)]
        )
    );
}

/// **O quê:** uma versão para exibição — `v0.57.0`, ou "? (rede)" quando não se sabe.
/// **Onde:** [`imprimir_plano`].
fn versionado(v: Option<&str>) -> String {
    v.map(|v| format!("v{v}")).unwrap_or_else(|| t("up.net_unknown"))
}

/// **O quê:** o "alvo X, instalado Y -> ação" de uma linha do plano.
/// **Onde:** [`imprimir_plano`], para o app e para cada componente.
fn linha_do_plano(alvo: &str, instalada: Option<&str>, ver: Veredito) -> String {
    let acao = match ver {
        Veredito::Atualizar => t("up.act_update"),
        Veredito::EmDia => t("up.act_uptodate"),
        Veredito::AFrente => t("up.act_ahead"),
    };
    tf("up.plan_line", &[("target", alvo), ("installed", &ou_nenhum(instalada)), ("action", &acao)])
}

/// **O quê:** o que se sabe sobre as versões de UM app para o `status`.
/// **Onde:** [`linhas_de_versoes`] e [`campos_json`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versoes {
    /// Último release: `Ok(None)` = o repo não tem release; `Err` = não deu para saber.
    pub release: Result<Option<String>, Causa>,
    /// `version` do `Cargo.toml` no main (`None` = não deu para ler).
    pub main: Option<String>,
    pub metodo: Metodo,
    pub origem: Origem,
}

/// **O quê:** o texto traduzido da causa de não saber o release (`? (rede)`, `? (repositório
/// não encontrado ...)`). **Onde:** [`linhas_de_versoes`].
pub fn texto_da_causa(c: Causa) -> String {
    t(match c {
        Causa::NotFound => "up.release_not_found",
        Causa::Network => "up.net_unknown",
        Causa::RateLimited => "up.release_rate",
        Causa::Unexpected => "up.release_unexpected",
    })
}

/// **O quê:** colhe as [`Versoes`] de `repo` (toca a rede). **Onde:** o `status`.
pub fn versoes_de(ctx: &Ctx, repo: &str) -> Versoes {
    let (metodo, origem) = alvo::metodo_e_origem(repo);
    Versoes {
        release: alvo::release_publicada(ctx, repo),
        main: super::versao::latest_version_of(ctx, repo),
        metodo,
        origem,
    }
}

/// **O quê:** o rótulo traduzido do método, com a origem: `binário (registrado)`.
/// **Onde:** [`linhas_de_versoes`].
pub fn rotulo_do_metodo(m: Metodo, o: Origem) -> String {
    let met = t(if m == Metodo::Binario { "up.method_bin" } else { "up.method_src" });
    let ori = t(if o == Origem::Registro { "up.origin_rec" } else { "up.origin_inf" });
    format!("{met} ({ori})")
}

/// **O quê:** as linhas `(rótulo, valor)` do `status` para um app, sem o prefixo de indentação.
/// Pura (o texto vem do catálogo). **Onde:** o `status`.
///
/// `última publicada` é o RELEASE (ou "nenhuma"); `no main` só aparece se diferir dele.
pub fn linhas_de_versoes(v: &Versoes) -> Vec<(String, String)> {
    let pub_ = match &v.release {
        Ok(Some(r)) => format!("v{r}"),
        Ok(None) => t("up.st_no_release"),
        Err(c) => texto_da_causa(*c),
    };
    let mut l = vec![(t("up.st_latest"), pub_)];
    let release = v.release.as_ref().ok().and_then(|r| r.as_deref());
    if let Some(m) = v.main.as_deref().filter(|m| Some(*m) != release) {
        l.push((t("up.st_main"), format!("v{m}")));
    }
    l.push((t("up.st_method"), rotulo_do_metodo(v.metodo, v.origem)));
    l
}

/// **O quê:** os campos JSON ESTÁVEIS de versões/método (chaves nunca traduzidas), prontos
/// para colar num objeto: `"latest_release": ..., "latest_main": ..., "install_method": ...,
/// "install_method_source": ...`. Pura. **Onde:** `status --json`.
pub fn campos_json(v: &Versoes) -> String {
    let txt = |o: Option<&str>| match o {
        Some(x) => format!("\"{}\"", x.replace('\\', "\\\\").replace('"', "\\\"")),
        None => "null".to_string(),
    };
    let rel = v.release.as_ref().ok().and_then(|r| r.as_deref());
    let erro = match &v.release {
        Err(c) => format!("\"{}\"", c.slug()),
        Ok(_) => "null".to_string(),
    };
    let origem = if v.origem == Origem::Registro { "recorded" } else { "inferred" };
    format!(
        "\"latest_release\": {}, \"latest_release_error\": {erro}, \"latest_main\": {}, \"install_method\": \"{}\", \
         \"install_method_source\": \"{origem}\"",
        txt(rel),
        txt(v.main.as_deref()),
        super::metodo::nome(v.metodo)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Máquina limpa (só o hub): o `update` avisa e nomeia o comando; com um app instalado, cala.
    #[test]
    fn dica_so_quando_nenhum_app_do_ecossistema_esta_instalado() {
        let limpa = crate::atualizar::planejar(false, Some("1.0.0"), "1.0.0", &[]);
        let d = dica_ecossistema_vazio(&limpa).expect("máquina limpa deve receber a dica");
        assert!(d.contains("schematize-market install --all"), "{d}");
        let git = APPS_GERIDOS.iter().find(|a| a.bin == "schematize-git").unwrap();
        let parcial = crate::atualizar::planejar(false, Some("1.0.0"), "1.0.0", &[git]);
        assert_eq!(dica_ecossistema_vazio(&parcial), None, "instalou só alguns: não insiste");
    }

    fn v(rel: Result<Option<&str>, Causa>, main: Option<&str>, m: Metodo, o: Origem) -> Versoes {
        Versoes {
            release: rel.map(|r| r.map(String::from)),
            main: main.map(String::from),
            metodo: m,
            origem: o,
        }
    }

    /// O caso do relato: main 0.66.0 e release 0.65.1 -> as DUAS linhas, e o método inferido.
    #[test]
    fn status_separa_release_e_main_quando_diferem() {
        let l = linhas_de_versoes(&v(
            Ok(Some("0.65.1")),
            Some("0.66.0"),
            Metodo::Binario,
            Origem::Inferido,
        ));
        let t_ = |k: &str| t(k);
        assert_eq!(l[0], (t_("up.st_latest"), "v0.65.1".to_string()));
        assert_eq!(l[1], (t_("up.st_main"), "v0.66.0".to_string()));
        assert!(l[2].1.contains(&t_("up.method_bin")) && l[2].1.contains(&t_("up.origin_inf")));
        assert_eq!(l.len(), 3);
    }

    #[test]
    fn main_igual_ao_release_nao_repete_a_linha() {
        let l = linhas_de_versoes(&v(
            Ok(Some("0.8.0")),
            Some("0.8.0"),
            Metodo::Fonte,
            Origem::Registro,
        ));
        assert_eq!(l.len(), 2);
        assert!(l[1].1.contains(&t("up.method_src")) && l[1].1.contains(&t("up.origin_rec")));
    }

    #[test]
    fn repo_sem_release_diz_nenhuma_e_nao_chuta_o_main() {
        let l = linhas_de_versoes(&v(Ok(None), Some("0.9.0"), Metodo::Fonte, Origem::Inferido));
        assert_eq!(l[0].1, t("up.st_no_release"));
        assert_eq!(l[1].1, "v0.9.0", "o main aparece na sua própria linha");
    }

    #[test]
    fn json_tem_os_campos_explicitos() {
        let j =
            campos_json(&v(Ok(Some("0.65.1")), Some("0.66.0"), Metodo::Binario, Origem::Inferido));
        assert!(j.contains("\"latest_release\": \"0.65.1\""), "{j}");
        assert!(j.contains("\"latest_main\": \"0.66.0\""), "{j}");
        assert!(j.contains("\"install_method\": \"binario\""), "{j}");
        assert!(j.contains("\"install_method_source\": \"inferred\""), "{j}");
        let j = campos_json(&v(Ok(None), None, Metodo::Fonte, Origem::Registro));
        assert!(
            j.contains("\"latest_release\": null") && j.contains("\"latest_main\": null"),
            "{j}"
        );
        assert!(j.contains("\"fonte\"") && j.contains("\"recorded\""), "{j}");
    }

    /// **A MENTIRA que este teste existe para impedir:** o repo `schematize_skills_rs` devolve
    /// 404 (privado/inexistente) e o `status` dizia `? (rede)` — mandando a pessoa checar a
    /// internet. Cada causa tem texto próprio, e o JSON leva o slug.
    #[test]
    fn cada_causa_tem_texto_proprio_e_o_404_nao_diz_rede() {
        let txt = |c| texto_da_causa(c);
        let nf = txt(Causa::NotFound);
        assert!(nf.starts_with("? (") && nf.ends_with(')'), "{nf}");
        assert_ne!(nf, t("up.net_unknown"), "404 não pode parecer problema de rede");
        let todos = [Causa::NotFound, Causa::Network, Causa::RateLimited, Causa::Unexpected];
        let textos: Vec<String> = todos.iter().map(|c| txt(*c)).collect();
        for (i, a) in textos.iter().enumerate() {
            assert!(a.starts_with("? ("), "{a}");
            for b in &textos[i + 1..] {
                assert_ne!(a, b, "duas causas com o mesmo texto");
            }
        }
        assert_eq!(txt(Causa::Network), t("up.net_unknown"), "rede mantém o texto de sempre");
        // E a linha de status usa o texto da causa.
        let l = linhas_de_versoes(&v(
            Err(Causa::NotFound),
            Some("0.1.0"),
            Metodo::Fonte,
            Origem::Inferido,
        ));
        assert_eq!(l[0].1, nf);
    }

    #[test]
    fn json_leva_o_slug_da_causa_e_release_null() {
        for (c, slug) in [
            (Causa::NotFound, "not_found"),
            (Causa::Network, "network"),
            (Causa::RateLimited, "rate_limited"),
            (Causa::Unexpected, "unexpected"),
        ] {
            let j = campos_json(&v(Err(c), Some("0.1.0"), Metodo::Fonte, Origem::Inferido));
            assert!(j.contains("\"latest_release\": null"), "{j}");
            assert!(j.contains(&format!("\"latest_release_error\": \"{slug}\"")), "{j}");
        }
        // Sem erro (inclusive "repo sem release"): o campo existe e é null.
        for rel in [Ok(Some("1.0.0")), Ok(None)] {
            let j = campos_json(&v(rel, None, Metodo::Fonte, Origem::Registro));
            assert!(j.contains("\"latest_release_error\": null"), "{j}");
        }
    }
}
