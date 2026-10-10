//! Rede sem dependência de crate — shell-out para o cliente HTTP nativo de cada SO.
//!
//! **O quê:** [`get_text`] lê uma URL como texto (o `Cargo.toml` cru de um repo, para saber a
//! última versão) e [`download`] grava um asset em disco.
//!
//! **Onde:** [`crate::atualizar`] — resolver versão-alvo e baixar binário pré-compilado.
//!
//! ## Procedência e a razão de não haver crate HTTP aqui
//!
//! Porte do `fetch.rs` do `schematize_updater_rs` (ADR-0013), sob o D4. A escolha original se
//! mantém pelo mesmo motivo: este é o caminho que INSTALA o resto. Uma árvore de dependências
//! de TLS aqui significa que o gestor de pacotes só compila onde essa árvore compila — e o
//! único componente que precisa funcionar em toda máquina passa a ser o mais frágil de todos.
//! `curl` já está em toda instalação Unix, e no Windows desde a build 17063.

use super::util;
use std::path::Path;

/// User-Agent das requisições. O GitHub responde 403 a requisição sem UA identificável, e
/// "sem internet" seria o diagnóstico errado para esse 403.
const UA: &str = "User-Agent: schematize-market";

/// **O quê:** lê o corpo de `url` como texto. `None` se falhar (rede fora, HTTP >= 400).
/// Timeout curto — um `status` que pendura por minutos é pior que um que diz "não sei".
///
/// **Onde:** [`crate::atualizar::versao`], ao ler o `version` do `Cargo.toml` no `main`.
pub fn get_text(url: &str) -> Option<String> {
    #[cfg(not(windows))]
    {
        util::capture("curl", &["-fsSL", "-m", "20", "-H", UA, url])
    }
    #[cfg(windows)]
    {
        if let Some(c) = curl_do_windows() {
            return util::capture(c, &["-fsSL", "-m", "20", "-H", UA, url]);
        }
        let ps = format!(
            "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; \
             (Invoke-WebRequest -UseBasicParsing -TimeoutSec 20 -Uri '{url}').Content"
        );
        util::capture("powershell", &["-NoProfile", "-Command", &ps])
    }
}

/// **O quê:** baixa `url` para `dest`. `true` se o arquivo ficou gravado.
///
/// **Onde:** [`crate::atualizar`], no caminho rápido (binário pré-compilado) e no self-update.
///
/// **Por que devolve `bool` e não `Result`:** o chamador não trata as causas de forma
/// diferente — asset ausente (404), rede fora e disco cheio levam todos ao mesmo lugar:
/// compilar do fonte. O `curl` já imprimiu a causa no terminal do usuário (a saída é herdada),
/// então a informação não se perde; o que sobe é a decisão.
pub fn download(url: &str, dest: &Path) -> bool {
    let Some(dest_s) = dest.to_str() else {
        return false;
    };
    #[cfg(not(windows))]
    {
        util::run_inherit("curl", &["-fSL", "-o", dest_s, url]).is_ok() && dest.is_file()
    }
    #[cfg(windows)]
    {
        if let Some(c) = curl_do_windows() {
            return util::run_inherit(c, &["-fSL", "-o", dest_s, url]).is_ok() && dest.is_file();
        }
        let ps = format!(
            "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; \
             Invoke-WebRequest -UseBasicParsing -Uri '{url}' -OutFile '{dest_s}'"
        );
        util::run_inherit("powershell", &["-NoProfile", "-Command", &ps]).is_ok() && dest.is_file()
    }
}

/// **O quê:** qual `curl` usar no Windows, se houver. `None` manda o chamador para o
/// PowerShell. **Onde:** [`get_text`] e [`download`].
#[cfg(windows)]
fn curl_do_windows() -> Option<&'static str> {
    if super::bin::binary_in_path("curl.exe") {
        Some("curl.exe")
    } else if super::bin::binary_in_path("curl") {
        Some("curl")
    } else {
        None
    }
}

/// **O quê:** monta a URL de um asset de release. Função PURA — não toca na rede.
///
/// **Onde:** [`crate::atualizar`], e os testes que provam que a URL não mudou no porte.
///
/// **Por que existe como função:** a URL estava montada com `format!` inline em três lugares
/// do updater. Uma delas errada seria um 404 tratado como "sem binário para esta plataforma",
/// e o sintoma visível seria só a lentidão de compilar do fonte — um bug que se esconde como
/// característica.
pub fn url_de_asset(repo: &str, tag: &str, asset: &str) -> String {
    format!("https://github.com/{repo}/releases/download/{tag}/{asset}")
}

