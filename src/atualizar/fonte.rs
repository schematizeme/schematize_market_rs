//! BUILD DO FONTE — o caminho confiável, que funciona em qualquer SO com toolchain.
//!
//! **O quê:** sincroniza um checkout PERSISTENTE por repo e compila com `cargo build
//! --release` sobre um `target/` COMPARTILHADO, copiando o binário para o diretório de
//! instalação.
//!
//! **Onde:** [`super::atualizar_app`] quando não há asset para a plataforma (ou o baixado não
//! executa aqui), e [`super::atualizar_componente`] para todo app que não publica binário.
//!
//! ## Procedência
//!
//! Porte do `install.rs` do `schematize_updater_rs` (ADR-0013), sob o D4. A escolha central
//! se mantém: **incremental**, não `cargo install --force`. O `--force` jogava fora o cache e
//! recompilava a árvore inteira toda vez; com checkout persistente e `target/` compartilhado,
//! só o que mudou recompila, e as ~226 deps comuns aos repos compilam UMA vez.

use super::binario::{limpa_interregno, substitui_binario};
use crate::nucleo::{plataforma, util};
use std::path::Path;

/// **O quê:** compila e instala o app do fonte (CLI + GUI Slint), mais a janela opcional.
///
/// **Onde:** [`super::atualizar_app`], como caminho confiável.
///
/// **A ordem importa:** a PURGA das cópias-fantasma vem antes de instalar, o CLI antes da GUI
/// (a GUI depende do crate dele), e a limpeza dos `target/` antigos só no fim — se o build
/// falhar, o cache anterior continua lá.
pub fn build_from_source() -> Result<(), String> {
    let cargo = plataforma::ensure_toolchain()?;
    plataforma::ensure_build_deps()?;

    let cargo_s = cargo.to_str().unwrap_or("cargo");
    let (cli_name, gui_name) = plataforma::bin_names();
    let (i_cli, i_gui) = plataforma::bin_names_interregno();
    let dir = plataforma::install_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("não consegui criar {}: {e}", dir.display()))?;

    purgar_fantasmas(&dir);

    // SQLite: se a distro tem a lib de desenvolvimento, LINKA a dela em vez de compilar ~250
    // mil linhas de C a cada build limpo. Mesma escolha que o install.sh faz.
    let feats: &[&str] = if plataforma::tem_sqlite_do_sistema() {
        println!("→ usando a libsqlite3 da distro (não compila o SQLite embutido).");
        &["--no-default-features", "--features", "sqlite-do-sistema"]
    } else {
        &[]
    };

    // CLI (schematize) — repo schematize-cli, crate na raiz.
    build_one(cargo_s, plataforma::APP_REPO, feats, None, &cli_name, &dir.join(&cli_name))?;
    limpa_interregno(&dir, &i_cli);

    // GUI (schematize-gui) — repo schematize_gui_slint. A GUI depende do crate `schematize`
    // como git-dep (branch=main); o `Cargo.lock` commitado FIXA um commit, e o build só
    // recompila — sem avançar o dep. Resultado: a GUI embutia uma versão VELHA
    // (`app_version()` = CARGO_PKG_VERSION do schematize no commit pinado) mesmo com o CLI já
    // novo. `cargo update -p schematize` avança o git-dep para o HEAD do main ANTES de
    // compilar → a versão embutida bate.
    build_one(
        cargo_s,
        plataforma::GUI_REPO,
        feats,
        Some("schematize"),
        &gui_name,
        &dir.join(&gui_name),
    )?;
    limpa_interregno(&dir, &i_gui);

    // Encerra qualquer GUI ANTIGA ainda aberta: só fechar a janela não bastava (o processo
    // velho seguia vivo e o relaunch reusava a versão anterior). Mata com os dois nomes — o
    // processo em execução pode ter subido pelo lançador do interregno.
    kill_stale_gui(&gui_name);
    kill_stale_gui(&i_gui);

    // A janela do gestor de atualizações — OPCIONAL: se o build falhar, o update NÃO falha (o
    // market e o app já estão instalados; é só chrome). Não depende do crate `schematize`,
    // então sem `refresh_dep`.
    let ugui = plataforma::gestor_gui_bin();
    if let Err(e) =
        build_one(cargo_s, plataforma::GESTOR_GUI_REPO, &[], None, &ugui, &dir.join(&ugui))
    {
        println!("aviso: build da janela do gestor falhou (opcional, seguindo): {e}");
    }

    limpa_targets_antigos();
    Ok(())
}

