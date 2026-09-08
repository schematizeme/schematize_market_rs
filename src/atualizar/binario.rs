//! TROCA DE BINÁRIO em disco — o caminho rápido (asset pré-compilado) e a substituição segura.
//!
//! **O quê:** [`try_binary`] baixa e instala os binários pré-compilados de uma versão;
//! [`substitui_binario`] troca um executável que pode estar EM EXECUÇÃO, sem matar ninguém.
//!
//! **Onde:** [`super::atualizar_app`] (caminho rápido), [`super::fonte`] (depois do build) e
//! [`super::selfupdate`] (o market trocando o próprio binário).
//!
//! ## Procedência
//!
//! Porte do `install.rs` do `schematize_updater_rs` (ADR-0013), sob o D4 — o mecanismo é o
//! mesmo, incluindo o teste de regressão do `Text file busy` que o motivou.

use crate::nucleo::{plataforma, rede, util};
use std::path::Path;

/// **O quê:** tenta baixar e instalar os binários pré-compilados da versão `target`.
/// `Ok(true)` = instalou; `Ok(false)` = asset ausente OU o baixado não executa aqui (cai para
/// o fonte); `Err` = falha de disco.
///
/// **Onde:** [`super::atualizar_app`], antes do caminho do fonte.
///
/// **A verificação que não é opcional:** depois de baixar o CLI, ele é EXECUTADO com
/// `--version` antes de ir para o lugar. Um binário compilado contra uma glibc mais nova, ou
/// para a arquitetura errada, baixa com HTTP 200 e não roda — e instalá-lo por cima do que
/// funcionava deixaria a máquina sem app, com o gestor achando que deu certo.
pub fn try_binary(target: &str, cli_asset: &str, gui_asset: &str) -> Result<bool, String> {
    let tmp = plataforma::state_dir().join("dl");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)
        .map_err(|e| format!("não consegui criar {}: {e}", tmp.display()))?;

    let (cli_name, gui_name) = plataforma::bin_names();
    let cli_tmp = tmp.join(&cli_name);
    let gui_tmp = tmp.join(&gui_name);
    let tag = format!("v{target}");

    // CLI é obrigatório; ausente (404) → não há caminho binário para esta versão.
    if !rede::download(&rede::url_de_asset(plataforma::APP_REPO, &tag, cli_asset), &cli_tmp) {
        return Ok(false);
    }
    make_executable(&cli_tmp);
    if !executa_e_reporta(&cli_tmp, target) {
        return Ok(false);
    }
    // GUI é opcional: se o asset não existir, instala só o CLI.
    let has_gui =
        rede::download(&rede::url_de_asset(plataforma::APP_REPO, &tag, gui_asset), &gui_tmp);
    if has_gui {
        make_executable(&gui_tmp);
    }

    let dir = plataforma::install_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("não consegui criar {}: {e}", dir.display()))?;
    place(&cli_tmp, &dir.join(&cli_name))?;
    if has_gui {
        let _ = place(&gui_tmp, &dir.join(&gui_name));
    }
    let (i_cli, i_gui) = plataforma::bin_names_interregno();
    limpa_interregno(&dir, &i_cli);
    limpa_interregno(&dir, &i_gui);
    let _ = std::fs::remove_dir_all(&tmp);
    println!("→ instalado do binário pré-compilado v{target}.");
    Ok(true)
}

/// **O quê:** o candidato EXECUTA aqui e reporta a versão esperada?
///
/// **Onde:** [`try_binary`] e [`super::selfupdate`] — nos dois lugares onde um binário baixado
/// vai substituir um que funciona.
///
/// **Por que a versão e não só "executou":** um asset da tag errada roda perfeitamente e
/// instala a versão errada. `--version` é a única resposta que distingue "baixei o certo" de
/// "baixei algo". `esperada` vazia aceita qualquer versão — é o caso do CLI, cujo binário
/// pode reportar uma revisão adiante da tag pedida.
pub fn executa_e_reporta(candidato: &Path, esperada: &str) -> bool {
    let Some(saida) = versao_do_candidato(candidato) else {
        return false;
    };
    if esperada.is_empty() {
        return true;
    }
    saida.split_whitespace().any(|t| t.trim_start_matches('v') == esperada)
}

/// Quanto tempo se insiste quando o SO diz que o arquivo recém-gravado ainda está "ocupado".
/// 1s em passos de 25ms: folgado para a corrida abaixo, curto o bastante para não parecer
/// travamento se o motivo for outro.
const ESPERA_ETXTBSY: (u32, u64) = (40, 25);

