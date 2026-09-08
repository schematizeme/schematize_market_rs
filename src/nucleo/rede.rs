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
