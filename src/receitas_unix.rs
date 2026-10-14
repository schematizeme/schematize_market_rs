//! **ReceitasUnix** — a implementação Linux/FreeBSD/macOS de [`Receitas`].
//!
//! **O quê:** o código de sempre do market, agora atrás do trait: receitas de `defs`,
//! detecção de família, execução de passo no shell de login, bootstrap do toolchain e as
//! sondas HTTP do GitHub por `curl` (ou `fetch(1)` no FreeBSD sem curl).
//!
//! **Onde:** o `main` do binário Unix (`ReceitasUnix`) e os testes. O Windows tem a sua em
//! `schematize_windows_rs`.

use crate::environments::defs::{self, Env, Recipe, Step, Tool};
use crate::environments::detect::{self, Family};
use crate::environments::{Machine, Method};
use crate::nucleo::{toolchain, util};
use crate::receitas::Receitas;
use schematize_plataforma::seams::rede::{cliente, Cliente};
use schematize_plataforma::seams::Plataforma;
use std::path::PathBuf;

/// User-Agent das requisições. O GitHub responde 403 a requisição sem UA identificável, e
/// "sem internet" seria o diagnóstico errado para esse 403.
const UA: &str = "User-Agent: schematize-market";

/// As receitas dos SO Unix (o que o market sempre fez).
#[derive(Debug, Default, Clone, Copy)]
pub struct ReceitasUnix;

impl Receitas for ReceitasUnix {
    /// **O quê:** delega a [`defs::install_recipe`]. **Onde:** `install`/`switch`.
    fn install_recipe(&self, env: &Env, m: Method, fam: Family, mise: bool) -> Recipe {
        defs::install_recipe(env, m, fam, mise)
    }

    /// **O quê:** delega a [`defs::remove_recipe`]. **Onde:** `remove`/`switch`.
    fn remove_recipe(&self, env: &Env, m: Method, fam: Family) -> Recipe {
        defs::remove_recipe(env, m, fam)
    }

    /// **O quê:** delega a [`defs::tool_install_recipe`]. **Onde:** `install <ferramenta>`.
    fn tool_install_recipe(&self, tool: &Tool, fam: Family) -> Recipe {
        defs::tool_install_recipe(tool, fam)
    }

    /// **O quê:** delega a [`defs::tool_remove_recipe`]. **Onde:** `remove <ferramenta>`.
    fn tool_remove_recipe(&self, tool: &Tool, fam: Family) -> Recipe {
        defs::tool_remove_recipe(tool, fam)
    }

    /// **O quê:** a família lida do `/etc/os-release` (FreeBSD pelo SO compilado).
    /// **Onde:** [`Machine::probe`].
    fn family(&self) -> Family {
        detect::family()
    }

    /// **O quê:** a regra Unix de métodos disponíveis (ver [`crate::environments::razao_unix`]).
    /// **Onde:** [`Machine::available`].
    fn method_reason(&self, m: &Machine, method: Method) -> Result<(), String> {
        crate::environments::razao_unix(m, method)
    }