/// **O quê:** roda `<candidato> --version` e devolve a saída. `None` se não executar.
///
/// **Onde:** [`executa_e_reporta`], o único lugar que verifica um binário recém-gravado.
///
/// **A corrida que a retentativa cobre, e por que ela é do PRODUTO e não do teste:** no Linux,
/// executar um arquivo falha com `ETXTBSY` enquanto QUALQUER processo o tiver aberto para
/// escrita. Nós fechamos o nosso — mas um `fork` que aconteça em outra thread entre o `open` e
/// o `close` **herda o descritor**, e o arquivo segue ocupado até esse filho fazer `exec`. O
/// market faz `fork` o tempo todo (curl, git, cargo), inclusive em paralelo com a gravação do
/// binário novo. Sem a retentativa, a consequência prática é a pior possível: a verificação
/// falha, a troca é recusada, e a máquina simplesmente **nunca se atualiza** — por uma janela
/// de microssegundos que nada tem a ver com a integridade do binário.
///
/// Só `ETXTBSY` é retentado. Todo outro erro — não existe, sem permissão, formato inválido,
/// arquitetura errada — é definitivo e recusa o candidato na hora, que é o ponto da
/// verificação.
fn versao_do_candidato(candidato: &Path) -> Option<String> {
    let (tentativas, ms) = ESPERA_ETXTBSY;
    for _ in 0..tentativas {
        match std::process::Command::new(candidato)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
        {
            Ok(out) if out.status.success() => {
                return Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
            }
            // Executou e saiu com erro: é resposta, não corrida. Candidato recusado.
            Ok(_) => return None,
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            Err(_) => return None,
        }
    }
    None
}

/// **O quê:** dá bit de execução ao arquivo (`0o755` no Unix; no Windows não há equivalente e
/// a chamada é inócua). **Onde:** todo binário recém-baixado ou recém-copiado.
///
/// **Por que a API do SO e não um shell-out para `chmod`:** o updater rodava `chmod +x`, que
/// depende de o `chmod` existir no PATH — num container mínimo, ou sob o PATH reduzido de um
/// lançador de desktop, ele pode não existir. Falhar aqui é best-effort dos dois jeitos, mas a
/// consequência não passa despercebida: um binário sem bit de execução não responde
/// `--version`, e [`executa_e_reporta`] recusa o candidato antes de ele substituir nada.
pub fn make_executable(p: &Path) {
    util::definir_modo(p, 0o755);
}

/// **O quê:** move `src` → `dst` (rename atômico no mesmo FS; fallback para a substituição
/// segura). **Onde:** [`try_binary`], ao tirar o download do temporário.
///
/// No Windows, se o destino estiver em uso, [`substitui_binario`] aposenta o antigo como
/// `.old` antes — não dá para sobrescrever um `.exe` em execução.
pub fn place(src: &Path, dst: &Path) -> Result<(), String> {
    // Caminho feliz: o download já está no mesmo sistema de arquivos → um rename resolve.
    if std::fs::rename(src, dst).is_ok() {
        make_executable(dst);
        return Ok(());
    }
    // Senão (EXDEV, ou destino travado no Windows), cai no mesmo mecanismo do build: grava ao
    // lado do destino e renomeia por cima. NUNCA `fs::copy` direto no destino — é o que dava
    // `Text file busy` com o agente rodando.
    substitui_binario(src, dst)
}

/// **O quê:** troca um binário que pode estar EM EXECUÇÃO, sem matar ninguém.
///
/// **Onde:** [`place`], o fim de cada build do fonte, e o self-update do próprio market.
///
/// **O erro que isto conserta:** `Text file busy` (ETXTBSY). No Linux não dá para abrir para
/// escrita um arquivo que está sendo executado — e o `schematize` está: o agente do autostart
/// roda o tempo todo. Então `fs::copy` direto no destino falhava no meio do update, depois de
/// já ter compilado tudo.
///
/// **A saída é a do Unix:** gravar um arquivo NOVO ao lado (mesmo diretório, para o rename ser
/// atômico e no mesmo sistema de arquivos) e `rename(2)` por cima. Renomear sobre um
/// executável em uso é permitido: quem já está rodando continua no inode antigo, e a próxima
/// execução pega o novo. Nada de pedir para o usuário fechar o app.
///
/// No Windows não existe esse truque (o arquivo fica travado), então lá seguimos aposentando
/// o antigo como `.old` antes de gravar.
///
/// **O que ele NÃO faz, e por que isso importa (D3/R1):** não verifica o candidato. Quem
/// substitui o binário do PRÓPRIO market tem de verificar ANTES — ver
/// [`super::selfupdate::trocar_com_rede`]. Aqui a verificação seria tarde demais para um
/// candidato que não executa.
pub fn substitui_binario(src: &Path, dst: &Path) -> Result<(), String> {
    #[cfg(windows)]
    if dst.exists() {
        let old = dst.with_extension("old");
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::rename(dst, &old);
    }
    let tmp = caminho_temporario(dst);
    let _ = std::fs::remove_file(&tmp);
    std::fs::copy(src, &tmp).map_err(|e| format!("não consegui gravar {}: {e}", tmp.display()))?;
    make_executable(&tmp);
    if let Err(e) = std::fs::rename(&tmp, dst) {
        // Não deixa o `.novo` órfão: um temporário de um binário inteiro por tentativa
        // frustrada vira lixo que ninguém sabe de onde veio.
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("não consegui substituir {}: {e}", dst.display()));
    }
    Ok(())
}

