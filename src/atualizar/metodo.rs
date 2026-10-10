//! MÉTODO — como cada app foi instalado: REGISTRADO, e só na falta do registro, inferido (D8).
//!
//! **O quê:** grava `binario`/`fonte` por app no estado do market, lê de volta com erro
//! explícito, e decide o método quando não há registro.
//!
//! **Onde:** [`super::alvo::metodo_de`], os pontos de instalação ([`super::componentes`],
//! [`super::selfupdate`], `appsdacasa`) e o `status`.
//!
//! ## Por que o checkout não decide (D8)
//!
//! O D7 dizia "fonte" se `build/<repo>/.git` existisse. Esse checkout nasce do fallback de
//! compilação — o mesmo caminho que o bug original disparava —, então toda máquina que caiu no
//! bug ficava "fonte" para sempre, mirando um `main` sem binário. Aqui o registro é a verdade;
//! sem ele, o checkout só vale se for **o commit de onde o binário instalado saiu**.

use super::alvo::Metodo;
use crate::nucleo::{plataforma, util};
use std::path::{Path, PathBuf};

/// **O quê:** de onde veio o método mostrado. **Onde:** [`decidir`] e o `status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origem {
    /// Gravado pelo market quando instalou/atualizou.
    Registro,
    /// Deduzido (máquinas anteriores ao registro).
    Inferido,
}

/// **O quê:** o que fazer com a versão instalada diante do alvo. **Onde:** [`veredito`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Veredito {
    /// Instalada é diferente do alvo e abaixo dele (ou há pin): troca.
    Atualizar,
    /// Igual (ou alvo desconhecido): nada a fazer.
    EmDia,
    /// Binário instalado ACIMA do publicado: mantém e relata.
    AFrente,
}

/// **O quê:** o nome gravado/emitido de um método (`binario`/`fonte`).
/// **Onde:** o arquivo de registro e o `status --json`.
pub fn nome(m: Metodo) -> &'static str {
    match m {
        Metodo::Binario => "binario",
        Metodo::Fonte => "fonte",
    }
}

/// **O quê:** interpreta o conteúdo do registro. Vazio/desconhecido = corrompido (`Err`).
/// **Onde:** [`ler_registro_de`].
pub fn interpretar_registro(conteudo: &str) -> Result<Metodo, String> {
    match conteudo.trim() {
        "binario" => Ok(Metodo::Binario),
        "fonte" => Ok(Metodo::Fonte),
        outro => Err(format!("conteúdo inesperado {outro:?} (esperava `binario` ou `fonte`)")),
    }
}

/// **O quê:** lê o registro de um caminho: ausente = `Ok(None)`; ilegível/corrompido = `Err`.
/// **Onde:** [`metodo_com_origem`] e os testes.
pub fn ler_registro_de(p: &Path) -> Result<Option<Metodo>, String> {
    match std::fs::read_to_string(p) {
        Ok(c) => interpretar_registro(&c)
            .map(Some)
            .map_err(|e| format!("registro {} corrompido: {e}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("não consegui ler o registro {}: {e}", p.display())),
    }
}

/// **O quê:** grava o método em `p` de forma atômica (temporário ao lado + `rename`).
/// **Onde:** [`registrar`].
pub fn gravar_registro_em(p: &Path, m: Metodo) -> Result<(), String> {
    let dir = p.parent().ok_or_else(|| format!("{} não tem diretório", p.display()))?;
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("não consegui criar {}: {e}", dir.display()))?;
    let tmp = p.with_extension(format!("tmp{}", std::process::id()));
    let r = std::fs::write(&tmp, format!("{}\n", nome(m))).and_then(|()| std::fs::rename(&tmp, p));
    if let Err(e) = r {
        // O temporário não pode sobrar; se nem isso der, a causa original ainda é a dita.
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("não consegui gravar o registro {}: {e}", p.display()));
    }
    Ok(())
}