/// **O quê:** monta a URL do `Cargo.toml` cru de um repo, no `main`. Função PURA.
///
/// **Onde:** [`crate::atualizar::versao::latest_version_of`].
///
/// **Por que raw e não a API do GitHub:** a API tem cota de 60 requisições por hora por IP,
/// sem token. Um `status` que consulta cinco repos gastaria a cota de um escritório inteiro
/// atrás do mesmo NAT em pouco mais de dez execuções. O raw não tem cota.
pub fn url_do_cargo_toml(repo: &str) -> String {
    format!("https://raw.githubusercontent.com/{repo}/main/Cargo.toml")
}

/// **O quê:** a URL final do `releases/latest` de um repo — `.../releases/tag/vX.Y.Z` —, lida
/// do redirecionamento, SEM seguir. `Err` com a causa se a rede falhar ou o GitHub recusar.
///
/// **Onde:** [`crate::atualizar::alvo::consultar_publicado`], para saber o último release
/// PUBLICADO (D7).
///
/// **Por que o redirect e não a API:** `github.com/<repo>/releases/latest` é página web, sem a
/// cota de 60/h por IP da API (ver [`url_do_cargo_toml`]). Uma falha continua sendo erro
/// EXPLÍCITO — nunca vira `None` mudo que o chamador confunda com "sem release".
pub fn url_do_ultimo_release(repo: &str) -> Result<String, String> {
    let url = format!("https://github.com/{repo}/releases/latest");
    #[cfg(not(windows))]
    let saida = util::capture(
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
    );
    #[cfg(windows)]
    let saida = {
        let ps = format!(
            "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; \
             $r=Invoke-WebRequest -UseBasicParsing -Method Head -TimeoutSec 20 -Uri '{url}'; \
             if($r.BaseResponse.ResponseUri){{$u=$r.BaseResponse.ResponseUri.AbsoluteUri}}\
             else{{$u=$r.BaseResponse.RequestMessage.RequestUri.AbsoluteUri}}; \
             \"302 $u\""
        );
        util::capture("powershell", &["-NoProfile", "-Command", &ps])
    };
    let saida = saida.ok_or_else(|| {
        format!("não consegui falar com o GitHub para ler o último release de {repo} (rede fora?)")
    })?;
    redirect_do_release(repo, &saida)
}

/// **O quê:** interpreta a saída `"<http_code> <redirect_url>"` do `curl`. Função PURA.
///
/// **Onde:** [`url_do_ultimo_release`], e os testes — o `curl` não entra na suíte.
///
/// **Por que separar o código HTTP:** 403/429 do GitHub é cota ou bloqueio, e dizer "sem
/// release" sobre isso mandaria a pessoa procurar o problema no lugar errado.
pub fn redirect_do_release(repo: &str, saida: &str) -> Result<String, String> {
    let mut it = saida.trim().splitn(2, ' ');
    let codigo = it.next().unwrap_or("");
    let destino = it.next().unwrap_or("").trim();
    match codigo {
        "301" | "302" | "303" | "307" | "308" if !destino.is_empty() => Ok(destino.to_string()),
        "403" | "429" => Err(format!(
            "o GitHub recusou a consulta do último release de {repo} (HTTP {codigo}, limite de \
             requisições?) — tente de novo em alguns minutos"
        )),
        "404" => Err(format!("o repositório {repo} não foi encontrado no GitHub (HTTP 404)")),
        outro => Err(format!(
            "resposta inesperada do GitHub ao ler o último release de {repo} (HTTP {outro})"
        )),
    }
}

/// **O quê:** interpreta o `%{http_code}` do `curl` da sonda de asset. Função PURA.
///
/// **Onde:** [`asset_existe`], e os testes.
///
/// **Três estados, não dois:** `200`/`206` = existe; `404` = não existe (a ÚNICA ausência
/// confirmada); qualquer outra coisa (`000` timeout/rede, `403`/`429` cota, `5xx`, vazio) é
/// falha da SONDA — e dizer "sem binário" sobre isso mentiria sobre a causa.
pub fn classificar_http_do_asset(saida: &str) -> Result<bool, String> {
    match saida.trim() {
        "200" | "206" => Ok(true),
        "404" => Ok(false),
        "" | "000" => Err("não consegui falar com o GitHub para conferir o binário da \
                           release (rede fora ou tempo esgotado)"
            .to_string()),
        "403" | "429" => Err("o GitHub recusou a conferência do binário da release (HTTP \
                              403/429, limite de requisições?) — tente em alguns minutos"
            .to_string()),
        outro => Err(format!("resposta inesperada do GitHub ao conferir o binário (HTTP {outro})")),
    }
}

