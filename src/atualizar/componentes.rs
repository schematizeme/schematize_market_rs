//! COMPONENTES — executar a atualização do app e de cada app gerido.
//!
//! **O quê:** [`atualizar_app`] (hub + GUI, binário ou fonte), [`atualizar_componente`] (um app
//! gerido, ou o self-update do market), e os passos de pós-instalação que só eles usam.
//!
//! **Onde:** [`super::install_or_update`], que decide O QUE fazer ([`super::planejar`]) e chama
//! estas funções para FAZER. Separado de `mod.rs` só por tamanho (teto de 750 linhas), sem
//! mudança de comportamento.

use super::metodo::{self, Veredito};
use super::{alvo, binario, exibir, fonte, ou_nenhum, selfupdate, versao, AppGerido, Politica};
use crate::nucleo::i18n::{t, tf};
use crate::nucleo::plataforma;

/// **O quê:** instala/atualiza o app (CLI + GUI) na versão `target`.
///
/// **Onde:** [`install_or_update`].
///
/// **Por que é uma função separada:** o caminho rápido faz `return` antes do build do fonte.
/// Enquanto a atualização dos outros apps morava lá dentro, ela **nunca rodava** quando havia
/// asset publicado — que passa a ser o caso normal depois de um release. Separar é o que
/// garante que os dois passos aconteçam, em qualquer caminho.
pub(super) fn atualizar_app(target: &str) -> Result<(), String> {
    // D7: só quem JÁ compila do fonte cai no fonte. Quem instala por binário e não tem asset
    // recebe a explicação — nunca um build implícito que pede `sudo` sem terminal.
    let metodo = alvo::metodo_de(plataforma::APP_REPO);

    // 1) CAMINHO RÁPIDO — binário pré-compilado, se houver asset para esta plataforma.
    if let Some((cli_asset, gui_asset)) = plataforma::asset_names() {
        match binario::try_binary(target, &cli_asset, &gui_asset) {
            Ok(true) => {
                metodo::registrar(plataforma::APP_REPO, alvo::Metodo::Binario);
                finish(target);
                return Ok(());
            }
            Ok(false) if metodo == alvo::Metodo::Binario => {
                return Err(alvo::mensagem_sem_binario(plataforma::APP_REPO, target));
            }
            Err(e) if metodo == alvo::Metodo::Binario => {
                return Err(format!(
                    "não consegui instalar o binário da v{target}: {e}\n  Nada foi alterado; \
                     o que está instalado continua funcionando."
                ));
            }
            Ok(false) => println!("→ sem binário compatível para v{target} — compilando do fonte…"),
            Err(e) => println!("→ via binário falhou ({e}) — compilando do fonte…"),
        }
    } else {
        println!("→ sem binário pré-compilado para esta plataforma/arch — compilando do fonte…");
    }

    // 2) CAMINHO CONFIÁVEL — compila do fonte (funciona em qualquer SO com toolchain). Chega
    // aqui só quem já era do fonte, ou quem não tem binário nenhum para a plataforma.
    fonte::build_from_source()?;
    metodo::registrar(plataforma::APP_REPO, alvo::Metodo::Fonte);
    finish(target);
    Ok(())
}