/// **O quê:** o SHA de procedência de uma linha de `--version`
/// (`schematize 0.66.0 (337747cba8e5)` -> `337747cba8e5`). Pura.
/// **Onde:** [`sha_instalado`].
pub fn sha_da_linha(linha: &str) -> Option<String> {
    let dentro = linha.rsplit_once('(')?.1.split_once(')')?.0.trim();
    let valido = (7..=40).contains(&dentro.len()) && dentro.chars().all(|c| c.is_ascii_hexdigit());
    valido.then(|| dentro.to_string())
}

/// **O quê:** os dois SHAs são o mesmo commit (um é prefixo do outro, ambos ≥ 7 hex)? Pura.
/// **Onde:** [`decidir`].
pub fn mesmo_commit(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    let hex = |s: &str| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit());
    hex(a) && hex(b) && (a.starts_with(b) || b.starts_with(a))
}

/// **O quê:** a decisão do método. Pura.
///
/// **Onde:** [`metodo_com_origem`] e os testes.
///
/// `pode_binario` = o app publica binário para esta plataforma. Registro vence; sem registro,
/// fonte só se o SHA do binário instalado é o HEAD do checkout; senão binário.
pub fn decidir(
    pode_binario: bool,
    registro: Option<Metodo>,
    sha_instalado: Option<&str>,
    head: Option<&str>,
) -> (Metodo, Origem) {
    if let Some(m) = registro {
        return (m, Origem::Registro);
    }
    let do_checkout = matches!((sha_instalado, head), (Some(a), Some(b)) if mesmo_commit(a, b));
    let m = if pode_binario && !do_checkout { Metodo::Binario } else { Metodo::Fonte };
    (m, Origem::Inferido)
}

/// **O quê:** o que fazer com a versão instalada. Pura.
///
/// **Onde:** [`veredito_do_app`], `atualizar_componente` e o plano.
///
/// **Regra (D8):** fonte ou pin ativo -> diferente = atualiza (pin vence, rebaixa). Binário sem
/// pin: abaixo atualiza, igual em dia, ACIMA mantém.
pub fn veredito(
    metodo: Metodo,
    pin_ativo: bool,
    instalada: Option<&str>,
    alvo: Option<&str>,
) -> Veredito {
    let diferente = || {
        if super::versao::desatualizado(instalada, alvo) {
            Veredito::Atualizar
        } else {
            Veredito::EmDia
        }
    };
    let (Some(i), Some(a)) = (instalada, alvo) else { return diferente() };
    if metodo == Metodo::Fonte || pin_ativo || i == a {
        return diferente();
    }
    if util::semver_lt(i, a) {
        Veredito::Atualizar
    } else if util::semver_lt(a, i) {
        Veredito::AFrente
    } else {
        Veredito::Atualizar // só o sufixo difere (rc vs final): é diferente, troca.
    }
}

/// **O quê:** o arquivo de registro do método de `repo` — `<state_dir>/metodo/<repo>`.
/// **Onde:** [`registrar`] e [`metodo_com_origem`].
pub fn arquivo_do_registro(repo: &str) -> PathBuf {
    let name = repo.rsplit('/').next().unwrap_or(repo);
    plataforma::state_dir().join("metodo").join(name)
}

/// **O quê:** grava o método com que `repo` acabou de ser instalado. Falha vira AVISO na tela
/// (nunca silêncio, nunca derruba a instalação que já deu certo).
/// **Onde:** logo após cada instalação/atualização bem-sucedida.
pub fn registrar(repo: &str, m: Metodo) {
    if let Err(e) = gravar_registro_em(&arquivo_do_registro(repo), m) {
        eprintln!("aviso: {e} — o método será inferido na próxima vez.");
    }
}

/// **O quê:** o HEAD do checkout persistente de `repo`, ou `None` se não há/ilegível.
/// **Onde:** [`metodo_com_origem`].
pub fn head_do_checkout(repo: &str) -> Option<String> {
    let dir = plataforma::build_src_dir(repo);
    if !dir.join(".git").exists() {
        return None;
    }
    util::capture("git", &["-C", dir.to_str()?, "rev-parse", "HEAD"])
}