/// **O quê:** remove as cópias-fantasma antes de instalar, e RELATA o que resistiu.
///
/// **Onde:** [`build_from_source`].
///
/// Não é zelo: é o conserto do "atualizei e voltou para uma versão antiga". O binário podia
/// estar em quatro lugares e o PATH resolvia para o errado; instalar por cima de UM deles não
/// desfaz isso. O diretório de destino não é varrido — lá a troca é por rename, então um build
/// que falhe no meio não deixa a máquina sem app.
fn purgar_fantasmas(dir: &Path) {
    let manter: Vec<std::path::PathBuf> = std::env::current_exe().ok().into_iter().collect();
    let (removidos, resistiram) = super::purga::remove_copias_fantasma(dir, &manter);
    for p in &removidos {
        println!("→ removida instalação anterior: {}", p.display());
    }
    for p in &resistiram {
        // Erro nunca engolido (piso 4): o que resistiu vai para a tela com o que fazer, em vez
        // de a purga fingir que limpou e o app "voltar" para uma versão velha depois.
        println!("aviso: não consegui remover {} (permissão?).", p.display());
        println!("       Se o comando continuar abrindo uma versão velha, apague este arquivo:");
        println!("       sudo rm {}", p.display());
    }
}

/// **O quê:** sincroniza o checkout de `repo` e compila `binname`, copiando para `dst`.
///
/// **Onde:** [`build_from_source`] e [`super::atualizar_componente`].
///
/// `refresh_dep` avança um git-dep para o HEAD do branch ANTES de compilar. É best-effort: se
/// a rede cair, segue com o lock existente (offline ainda compila). Sem isto, a versão
/// embutida trava no commit que o `Cargo.lock` commitado pina.
pub fn build_one(
    cargo: &str,
    repo: &str,
    extra: &[&str],
    refresh_dep: Option<&str>,
    binname: &str,
    dst: &Path,
) -> Result<(), String> {
    let src = plataforma::build_src_dir(repo);
    sync_checkout(repo, &src)?;

    let manifest_s = src.join("Cargo.toml").to_string_lossy().to_string();

    if let Some(dep) = refresh_dep {
        println!("→ atualizando dep `{dep}` para o HEAD do main (evita versão embutida velha)…");
        let _ = util::run_inherit(cargo, &["update", "--manifest-path", &manifest_s, "-p", dep]);
    }

    println!("→ compilando {binname} (incremental — só o que mudou recompila)…");
    let mut args: Vec<String> =
        vec!["build".into(), "--release".into(), "--manifest-path".into(), manifest_s];
    for e in extra {
        args.push((*e).to_string());
    }
    let argsref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    // `target/` COMPARTILHADO: as ~226 dependências comuns aos repos compilam UMA vez, não uma
    // por repo (ver `plataforma::shared_target_dir`).
    let tgt = plataforma::shared_target_dir();
    std::fs::create_dir_all(&tgt)
        .map_err(|e| format!("não consegui criar {}: {e}", tgt.display()))?;
    let tgt_s = tgt.to_string_lossy().to_string();
    util::run_inherit_env(cargo, &argsref, &[("CARGO_TARGET_DIR", &tgt_s)])?;

    // Copia (NÃO move) o binário — mantém o artefato no target para o próximo build
    // incremental.
    let built = tgt.join("release").join(binname);
    if !built.is_file() {
        return Err(format!("o build não produziu {}", built.display()));
    }
    substitui_binario(&built, dst)
}

/// **O quê:** deixa o checkout `src` de `repo` na versão do `main`. Clona se não existir;
/// reclona se estiver corrompido. Shallow, para ser rápido e leve.
///
/// **Onde:** [`build_one`], sempre antes de compilar.
fn sync_checkout(repo: &str, src: &Path) -> Result<(), String> {
    let url = format!("https://github.com/{repo}");
    let src_s = src.to_string_lossy().to_string();
    if src.join(".git").is_dir() {
        let _ =
            util::run_inherit("git", &["-C", &src_s, "fetch", "--depth", "1", "origin", "main"]);
        if util::run_inherit("git", &["-C", &src_s, "reset", "--hard", "origin/main"]).is_ok() {
            return Ok(());
        }
        // Checkout corrompido → reclona do zero.
        let _ = std::fs::remove_dir_all(src);
    }
    if let Some(parent) = src.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("não consegui criar {}: {e}", parent.display()))?;
    }
    util::run_inherit("git", &["clone", "--depth", "1", &url, &src_s])
}