/// **O quê:** o caminho do temporário de uma substituição — `<destino>.novo`, NO MESMO
/// diretório do destino.
///
/// **Onde:** [`substitui_binario`] e [`super::selfupdate`].
///
/// **Por que o mesmo diretório, e não o `/tmp`:** `rename(2)` entre sistemas de arquivos
/// diferentes falha com `EXDEV`, e é justamente o rename que precisa funcionar. Em muitas
/// máquinas `/tmp` é um `tmpfs` — outro filesystem —, então um temporário lá tornaria a troca
/// atômica impossível exatamente onde ela mais importa.
pub fn caminho_temporario(dst: &Path) -> std::path::PathBuf {
    dst.with_file_name(format!(
        "{}.novo",
        dst.file_name().and_then(|s| s.to_str()).unwrap_or("schematize")
    ))
}

/// **O quê:** apaga um binário do INTERREGNO (nome Overflow) do diretório de instalação.
///
/// **Onde:** depois de cada instalação, no caminho binário e no do fonte.
///
/// Aquele nome saiu de circulação: não há instalação nova para escrever por cima, e deixá-lo
/// no PATH cria um binário órfão que ninguém atualiza — o fantasma clássico que faz o app
/// "voltar" para uma versão velha quando o PATH resolve para ele primeiro.
///
/// Só o diretório de instalação: as outras cópias são da [`super::purga`], que já as conhece.
pub fn limpa_interregno(dir: &Path, nome: &str) {
    let p = dir.join(nome);
    if p.exists() && std::fs::remove_file(&p).is_ok() {
        println!("→ removido binário do interregno: {}", p.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **REGRESSÃO: trocar um binário que está EM EXECUÇÃO.**
    ///
    /// Foi o bug que escapou duas vezes (Linux Mint e openSUSE): o update compilava tudo e
    /// morria no último passo com `Text file busy` (ETXTBSY), porque o `schematize` está
    /// sempre rodando — o agente do autostart. Escrever no destino falha; renomear por cima,
    /// não. Este teste monta exatamente esse cenário.
    #[cfg(unix)]
    #[test]
    fn substitui_binario_em_execucao() {
        use std::process::{Command, Stdio};

        let base = std::env::temp_dir().join(format!("market-busy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let alvo = base.join("emuso");

        // Um executável de verdade no lugar do destino, e um processo RODANDO ele.
        std::fs::copy("/bin/sleep", &alvo).unwrap();
        make_executable(&alvo);
        // O spawn também pode esbarrar no ETXTBSY da corrida descrita em `versao_do_candidato`
        // (outra thread da suíte forkando enquanto gravamos). Insiste pelo mesmo motivo.
        let mut filho = None;
        for _ in 0..40 {
            match Command::new(&alvo).arg("30").stdout(Stdio::null()).stderr(Stdio::null()).spawn()
            {
                Ok(f) => {
                    filho = Some(f);
                    break;
                }
                Err(e) if e.raw_os_error() == Some(26) => {
                    std::thread::sleep(std::time::Duration::from_millis(25))
                }
                Err(e) => panic!("não consegui rodar o binário de teste: {e}"),
            }
        }
        let mut filho = filho.expect("o binário de teste seguiu ocupado por mais de 1s");

        // O jeito ANTIGO (abrir o destino para escrita) tem de falhar — é o bug.
        //
        // A sonda é um `OpenOptions::write`, NÃO um `fs::copy`, e a diferença é o que torna o
        // teste determinístico: `spawn` volta assim que o fork acontece, e o kernel só marca o
        // texto como ocupado quando o `exec` completa — então há uma janela em que a escrita
        // ainda passa. Com `fs::copy` a primeira tentativa dessa janela SUBSTITUIRIA o binário
        // e destruiria o cenário do teste (foi o que deixou esta suíte flaky). Um `open` para
        // escrita faz exatamente a mesma checagem do kernel e não modifica nada se passar.
        let sonda = || std::fs::OpenOptions::new().write(true).open(&alvo);
        let mut erro = None;
        for _ in 0..200 {
            match sonda() {
                Err(e) if e.raw_os_error() == Some(26) => {
                    erro = Some(e);
                    break;
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(5)),
            }
        }
        let erro = erro.expect(
            "esperava ETXTBSY (26) ao abrir para escrita um binário em execução — o `exec` do \
             filho não chegou a acontecer em 1s",
        );
        assert_eq!(erro.raw_os_error(), Some(26));
        // E o `fs::copy` — o jeito que o código ANTIGO usava — falha pelo mesmo motivo.
        assert_eq!(std::fs::copy("/bin/true", &alvo).unwrap_err().raw_os_error(), Some(26));

        // O jeito NOVO (gravar ao lado + renomear) tem de passar, com o processo vivo.
        substitui_binario(Path::new("/bin/true"), &alvo).expect("substituição devia funcionar");
        assert!(filho.try_wait().unwrap().is_none(), "o processo antigo tem de seguir vivo");

        // E o destino agora é o binário NOVO (o `true` sai com 0 na hora; o `sleep` não).
        //
        // Com a mesma espera de `versao_do_candidato`, e pelo mesmo motivo: um `fork` de outra
        // thread da suíte pode ter herdado o descritor de escrita do arquivo que acabamos de
        // gravar, e o `exec` falha com ETXTBSY até esse filho executar. É a corrida do
        // produto, e o teste vive nela do mesmo jeito.
        let mut st = None;
        for _ in 0..40 {
            match Command::new(&alvo).status() {
                Ok(s) => {
                    st = Some(s);
                    break;
                }
                Err(e) if e.raw_os_error() == Some(26) => {
                    std::thread::sleep(std::time::Duration::from_millis(25))
                }
                Err(e) => panic!("o binário novo não executou: {e}"),
            }
        }
        assert!(st.expect("o binário novo seguiu ocupado por mais de 1s").success());

        let _ = filho.kill();
        let _ = filho.wait();
        let _ = std::fs::remove_dir_all(&base);
    }

    /// O temporário fica NO MESMO diretório do destino: rename entre sistemas de arquivos
    /// diferentes falha com EXDEV, e é o rename que precisa funcionar.
    #[cfg(unix)]
    #[test]
    fn temporario_fica_no_diretorio_do_destino() {
        let base = std::env::temp_dir().join(format!("market-tmpdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let alvo = base.join("bin");

        assert_eq!(caminho_temporario(&alvo), base.join("bin.novo"));
        assert_eq!(
            caminho_temporario(&alvo).parent(),
            alvo.parent(),
            "temporário fora do diretório do destino → rename falha com EXDEV"
        );

        substitui_binario(Path::new("/bin/true"), &alvo).unwrap();
        assert!(alvo.is_file());
        // não deixou lixo para trás
        assert!(!base.join("bin.novo").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Falhar a substituição NÃO deixa `.novo` órfão — um binário inteiro por tentativa
    /// frustrada vira lixo que ninguém sabe de onde veio.
    #[cfg(unix)]
    #[test]
    fn falha_na_troca_nao_deixa_temporario_orfao() {
        let base = std::env::temp_dir().join(format!("market-orfao-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        // Destino é um DIRETÓRIO não-vazio: o rename por cima falha (ENOTEMPTY/EISDIR).
        let alvo = base.join("alvo");
        std::fs::create_dir_all(alvo.join("dentro")).unwrap();

        let r = substitui_binario(Path::new("/bin/true"), &alvo);
        assert!(r.is_err(), "renomear por cima de um diretório não-vazio tinha de falhar");
        assert!(!caminho_temporario(&alvo).exists(), "o `.novo` ficou órfão");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **O que a verificação de candidato trava:** um binário que não executa, ou que executa
    /// e reporta OUTRA versão, não passa. É o que separa "baixei o certo" de "baixei algo".
    #[cfg(unix)]
    #[test]
    fn candidato_e_verificado_por_versao() {
        let base = std::env::temp_dir().join(format!("market-verif-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        // Um "binário" que responde `--version` com a versão certa.
        let bom = base.join("bom");
        std::fs::write(&bom, "#!/bin/sh\necho \"schematize-market 9.9.9\"\n").unwrap();
        make_executable(&bom);
        assert!(executa_e_reporta(&bom, "9.9.9"), "a versão certa tinha de passar");
        assert!(executa_e_reporta(&bom, ""), "sem versão esperada, basta executar");
        assert!(!executa_e_reporta(&bom, "1.0.0"), "versão ERRADA não pode passar");

        // Um candidato que não executa: arquivo de texto sem bit de execução.
        let ruim = base.join("ruim");
        std::fs::write(&ruim, "não sou um binário").unwrap();
        assert!(!executa_e_reporta(&ruim, "9.9.9"));

        // E um que nem existe.
        assert!(!executa_e_reporta(&base.join("inexistente"), "9.9.9"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