/// **O quê:** o SHA de procedência do binário instalado de `repo` (via `--version`). O market
/// usa o SHA compilado em si mesmo. **Onde:** [`metodo_com_origem`].
pub fn sha_instalado(repo: &str) -> Option<String> {
    if repo == plataforma::MARKET_REPO {
        return crate::nucleo::procedencia::sha().map(String::from);
    }
    let bin = if repo == plataforma::APP_REPO { plataforma::app_bin(false) } else { return None };
    sha_da_linha(&util::capture(bin.to_str()?, &["--version"])?)
}

/// **O quê:** o método de `repo` e de onde veio a resposta. Toca disco e roda `git`/`--version`
/// só na falta do registro. Registro ilegível avisa UMA vez por app e cai na inferência.
/// **Onde:** [`super::alvo::metodo_de`] e o `status`.
pub fn metodo_com_origem(repo: &str, pode_binario: bool) -> (Metodo, Origem) {
    let registro = match ler_registro_de(&arquivo_do_registro(repo)) {
        Ok(r) => r,
        Err(e) => {
            avisar_uma_vez(repo, &e);
            None
        }
    };
    if registro.is_some() || !pode_binario {
        return decidir(pode_binario, registro, None, None);
    }
    decidir(pode_binario, None, sha_instalado(repo).as_deref(), head_do_checkout(repo).as_deref())
}

/// **O quê:** imprime o aviso de registro ruim uma vez por app e por execução.
/// **Onde:** [`metodo_com_origem`].
fn avisar_uma_vez(repo: &str, erro: &str) {
    use std::sync::{Mutex, OnceLock};
    static JA: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    let mut ja =
        JA.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap_or_else(|e| e.into_inner());
    if !ja.iter().any(|r| r == repo) {
        ja.push(repo.to_string());
        eprintln!("{}", crate::nucleo::i18n::tf("up.method_bad", &[("error", erro)]));
    }
}

