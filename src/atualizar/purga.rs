//! PURGA — remove qualquer instalação anterior antes de instalar a nova.
//!
//! **O quê:** apaga os binários da casa em TODOS os diretórios onde alguma versão já pôde ser
//! instalada, menos o diretório de destino e menos o que se pede para manter.
//!
//! **Onde:** [`super::build_from_source`], antes de colocar os binários novos. Espelha o
//! `purge_previous` do `install.sh` — os dois caminhos de instalação têm de deixar a máquina
//! no mesmo estado.
//!
//! ## Por que existe
//!
//! O schematize já pôde ir parar em quatro lugares: `/usr/bin` (pacote .deb/.rpm),
//! `/usr/local/bin` (self-update via pkexec), `~/.local/bin` (fallback) e `~/.cargo/bin`
//! (fonte/gestor). Quem instalou por caminhos diferentes ao longo do tempo fica com várias
//! cópias, e quem "ganha" é a primeira do PATH — que pode ser a MAIS VELHA. Foi assim que uma
//! máquina recém-atualizada voltou a rodar uma versão antiga: o binário novo entrou num
//! diretório e o PATH resolveu para o outro. Atualizar não conserta isso, porque o problema
//! não é a versão — é a ambiguidade.
//!
//! **O que NÃO é tocado:** dependências do sistema e DADOS do usuário (`~/.claude`,
//! `~/.schematize`, `~/.overflow`). Purga instalação, não o trabalho de ninguém. E nunca o
//! binário que está EM EXECUÇÃO agora — quem o substitui é o `substitui_binario`.
//!
//! ## Procedência
//!
//! Porte do `purga.rs` do `schematize_updater_rs` (ADR-0013), sob o D4. O que mudou é a
//! LISTA de nomes: ela agora inclui o `schematize-market` (que passou a existir) e o
//! `schematize-optimizer` (que existia e nunca esteve aqui) — ver [`nomes`].

use crate::nucleo::plataforma;
use std::path::{Path, PathBuf};

/// **O quê:** os nomes exatos dos binários da casa, em todos os nomes que já tiveram.
///
/// **Onde:** [`remove_em`].
///
/// **Nada de padrão/glob:** um `rm schematize*` num diretório de sistema é o tipo de comando
/// que apaga o vizinho. A lista é literal, e o teste conta os itens.
///
/// **Por que os nomes MORTOS continuam aqui:** é justamente o que a purga faz. Um binário do
/// interregno (Overflow), ou o `schematize-updater` aposentado pelo ADR-0013, sentado num
/// diretório de maior precedência no PATH, é o fantasma que responde no lugar do vivo.
fn nomes() -> Vec<String> {
    let sfx = plataforma::exe_suffix();
    [
        // Interregno (o app se chamou Overflow).
        "overflow",
        "overflow-gui",
        "overflow-updater",
        "overflow-updater-gui",
        // Nomes vivos da casa.
        "schematize",
        "schematize-gui",
        "schematize-market-gui",
        "schematize-deployer",
        "schematize-optimizer",
        "schematize-market",
        // APOSENTADO pelo ADR-0013: o market o sucede. Fica na lista porque um binário que
        // ninguém mais atualiza, respondendo `update` e não fazendo nada, é pior que ausente.
        "schematize-updater",
        // O nome que a JANELA teve até o ADR-0014, quando o dono dela mudou. Quem instalou
        // antes tem este arquivo; deixá-lo cria duas janelas no menu, uma delas falando com
        // um binário que não existe mais.
        "schematize-updater-gui",
        // O nome que o Deployer teve até o commit `0fa0112`, antes de o ADR-0012 pôr o prefixo
        // da casa. Máquinas instaladas antes disso têm este arquivo — e o updater nunca o
        // listou aqui, então ele sobrevivia a toda purga.
        "deployer",
    ]
    .iter()
    .map(|b| format!("{b}{sfx}"))
    .collect()
}