/// **O quê:** mantém um componente em dia. **Nunca devolve erro.**
///
/// **Onde:** [`install_or_update`], uma vez por item do plano.
///
/// **Por que nunca falha (piso 10):** cada app é uma entidade à parte. Derrubar o `update`
/// inteiro porque um app opcional não compilou é exatamente a cascata que o piso proíbe — e
/// o que a pessoa pediu (o app em dia) já aconteceu. O erro é DITO, com a causa, e o comando
/// segue.
///
/// **O market é o caso especial:** trocar o próprio binário em execução é o único modo de
/// falha capaz de deixar a máquina sem gestor nenhum, então ele vai por [`selfupdate`], que
/// verifica o candidato antes da troca.
pub(super) fn atualizar_componente(app: &AppGerido, force: bool) {
    let alvo_path = app.caminho();
    if app.politica == Politica::SeInstalado && !app.instalado() {
        return; // não instalado — e atualizar não é instalar.
    }
    let instalada = app.versao_instalada();
    let alvo = alvo::alvo_de(app.repo);
    let ultima = match &alvo {
        alvo::Alvo::Versao(v) | alvo::Alvo::SemAsset { versao: v } => Some(v.clone()),
        alvo::Alvo::Indisponivel(_) => None,
    };
    // Não saber o alvo NÃO some em silêncio: a causa vai para a tela (piso 4).
    if let (alvo::Alvo::Indisponivel(e), false) = (&alvo, force) {
        println!("aviso: {}: {e} — nada foi alterado.", app.bin);
        return;
    }

    // D8: binário instalado ACIMA do publicado é mantido e relatado; só o fonte (ou --force)
    // troca "diferente por diferente".
    let ver =
        metodo::veredito(alvo::metodo_de(app.repo), false, instalada.as_deref(), ultima.as_deref());
    if let (false, Veredito::AFrente, Some(pub_)) = (force, ver, ultima.as_deref()) {
        exibir::avisar_a_frente(app.bin, instalada.as_deref(), pub_);
        return;
    }
    if !force && ver != Veredito::Atualizar {
        let v = instalada.as_deref().unwrap_or("?");
        println!("{}", tf("up.comp_uptodate", &[("bin", app.bin), ("version", v)]));
        return;
    }
    println!(
        "{}",
        tf(
            "up.comp_target",
            &[
                ("bin", app.bin),
                ("target", ultima.as_deref().unwrap_or("?")),
                ("installed", &ou_nenhum(instalada.as_deref())),
            ]
        )
    );

    // O market vai por outro caminho: trocar o PRÓPRIO binário em execução é o único modo de
    // falha capaz de deixar a máquina sem gestor nenhum, e por isso passa pela rede do D3.
    //
    // O reconhecimento é pela POLÍTICA, não por comparação de nome. Casar string de binário é
    // exatamente o que deixou o update do deployer morto por um release (o nome mudou, a
    // comparação não), e a política já é invariante travada por teste: existe um e só um
    // componente `Sempre`, e ele é este programa.
    if app.politica == Politica::Sempre {
        if let alvo::Alvo::SemAsset { versao } = &alvo {
            println!("{}", alvo::mensagem_sem_binario(app.repo, versao));
            return;
        }
        let metodo = alvo::metodo_de(app.repo);
        if let Err(e) = selfupdate::atualizar_a_si_mesmo(ultima.as_deref(), metodo) {
            println!("{}", tf("up.self_failed", &[("error", &e)]));
        }
        return;
    }

    // Mesma resolução de toolchain do build do app. Sem ela não há o que compilar — e isso é
    // aviso, não erro: quem não tem cargo continua com o que já tinha.
    let cargo = match plataforma::ensure_toolchain() {
        Ok(c) => c,
        Err(e) => {
            println!("{}", tf("up.no_toolchain", &[("bin", app.bin), ("error", &e)]));
            return;
        }
    };
    let cargo_s = cargo.to_str().unwrap_or("cargo");
    match fonte::build_one(cargo_s, app.repo, &[], None, &app.bin_name(), &alvo_path) {
        Ok(()) => {
            if let Err(e) = conferir_versao_instalada(app, &alvo_path, ultima.as_deref()) {
                println!("{}", tf("up.comp_failed", &[("bin", app.bin), ("error", &e)]));
                return;
            }
            metodo::registrar(app.repo, alvo::Metodo::Fonte);
            aposentar_nome_legado(app);
            // **A JANELA vem junto, quando ela sai do mesmo repo (ADR-0020).**
            //
            // O `cargo build --release` acima já a produziu — os dois `[[bin]]` compilam
            // juntos —, mas o `build_one` copia UM binário pelo nome. Sem este segundo passo a
            // janela ficaria no `target/` compartilhado e nunca em `~/.cargo/bin`, e a aba
            // delegada do hub diria "não instalada" DEPOIS de a pessoa ter instalado.
            if let Some((nome, dst)) = app.gui_nome_e_destino() {
                let built = plataforma::shared_target_dir().join("release").join(&nome);
                match binario::substitui_binario(&built, &dst) {
                    Ok(()) => println!("✓ {nome}"),
                    // Não derruba o componente: o headless JÁ está instalado e funcionando, e
                    // dizer "falhou" sobre isso mandaria a pessoa reinstalar o que já tem. Mas
                    // também não se cala — a janela ausente tem de ter um motivo na tela.
                    Err(e) => println!("aviso: o binário está instalado, mas a janela não: {e}"),
                }
            }
            println!("✓ {}", tf("up.comp_done", &[("bin", app.bin)]));
        }
        Err(e) => println!("{}", tf("up.comp_failed", &[("bin", app.bin), ("error", &e)])),
    }
}