/// **O quê:** o veredito do app (hub) considerando pin e método. **Onde:** `install_or_update`.
pub fn veredito_do_app(instalada: Option<&str>, alvo: Option<&str>) -> Veredito {
    let metodo = super::alvo::metodo_de(plataforma::APP_REPO);
    veredito(metodo, super::versao::read_pin().is_some(), instalada, alvo)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA_A: &str = "337747cba8e5";
    const SHA_B: &str = "a1b2c3d4e5f6";

    /// O BUG DESTA MÁQUINA: checkout velho (outro HEAD) não faz o app virar "fonte".
    #[test]
    fn checkout_velho_com_sha_diferente_e_binario() {
        let (m, o) = decidir(true, None, Some(SHA_A), Some(SHA_B));
        assert_eq!((m, o), (Metodo::Binario, Origem::Inferido));
    }

    #[test]
    fn sha_igual_ao_head_e_fonte_comparando_por_prefixo() {
        let completo = "337747cba8e5aaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert_eq!(decidir(true, None, Some(SHA_A), Some(completo)).0, Metodo::Fonte);
        assert_eq!(decidir(true, None, Some(completo), Some(SHA_A)).0, Metodo::Fonte);
    }

    #[test]
    fn sem_sha_no_version_ou_sem_checkout_e_binario() {
        assert_eq!(decidir(true, None, None, Some(SHA_A)).0, Metodo::Binario);
        assert_eq!(decidir(true, None, Some(SHA_A), None).0, Metodo::Binario);
        assert_eq!(decidir(true, None, None, None).0, Metodo::Binario);
    }

    #[test]
    fn o_registro_vence_a_inferencia_nos_dois_sentidos() {
        let r = decidir(true, Some(Metodo::Binario), Some(SHA_A), Some(SHA_A));
        assert_eq!(r, (Metodo::Binario, Origem::Registro));
        let r = decidir(true, Some(Metodo::Fonte), Some(SHA_A), Some(SHA_B));
        assert_eq!(r, (Metodo::Fonte, Origem::Registro));
    }

    #[test]
    fn app_sem_binario_publicado_so_tem_o_fonte() {
        assert_eq!(decidir(false, None, Some(SHA_A), Some(SHA_B)).0, Metodo::Fonte);
    }

    #[test]
    fn mesmo_commit_exige_hex_e_minimo_de_7() {
        assert!(mesmo_commit("abcdef0", "abcdef0123"));
        assert!(!mesmo_commit("abcdef", "abcdef0123"), "curto demais");
        assert!(!mesmo_commit("abcdef0", "abcdef1"));
        assert!(!mesmo_commit("", ""));
    }

    #[test]
    fn sha_vem_da_linha_de_version() {
        assert_eq!(sha_da_linha("schematize 0.66.0 (337747cba8e5)").as_deref(), Some(SHA_A));
        assert_eq!(sha_da_linha("schematize 0.66.0 (fonte sem git)"), None);
        assert_eq!(sha_da_linha("schematize-market 0.3.0"), None);
        assert_eq!(sha_da_linha("schematize 0.1.0 (fatal:)"), None);
    }

    #[test]
    fn instalado_acima_do_publicado_e_mantido() {
        let v = veredito(Metodo::Binario, false, Some("0.66.0"), Some("0.65.1"));
        assert_eq!(v, Veredito::AFrente);
        // comparação numérica, não lexicográfica
        assert_eq!(
            veredito(Metodo::Binario, false, Some("0.10.0"), Some("0.9.0")),
            Veredito::AFrente
        );
    }

    #[test]
    fn abaixo_atualiza_e_igual_esta_em_dia() {
        assert_eq!(
            veredito(Metodo::Binario, false, Some("0.64.0"), Some("0.65.1")),
            Veredito::Atualizar
        );
        assert_eq!(
            veredito(Metodo::Binario, false, Some("0.65.1"), Some("0.65.1")),
            Veredito::EmDia
        );
        assert_eq!(veredito(Metodo::Binario, false, None, Some("0.65.1")), Veredito::Atualizar);
        assert_eq!(veredito(Metodo::Binario, false, Some("0.65.1"), None), Veredito::EmDia);
    }

    #[test]
    fn pin_mais_baixo_rebaixa_de_proposito() {
        let v = veredito(Metodo::Binario, true, Some("0.66.0"), Some("0.65.1"));
        assert_eq!(v, Veredito::Atualizar, "o pin é explícito: vence");
    }

    #[test]
    fn fonte_mantem_a_regra_antiga_de_diferente_atualiza() {
        let v = veredito(Metodo::Fonte, false, Some("0.66.0"), Some("0.65.1"));
        assert_eq!(v, Veredito::Atualizar);
    }

    fn sandbox(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("market-metodo-{n}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn registro_ida_e_volta_e_ausente_nao_e_erro() {
        let d = sandbox("rt");
        let p = d.join("metodo").join("app");
        assert_eq!(ler_registro_de(&p), Ok(None), "ausente = ausente");
        gravar_registro_em(&p, Metodo::Binario).unwrap();
        assert_eq!(ler_registro_de(&p), Ok(Some(Metodo::Binario)));
        gravar_registro_em(&p, Metodo::Fonte).unwrap();
        assert_eq!(ler_registro_de(&p), Ok(Some(Metodo::Fonte)));
        let sobras: Vec<_> = std::fs::read_dir(p.parent().unwrap()).unwrap().flatten().collect();
        assert_eq!(sobras.len(), 1, "gravação atômica não deixa temporário");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn registro_corrompido_ou_ilegivel_e_erro_explicito() {
        let d = sandbox("bad");
        let p = d.join("x");
        std::fs::write(&p, "talvez\n").unwrap();
        assert!(ler_registro_de(&p).unwrap_err().contains("x"), "o erro cita o arquivo");
        std::fs::write(&p, "").unwrap();
        assert!(ler_registro_de(&p).is_err(), "vazio é corrompido, não ausente");
        // diretório no lugar do arquivo: ilegível, não ausente
        let dir = d.join("dir");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(ler_registro_de(&dir).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