    /// **O quê:** roda o passo pela costura `plat.shell.run_herdado` (`<shell de login> -lc`
    /// herdando stdin/stdout/stderr: saída ao vivo e o `sudo` consegue pedir a senha). Nota é
    /// no-op. **Onde:** `run_steps` das ações de instalar/remover.
    fn exec_step(&self, plat: &Plataforma, s: &Step) -> Result<(), String> {
        if s.source == "nota" {
            return Ok(());
        }
        let status =
            plat.shell.run_herdado(&s.cmd).map_err(|e| format!("falha ao executar shell: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("comando falhou ({status}): {}", s.cmd))
        }
    }

    /// **O quê:** delega a [`toolchain::ensure_toolchain`] (rustup). **Onde:** build do fonte.
    fn garantir_toolchain(&self, _plat: &Plataforma) -> Result<PathBuf, String> {
        toolchain::ensure_toolchain()
    }

    /// **O quê:** delega a [`toolchain::ensure_build_deps`]. **Onde:** build do fonte.
    fn garantir_build_deps(&self, _plat: &Plataforma) -> Result<(), String> {
        toolchain::ensure_build_deps()
    }

    /// **O quê:** `curl -w "%{http_code} %{redirect_url}"` (ou página canônica via `fetch`).
    /// **Onde:** [`crate::nucleo::rede::url_do_ultimo_release_tipado`].
    fn sondar_ultimo_release(&self, repo: &str) -> Option<String> {
        let url = format!("https://github.com/{repo}/releases/latest");
        if cliente() == Cliente::Fetch {
            let a = fetch_texto_args(&url);
            let r = util::run("fetch", &a.iter().map(String::as_str).collect::<Vec<_>>());
            return match (&r, http_de_fetch(&r)) {
                (Ok(html), _) => canonical_da_pagina(html).map(|u| format!("302 {u}")),
                (_, cod) => Some(format!("{cod} ")),
            };
        }
        util::capture(
            "curl",
            &[
                "-sS",
                "-m",
                "20",
                "-H",
                UA,
                "-o",
                "/dev/null",
                "-w",
                "%{http_code} %{redirect_url}",
                &url,
            ],
        )
    }

    /// **O quê:** `curl -r 0-0 -w %{http_code}` (ou `fetch -s`). **Onde:**
    /// [`crate::nucleo::rede::asset_existe`].
    fn sondar_asset(&self, url: &str) -> Option<String> {
        if cliente() == Cliente::Fetch {
            // `-s`: só confere (tamanho) sem baixar o corpo; o erro traz o motivo (404 etc.).
            return Some(http_de_fetch(&util::run("fetch", &["-s", "-T", "20", url])).to_string());
        }
        util::capture(
            "curl",
            &[
                "-sSL",
                "-m",
                "20",
                "-H",
                UA,
                "-r",
                "0-0",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                url,
            ],
        )
    }

    /// **O quê:** `curl -H <cabecalho>`; o `fetch` não envia cabeçalho arbitrário -> `None`.
    /// **Onde:** a consulta de commit pela API (só quando não há `git`).
    fn texto_com_cabecalho(&self, url: &str, cabecalho: &str) -> Option<String> {
        if cliente() == Cliente::Fetch {
            return None;
        }
        util::capture("curl", &["-fsSL", "-m", "20", "-H", UA, "-H", cabecalho, url])
    }
}

/// **O quê:** o trecho de shell que escreve o corpo de `url` na saída padrão, pra ir num
/// `| sh`: `curl … <url>` ou `fetch -o - <url>`. PURA.
/// **Onde:** os bootstraps `curl … | sh` (rustup) que rodam também no FreeBSD.
pub fn baixar_para_pipe(cli: Cliente, url: &str) -> String {
    match cli {
        Cliente::Curl => format!("curl --proto '=https' --tlsv1.2 -sSf {url}"),
        Cliente::Fetch => format!("fetch -o - {url}"),
    }
}

/// **O quê:** argv do `fetch` pra ler texto (`-o -`, sem barra de progresso, timeout 20s). PURA.
/// **Onde:** [`ReceitasUnix::sondar_ultimo_release`] no FreeBSD sem curl.
pub fn fetch_texto_args(url: &str) -> Vec<String> {
    ["-q", "-T", "20", "-o", "-", url].iter().map(|s| s.to_string()).collect()
}

/// **O quê:** traduz o resultado de um `fetch` de sonda no código HTTP que o resto do módulo
/// já sabe classificar (`200`, `404`, `403`, `000`). PURA.
/// **Onde:** as sondas no modo `Fetch`.
pub fn http_de_fetch(res: &Result<String, String>) -> &'static str {
    match res {
        Ok(_) => "200",
        Err(e) if e.contains("404") || e.contains("Not Found") => "404",
        Err(e) if e.contains("403") || e.contains("429") => "403",
        Err(_) => "000",
    }
}

/// **O quê:** tira de uma página de release do GitHub o `href` do `rel="canonical"` (que, para
/// `/releases/latest`, é a URL da tag). PURA.
/// **Onde:** [`ReceitasUnix::sondar_ultimo_release`] no modo `Fetch` (o `fetch` não expõe o
/// redirect).
pub fn canonical_da_pagina(html: &str) -> Option<String> {
    let i = html.find("rel=\"canonical\"")?;
    let resto = &html[i..];
    let j = resto.find("href=\"")? + 6;
    let fim = resto[j..].find('"')?;
    Some(resto[j..j + fim].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nucleo::rede::classificar_http_do_asset;

    /// Os comandos do `fetch` são os do pedido: `-o - <url>`.
    #[test]
    fn comandos_do_fetch() {
        assert_eq!(baixar_para_pipe(Cliente::Fetch, "https://x/y"), "fetch -o - https://x/y");
        assert!(baixar_para_pipe(Cliente::Curl, "https://x/y").starts_with("curl "));
        let t = fetch_texto_args("https://x/y");
        assert!(t.windows(2).any(|w| w == ["-o", "-"]) && t.last().unwrap() == "https://x/y");
    }

    /// A sonda por `fetch` cai nos MESMOS códigos HTTP que o resto classifica.
    #[test]
    fn sonda_do_fetch_vira_codigo_http() {
        assert_eq!(http_de_fetch(&Ok(String::new())), "200");
        assert_eq!(http_de_fetch(&Err("fetch: https://x: Not Found".into())), "404");
        assert_eq!(http_de_fetch(&Err("fetch: https://x: Forbidden 403".into())), "403");
        assert_eq!(http_de_fetch(&Err("fetch: sem rota".into())), "000");
        assert_eq!(classificar_http_do_asset(http_de_fetch(&Ok(String::new()))), Ok(true));
    }

    /// A URL da tag sai do `rel="canonical"` da página do release.
    #[test]
    fn canonical_da_pagina_acha_a_tag() {
        let html = r#"<head><link rel="canonical" href="https://github.com/o/r/releases/tag/v1.2.3"></head>"#;
        assert_eq!(
            canonical_da_pagina(html).as_deref(),
            Some("https://github.com/o/r/releases/tag/v1.2.3")
        );
        assert_eq!(canonical_da_pagina("<html></html>"), None);
    }
}