/// **O quê:** encerra processos da GUI já rodando, por NOME EXATO do binário.
///
/// **Onde:** [`build_from_source`], depois de trocar o executável.
///
/// O usuário reclamava de "atualizei mas abre a versão antiga" porque fechar a janela não
/// matava o processo. Best-effort e cross-OS; nunca falha o update. NÃO casa o market nem o
/// CLI — só o binário exato da GUI (`pkill -x`, não substring).
fn kill_stale_gui(gui_name: &str) {
    #[cfg(windows)]
    {
        let _ = util::capture("taskkill", &["/F", "/IM", &format!("{gui_name}.exe")]);
    }
    #[cfg(not(windows))]
    {
        let _ = util::capture("pkill", &["-x", gui_name]);
    }
}

/// **O quê:** remove os `target/` por-repo que existiam ANTES do target compartilhado.
///
/// **Onde:** o fim de [`build_from_source`] — nunca antes: se o build falhar, o cache antigo
/// continua lá.
///
/// Quem já tinha o app instalado carrega um `target/` dentro de cada checkout — dezenas de GB
/// de artefato que nenhum build volta a ler. Deixar isso para o usuário descobrir e apagar à
/// mão é o oposto do piso da casa: o software mudou o layout, o software limpa.
///
/// Só toca em caminhos que ESTE programa criou (`build_src_dir(repo)/target`) — nada de varrer
/// diretório por padrão. Falhar aqui não é erro: é só disco que sobrou.
fn limpa_targets_antigos() {
    let repos = [
        plataforma::APP_REPO,
        plataforma::GUI_REPO,
        plataforma::GESTOR_GUI_REPO,
        plataforma::DEPLOYER_REPO,
        plataforma::OPTIMIZER_REPO,
        plataforma::MARKET_REPO,
    ];
    let mut liberado: u64 = 0;
    for repo in repos {
        let antigo = plataforma::build_src_dir(repo).join("target");
        if !antigo.is_dir() {
            continue;
        }
        let tamanho = tamanho_de(&antigo);
        if std::fs::remove_dir_all(&antigo).is_ok() {
            liberado += tamanho;
        }
    }
    if liberado > 0 {
        println!(
            "→ liberados {} de `target/` antigo (agora há um só, compartilhado).",
            legivel(liberado)
        );
    }
}

/// **O quê:** soma o tamanho dos arquivos de uma árvore. **Onde:** [`limpa_targets_antigos`].
///
/// Best-effort: o que não der para ler conta 0 — é para imprimir "liberados X GB", não uma
/// contabilidade.
fn tamanho_de(dir: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    for e in rd.flatten() {
        match e.metadata() {
            Ok(m) if m.is_dir() => total += tamanho_de(&e.path()),
            Ok(m) => total += m.len(),
            Err(_) => {}
        }
    }
    total
}

/// **O quê:** bytes em unidade legível (uma casa decimal a partir de MB). Função PURA.
/// **Onde:** [`limpa_targets_antigos`].
fn legivel(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A limpeza de `target/` antigo cobre TODOS os repos que este app compila — incluindo os
    /// três que o updater nunca soube que existiam (deployer, optimizer, market). Um repo de
    /// fora da lista deixa alguns GB órfãos que ninguém volta a ler.
    #[test]
    fn a_limpeza_cobre_todo_repo_que_o_market_compila() {
        let na_limpeza = [
            plataforma::APP_REPO,
            plataforma::GUI_REPO,
            plataforma::GESTOR_GUI_REPO,
            plataforma::DEPLOYER_REPO,
            plataforma::OPTIMIZER_REPO,
            plataforma::MARKET_REPO,
        ];
        // A tabela de apps geridos é a fonte da verdade do que se compila; a limpeza tem de
        // conter todos os repos dela. Contagem, não amostra.
        for app in super::super::APPS_GERIDOS {
            assert!(
                na_limpeza.contains(&app.repo),
                "{} compila de {} e o `target/` dele nunca é limpo",
                app.bin,
                app.repo
            );
        }
    }

    /// A unidade legível não mente na fronteira — e MB/GB usam base 1024, como o resto do
    /// mundo de disco.
    #[test]
    fn tamanho_legivel_nas_fronteiras() {
        assert_eq!(legivel(0), "0 B");
        assert_eq!(legivel(1023), "1023 B");
        assert_eq!(legivel(1024 * 1024), "1.0 MB");
        assert_eq!(legivel(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(legivel(3 * 1024 * 1024 * 1024 + 512 * 1024 * 1024), "3.5 GB");
    }

    /// `tamanho_de` num diretório que não existe é 0, não pânico — a limpeza roda em máquina
    /// que nunca compilou nada.
    #[test]
    fn tamanho_de_diretorio_ausente_e_zero() {
        assert_eq!(tamanho_de(Path::new("/nao/existe/em/lugar/nenhum")), 0);
    }
}