/// **O quê:** todo diretório onde alguma versão já pôde ser instalada.
/// **Onde:** [`remove_copias_fantasma`].
fn diretorios() -> Vec<PathBuf> {
    let home = crate::nucleo::util::home();
    vec![
        home.join(".cargo").join("bin"),
        home.join(".local").join("bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ]
}

/// **O quê:** remove as cópias-FANTASMA — toda instalação anterior FORA do diretório em que
/// esta instalação vai escrever (`destino`). Devolve `(removidos, resistiram)`.
///
/// **Onde:** [`super::build_from_source`].
///
/// **Por que não apaga também o destino:** o binário de lá é substituído no fim, por rename, e
/// apagar antes abriria uma janela — um build de 20 minutos que falhasse deixaria a máquina
/// sem app nenhum. As cópias que causam o bug são justamente as OUTRAS: são elas que o PATH
/// pode resolver primeiro.
///
/// **`manter`** cobre o binário em execução (este) — apagar a si mesmo no meio da instalação
/// deixaria a máquina sem gestor se algo falhasse depois.
///
/// **Falhar em um deles não é erro:** pode ser `/usr/bin` sem permissão. O que resistiu é
/// reportado para o usuário resolver, em vez de fingir que limpou.
pub fn remove_copias_fantasma(destino: &Path, manter: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    remove_em(&diretorios(), destino, manter)
}

/// **O quê:** o motor da purga, com a lista de diretórios por PARÂMETRO.
///
/// **Onde:** [`remove_copias_fantasma`] e os testes.
///
/// **Por que o parâmetro:** a primeira versão disto varria `diretorios()` fixos, e o teste que
/// deveria provar "não apaga no destino" apagou os binários REAIS da máquina onde rodou.
/// Função que remove arquivo tem de ser testável sem tocar no sistema — senão o teste é que
/// vira o risco.
fn remove_em(dirs: &[PathBuf], destino: &Path, manter: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut removidos = Vec::new();
    let mut resistiram = Vec::new();
    for dir in dirs {
        if mesmo_arquivo(dir, destino) {
            continue; // o destino é trocado por rename, não apagado
        }
        for nome in nomes() {
            let alvo = dir.join(&nome);
            if !alvo.is_file() || manter.iter().any(|m| mesmo_arquivo(m, &alvo)) {
                continue;
            }
            match std::fs::remove_file(&alvo) {
                Ok(()) => removidos.push(alvo),
                Err(_) => resistiram.push(alvo),
            }
        }
    }
    (removidos, resistiram)
}

/// **O quê:** dois caminhos apontam para o mesmo arquivo?
///
/// **Onde:** [`remove_em`], nas duas guardas que impedem apagar o que não se deve.
///
/// Compara canonizado — `~/.cargo/bin/x` e `/home/u/.cargo/bin/x` são o mesmo, e não podemos
/// apagar o que pedimos para manter.
fn mesmo_arquivo(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lista de nomes é EXATA — nada de padrão que possa varrer vizinho — e cobre TODOS os
    /// binários da casa, vivos e mortos.
    ///
    /// **O que esta contagem pega:** o updater listava 4 binários e nunca soube do
    /// `schematize-deployer`, do `schematize-optimizer` nem do `schematize-market`. Uma cópia
    /// desses num dir de maior precedência sobreviveria à purga — o fantasma que a purga
    /// existe para matar, escapando por não estar na lista.
    #[test]
    fn so_nomes_exatos_e_a_casa_inteira() {
        let n = nomes();
        assert_eq!(n.len(), 13, "4 do interregno + 9 vivos/aposentados: {n:?}");
        assert_eq!(n.iter().filter(|x| x.starts_with("overflow")).count(), 4);
        assert!(n.iter().all(|x| !x.contains('*') && !x.contains('?')));
        // Os que o updater não conhecia, e por isso escapavam.
        let sfx = plataforma::exe_suffix();
        for b in [
            "schematize-deployer",
            "schematize-optimizer",
            "schematize-market",
            "schematize-market-gui",
        ] {
            assert!(n.contains(&format!("{b}{sfx}")), "{b} fora da purga — vira fantasma");
        }
        // E os aposentados: o updater (motivo do ADR-0013) e o nome que o deployer teve.
        assert!(n.contains(&format!("schematize-updater{sfx}")));
        assert!(
            n.contains(&format!("deployer{sfx}")),
            "o nome do deployer ANTES do ADR-0012 sobrevivia a toda purga"
        );
        assert!(
            n.contains(&format!("schematize-updater-gui{sfx}")),
            "o nome da JANELA antes do ADR-0014 — duas janelas no menu, uma sem backend"
        );
    }

    /// Os quatro diretórios em que uma versão já pôde ser instalada estão cobertos — é a lista
    /// inteira que faz a ambiguidade de PATH sumir.
    #[test]
    fn cobre_os_quatro_lugares_possiveis() {
        let d: Vec<String> = diretorios().iter().map(|p| p.display().to_string()).collect();
        assert!(d.iter().any(|p| p.ends_with(".cargo/bin")));
        assert!(d.iter().any(|p| p.ends_with(".local/bin")));
        assert!(d.iter().any(|p| p == "/usr/local/bin"));
        assert!(d.iter().any(|p| p == "/usr/bin"));
    }

    /// O destino não é varrido; as cópias-fantasma são. Roda 100% em diretórios temporários —
    /// `remove_em` recebe a lista justamente para o teste nunca tocar em `/usr/bin` nem em
    /// `~/.cargo/bin` da máquina que roda a suíte.
    #[test]
    fn varre_a_fantasma_e_poupa_o_destino() {
        let base = std::env::temp_dir().join(format!("market-purga-dest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let destino = base.join("destino");
        let fantasma = base.join("fantasma");
        std::fs::create_dir_all(&destino).unwrap();
        std::fs::create_dir_all(&fantasma).unwrap();
        let bom = destino.join("schematize");
        let velho = fantasma.join("schematize");
        std::fs::write(&bom, b"novo").unwrap();
        std::fs::write(&velho, b"velho").unwrap();

        let (rm, _) = remove_em(&[destino.clone(), fantasma.clone()], &destino, &[]);

        assert!(bom.is_file(), "o binário do destino tem de continuar lá");
        assert!(!velho.exists(), "a cópia-fantasma tem de sumir — é ela que o PATH pegava");
        assert_eq!(rm, vec![velho]);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// O que está em `manter` NÃO é removido — é o que impede o market de apagar a si mesmo
    /// no meio da instalação, deixando a máquina sem gestor de pacotes nenhum (R1).
    #[test]
    fn respeita_o_que_deve_ser_mantido() {
        let base = std::env::temp_dir().join(format!("market-purga-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("bin");
        std::fs::create_dir_all(&dir).unwrap();
        let eu = dir.join("schematize-market");
        let outro = dir.join("schematize-updater");
        std::fs::write(&eu, b"x").unwrap();
        std::fs::write(&outro, b"y").unwrap();

        // O destino é OUTRO diretório, então este aqui é varrido — menos o que se mantém.
        let (rm, _) =
            remove_em(std::slice::from_ref(&dir), &base.join("destino"), std::slice::from_ref(&eu));

        assert!(eu.is_file(), "apagou o binário EM EXECUÇÃO — a máquina ficaria sem gestor");
        assert!(!outro.exists(), "o aposentado tinha de sair");
        assert_eq!(rm, vec![outro]);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Canonizar tem de resolver caminhos equivalentes — senão a guarda de `manter` falha por
    /// um `.` no meio do caminho e o binário em execução some.
    #[test]
    fn caminhos_equivalentes_sao_o_mesmo_arquivo() {
        let base = std::env::temp_dir().join(format!("market-eq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let eu = base.join("schematize-market");
        std::fs::write(&eu, b"x").unwrap();
        assert!(mesmo_arquivo(&eu, &base.join(".").join("schematize-market")));
        // E arquivos diferentes não colidem.
        let outro = base.join("schematize");
        std::fs::write(&outro, b"y").unwrap();
        assert!(!mesmo_arquivo(&eu, &outro));
        let _ = std::fs::remove_dir_all(&base);
    }
}