/// **O quê:** o binário que acabou de ser instalado reporta mesmo a versão que se pediu?
///
/// **Onde:** [`atualizar_componente`], entre o build e o "✓ atualizado".
///
/// **O caso real que esta conferência existe para pegar** — medido, não hipotético. O
/// `target/` é COMPARTILHADO e PERSISTENTE entre updates. Quando o Deployer trocou de nome de
/// binário (`deployer` → `schematize-deployer`, commit `0fa0112`), o artefato antigo continuou
/// no cache; o updater, que ainda pedia o nome velho, encontrava esse artefato **obsoleto**,
/// copiava-o para o `~/.cargo/bin` e imprimia "✓ deployer atualizado" sem ter atualizado nada.
/// A pessoa via a mensagem de sucesso e continuava com o binário que já tinha. (Na máquina onde
/// isto foi diagnosticado o artefato obsoleto `target/release/deployer` estava lá, ao lado dos
/// atuais; o deployer instalado estava em dia por coincidência — a renomeação e a última
/// publicação caíram no mesmo ciclo.)
///
/// Um build que "deu certo" e não mudou nada é pior que um build que falhou: o erro visível
/// manda investigar, o sucesso falso manda embora.
///
/// **Sem saber a última versão (rede fora), não há o que conferir** — e aí a ausência de
/// conferência não é omissão, é a única resposta honesta.
fn conferir_versao_instalada(
    app: &AppGerido,
    caminho: &std::path::Path,
    esperada: Option<&str>,
) -> Result<(), String> {
    let Some(esperada) = esperada else { return Ok(()) };
    if binario::executa_e_reporta(caminho, esperada) {
        return Ok(());
    }
    let obtida = versao::installed_version_of(caminho);
    Err(format!(
        "o build terminou mas {} responde {} em vez de v{esperada} — provavelmente um artefato \
         obsoleto no cache de build. Rode `schematize-market update --force`, e se persistir \
         apague {}",
        app.bin,
        obtida.map(|v| format!("v{v}")).unwrap_or_else(|| "nada".into()),
        plataforma::build_src_dir(app.repo).display()
    ))
}

/// **O quê:** apaga o binário com o nome ANTIGO, depois de o novo estar no lugar. Diz o que fez.
///
/// **Onde:** [`atualizar_componente`], só no caminho de sucesso.
///
/// **Por que só depois do sucesso:** apagar antes abriria uma janela em que a pessoa não tem
/// nem o antigo nem o novo — e um build de vinte minutos que falhe no fim a deixaria sem o app.
///
/// **Por que apagar:** deixar os dois é criar exatamente o fantasma que a [`super::purga`]
/// existe para matar. O `deployer` velho continuaria no PATH, respondendo `--version` com a
/// versão de antes da renomeação, e a pessoa veria duas verdades sobre o mesmo programa.
///
/// **Nada some em silêncio:** a remoção é impressa com o caminho. Um gestor que apaga arquivo
/// sem dizer é um gestor de que se desconfia.
fn aposentar_nome_legado(app: &AppGerido) {
    let Some(velho) = app.caminho_legado() else { return };
    if !velho.is_file() {
        return;
    }
    match std::fs::remove_file(&velho) {
        Ok(()) => println!(
            "→ removido o binário com o nome antigo: {} (agora é {})",
            velho.display(),
            app.bin
        ),
        // Erro nunca engolido (piso 4): se resistiu, a pessoa precisa saber — senão o PATH
        // pode continuar resolvendo para ele e o "atualizei e não mudou nada" volta.
        Err(e) => {
            println!("aviso: não consegui remover {} ({e}).", velho.display());
            println!("       Apague-o à mão, senão o PATH pode continuar achando a versão velha.");
        }
    }
}

/// **O quê:** pós-instalação — PATH, lançador e relatório honesto da versão que ficou.
/// **Onde:** o fim de [`atualizar_app`], nos dois caminhos.
fn finish(target: &str) {
    plataforma::ensure_path_setup();
    plataforma::make_launcher();
    let got = versao::installed_app_version().unwrap_or_else(|| target.to_string());
    let dir = plataforma::install_dir().display().to_string();
    println!("✓ {}", tf("up.installed_at", &[("version", &got), ("dir", &dir)]));
    println!("  {}", t("up.reopen"));
}