/// **O quê:** o arquivo em `url` existe? Pede só o 1º byte. `Ok(false)` só para 404 confirmado.
///
/// **Onde:** [`crate::atualizar::alvo::consultar_publicado`], para saber se o release tem o
/// asset desta plataforma ANTES de mirar nele.
///
/// **Por que `-r 0-0` e não `HEAD`:** o download passa por redirect para um storage assinado
/// que nem sempre aceita `HEAD`. **Sem `-f`**, como [`url_do_ultimo_release`]: o código HTTP
/// vem por `-w` e a sonda não escreve nada no terminal da pessoa.
pub fn asset_existe(url: &str) -> Result<bool, String> {
    #[cfg(not(windows))]
    let saida = util::capture(
        "curl",
        &["-sSL", "-m", "20", "-H", UA, "-r", "0-0", "-o", "/dev/null", "-w", "%{http_code}", url],
    );
    #[cfg(windows)]
    let saida = {
        let ps = format!(
            "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; \
             try {{ (Invoke-WebRequest -UseBasicParsing -Method Head -TimeoutSec 20 -Uri '{url}').StatusCode }} \
             catch {{ if($_.Exception.Response){{[int]$_.Exception.Response.StatusCode}}else{{0}} }}"
        );
        util::capture("powershell", &["-NoProfile", "-Command", &ps])
    };
    classificar_http_do_asset(saida.as_deref().unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nucleo::plataforma;

    /// **O que trava:** a URL do asset é EXATAMENTE a que o updater montava. O porte é
    /// movimento (D4) — se este teste falhar, o download passou a apontar para outro lugar.
    #[test]
    fn a_url_do_asset_e_a_mesma_de_antes() {
        assert_eq!(
            url_de_asset("schematizeme/schematize-cli", "v0.57.0", "schematize-linux-x86_64"),
            "https://github.com/schematizeme/schematize-cli/releases/download/v0.57.0/schematize-linux-x86_64"
        );
    }

    /// **Os quatro alvos de SO**, montados a partir da tabela de assets — não digitados de
    /// novo aqui. Um teste que redigita o nome prova que sei escrever, não que o código monta
    /// certo.
    #[test]
    fn a_url_fecha_para_os_quatro_alvos_de_so() {
        let alvos = [
            ("schematize-linux-x86_64", "schematize-gui-linux-x86_64"),
            ("schematize-macos-arm64", "schematize-gui-macos-arm64"),
            ("schematize-macos-x86_64", "schematize-gui-macos-x86_64"),
            ("schematize-windows-x86_64.exe", "schematize-gui-windows-x86_64.exe"),
        ];
        for (cli, gui) in alvos {
            for asset in [cli, gui] {
                let u = url_de_asset(plataforma::APP_REPO, "v1.2.3", asset);
                assert!(u.starts_with(
                    "https://github.com/schematizeme/schematize-cli/releases/download/v1.2.3/"
                ));
                assert!(u.ends_with(asset), "{u} não termina no asset {asset}");
                // A tag entra com o `v`; sem ele o GitHub devolve 404 e o híbrido cai para o
                // fonte em silêncio — foi assim que o `pin latest` virou alvo `vlatest`.
                assert!(u.contains("/download/v1.2.3/"), "{u}");
            }
        }
    }

    /// A sonda de asset tem TRÊS estados: só o 404 confirma ausência.
    #[test]
    fn sonda_de_asset_so_afirma_ausencia_com_404() {
        assert_eq!(classificar_http_do_asset("200"), Ok(true));
        assert_eq!(classificar_http_do_asset("206\n"), Ok(true));
        assert_eq!(classificar_http_do_asset("404"), Ok(false));
        assert!(classificar_http_do_asset("403").unwrap_err().contains("403"));
        assert!(classificar_http_do_asset("000").unwrap_err().contains("rede"));
        assert!(classificar_http_do_asset("").unwrap_err().contains("rede"));
        assert!(classificar_http_do_asset("503").unwrap_err().contains("503"));
    }

    /// A saída do `curl` vira URL de release, ou um erro que diz a causa.
    #[test]
    fn redirect_do_release_distingue_cota_de_ausencia() {
        let r = "schematizeme/schematize-cli";
        let ok = "302 https://github.com/schematizeme/schematize-cli/releases/tag/v0.65.1";
        assert!(redirect_do_release(r, ok).unwrap().ends_with("/tag/v0.65.1"));
        assert!(redirect_do_release(r, "403 ").unwrap_err().contains("limite"));
        assert!(redirect_do_release(r, "429 ").unwrap_err().contains("429"));
        assert!(redirect_do_release(r, "404 ").unwrap_err().contains("404"));
        // 302 sem destino e lixo nunca viram sucesso.
        assert!(redirect_do_release(r, "302 ").is_err());
        assert!(redirect_do_release(r, "").is_err());
    }

    /// O `Cargo.toml` é lido do RAW, no `main` — nunca da API (cota de 60/h por IP).
    #[test]
    fn a_versao_vem_do_raw_e_nao_da_api() {
        let u = url_do_cargo_toml(plataforma::DEPLOYER_REPO);
        assert_eq!(
            u,
            "https://raw.githubusercontent.com/schematizeme/schematize_deployer_rs/main/Cargo.toml"
        );
        assert!(!u.contains("api.github.com"), "a API tem cota; o raw não: {u}");
    }
}
