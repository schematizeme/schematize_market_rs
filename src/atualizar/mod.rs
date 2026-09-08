//! ATUALIZAR — o market como dono de instalar e manter em dia todo o ecossistema.
//!
//! **O quê:** decide o que este `update` faz nesta máquina ([`planejar`]) e executa —
//! caminho rápido (binário pré-compilado) com queda para o fonte, mais um passo por app
//! gerido que já esteja instalado.
//!
//! **Onde:** os comandos `schematize-market update | install | status | pin | unpin`.
//!
//! ## Procedência (ADR-0013)
//!
//! Este módulo é o `install.rs` do `schematize_updater_rs` movido para cá sob o D4 —
//! movimento, não reescrita. Pin, purga, detecção de plataforma, nomes de asset e a ordem
//! "binário pronto → fonte" chegam com o comportamento que tinham.
//!
//! ## As duas mudanças que NÃO são movimento, e por que elas são o ponto
//!
//! **1. O plano deixa de ser sobre um app e passa a ser sobre uma TABELA.** O updater tinha
//! `deve_reconstruir_deployer` escrito à mão e não sabia do optimizer nem do market — dois
//! apps publicados que nenhum caminho de atualização alcançava. Aqui há [`APPS_GERIDOS`], e a
//! contagem é o teste.
//!
//! **2. O market se atualiza.** O updater atualizava o app **de fora** e nunca precisou trocar
//! o próprio binário em execução. O market precisa, e por isso o self-update mora em módulo
//! próprio, com verificação antes da troca — ver [`selfupdate`].

pub mod binario;
pub mod fonte;
pub mod purga;
pub mod selfupdate;
pub mod versao;

use crate::nucleo::i18n::{t, tf};
use crate::nucleo::plataforma;
use std::path::PathBuf;

/// **O quê:** quando um componente entra num `update`.
///
/// **Onde:** [`AppGerido::politica`] e [`planejar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Politica {
    /// Só se o binário **já estiver** na máquina.
    ///
    /// **A regra:** atualizar não instala app que ninguém pediu. Quem roda `update` quer o que
    /// já tem, mais novo — não software novo aparecendo no `~/.cargo/bin` porque a casa lançou
    /// outro produto. Um gestor que faz isso vira um gestor de que se desconfia, e a
    /// desconfiança contamina justamente as atualizações que importam.
    SeInstalado,
    /// **Sempre.** Reservado ao próprio market: é o único componente que ninguém mais
    /// atualiza. Se ele ficar parado na última versão publicada, qualquer correção nele —
    /// inclusive nas de atualizar — nunca chega em máquina nenhuma. Era exatamente a situação
    /// do updater antes de ele aprender a se reconstruir.
    Sempre,
}

/// **O quê:** um componente que o market mantém em dia.
///
/// **Onde:** [`APPS_GERIDOS`].
///
/// **Por que uma tabela e não um módulo por app:** a lógica de descobrir, versionar e
/// reconstruir é idêntica; duplicá-la por app é a divergência esperando acontecer — e já
/// aconteceu: o `deve_reconstruir_deployer` do updater procurava um binário chamado
/// `deployer`, nome que o ADR-0012 aposentou em favor de `schematize-deployer`. A função
/// continuou compilando, retornando `false` para sempre, e **o deployer deixou de ser
/// atualizado em toda máquina, em silêncio**. O que muda entre os apps é o nome e o repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppGerido {
    /// Nome do binário, SEM sufixo de plataforma (o `.exe` entra em [`AppGerido::bin_name`]).
    pub bin: &'static str,
    /// Repositório de onde ele é compilado, e de onde se lê a versão publicada.
    pub repo: &'static str,
    /// Uma linha sobre o que ele faz — vai para o `status` e o `--dry-run`.
    pub sobre: &'static str,
    /// Quando ele entra num `update`.
    pub politica: Politica,
    /// O nome que este binário TEVE antes, se ele foi renomeado.
    ///
    /// **Por que isto existe:** o Deployer se chamou `deployer` até o commit `0fa0112`. Numa
    /// máquina que instalou antes disso, é esse o arquivo que está em `~/.cargo/bin` — e um
    /// gestor que só conhece o nome novo diz "não instalado" sobre um app que está lá,
    /// deixando a pessoa parada na versão antiga para sempre. Reconhecer o nome velho é o que
    /// permite ATUALIZAR quem está nesse estado, e só então apagar o arquivo antigo.
    pub legado: Option<&'static str>,
}

impl AppGerido {
    /// **O quê:** o nome do binário nesta plataforma (com `.exe` no Windows).
    /// **Onde:** [`estado_dos_apps`] e o plano.
    pub fn bin_name(&self) -> String {
        format!("{}{}", self.bin, plataforma::exe_suffix())
    }

    /// **O quê:** o caminho onde este componente é instalado. **Onde:** idem.
    pub fn caminho(&self) -> PathBuf {
        plataforma::install_dir().join(self.bin_name())
    }

    /// **O quê:** o caminho do binário com o nome ANTIGO, se este app foi renomeado.
    /// **Onde:** [`AppGerido::instalado`] e [`aposentar_nome_legado`].
    pub fn caminho_legado(&self) -> Option<PathBuf> {
        self.legado
            .map(|l| plataforma::install_dir().join(format!("{l}{}", plataforma::exe_suffix())))
    }

    /// **O quê:** este app está na máquina, com o nome novo OU com o antigo?
    ///
    /// **Onde:** [`estado_dos_apps`].
    ///
    /// **Por que os dois contam:** quem instalou antes da renomeação tem só o nome velho. Dizer
    /// "não instalado" a essa pessoa a deixa presa na versão que tem, sem nunca saber por quê.
    pub fn instalado(&self) -> bool {
        self.caminho().is_file() || self.caminho_legado().map(|p| p.is_file()).unwrap_or(false)
    }

    /// **O quê:** a versão que está de fato rodando nesta máquina — do binário novo se houver,
    /// senão do antigo. **Onde:** o plano, o `status` e [`atualizar_componente`].
    pub fn versao_instalada(&self) -> Option<String> {
        // O market é o binário que está EXECUTANDO este código. Perguntar ao disco por
        // `~/.cargo/bin/schematize-market` responderia "nenhum" quando ele foi invocado de
        // outro lugar — um `status` que não sabe a própria versão é um `status` que ninguém
        // acredita. A versão compilada aqui é a resposta certa, e é a única sem rede.
        if self.politica == Politica::Sempre {
            return Some(env!("CARGO_PKG_VERSION").to_string());
        }
        let novo = self.caminho();
        if novo.is_file() {
            return versao::installed_version_of(&novo);
        }
        versao::installed_version_of(&self.caminho_legado()?)
    }
}

/// Os componentes que o market mantém em dia.
///
/// **O hub (`schematize`) e a GUI não estão aqui**, e é deliberado: os dois saem do MESMO
/// repo e do MESMO release, são tratados como um par pela versão-alvo ([`versao::target_version`])
/// e têm caminho rápido de binário pré-compilado que nenhum outro tem. Misturá-los na tabela
/// obrigaria toda entrada a carregar campos que só eles usam.
pub const APPS_GERIDOS: &[AppGerido] = &[
    AppGerido {
        bin: "schematize-deployer",
        repo: plataforma::DEPLOYER_REPO,
        sobre: "SSH, VPS, DNS e cofre — opera servidor com a credencial fora do agente",
        politica: Politica::SeInstalado,
        // Chamou-se `deployer` até o commit `0fa0112`; o ADR-0012 pôs o prefixo da casa.
        legado: Some("deployer"),
    },
    AppGerido {
        bin: "schematize-optimizer",
        repo: plataforma::OPTIMIZER_REPO,
        sobre: "mede o ambiente de dev e põe cada software no seu teto de recurso",
        politica: Politica::SeInstalado,
        legado: None,
    },
    AppGerido {
        bin: "schematize-market",
        repo: plataforma::MARKET_REPO,
        sobre: "instala e atualiza tudo do ecossistema — este programa",
        politica: Politica::Sempre,
        legado: None,
    },
];

/// **O quê:** o que um `update` precisa fazer nesta máquina.
///
/// **Onde:** [`install_or_update`], como primeira coisa, e o `--dry-run`.
///
/// **Por que isto é uma struct e não um `if` no meio do fluxo:** o defeito original foi
/// exatamente de FLUXO — um `return Ok(())` antecipado quando o app estava em dia tornava a
/// atualização dos outros apps inalcançável. Fluxo enterrado dentro de uma função com rede,
/// clone e `cargo` não é testável, e por isso o defeito não foi pego por teste nenhum. Como
/// struct, a decisão é uma função pura e há teste sobre ela.
#[derive(Debug, PartialEq, Eq)]
pub struct Plano {
    /// Atualizar o app (o par CLI + GUI)?
    pub app: bool,
    /// Os componentes a verificar. **Verificar ≠ atualizar:** a versão de cada um decide
    /// depois, em [`atualizar_componente`].
    pub componentes: Vec<&'static AppGerido>,
}

impl Plano {
    /// **O quê:** `true` quando não há nada a fazer. **Onde:** o `--dry-run` e a mensagem final.
    pub fn vazio(&self) -> bool {
        !self.app && self.componentes.is_empty()
    }
}

/// **O quê:** decide o que este `update` faz, a partir do estado da máquina. Função PURA.
///
/// **Onde:** [`install_or_update`] e o `--dry-run`.
///
/// **A regra que mais importa:** os componentes entram **independentemente** de o app estar em
/// dia. São apps distintos, com versões e ciclos próprios; amarrar um ao outro é o defeito que
/// esta função existe para impedir.
///
/// `instalados` é o conjunto de binários presentes — vem de [`estado_dos_apps`], que toca o
/// disco. A separação é o que torna a decisão testável sem uma máquina de verdade.
pub fn planejar(
    force: bool,
    instalada: Option<&str>,
    alvo: &str,
    instalados: &[&'static AppGerido],
) -> Plano {
    Plano {
        app: force || instalada != Some(alvo),
        componentes: APPS_GERIDOS
            .iter()
            .filter(|a| match a.politica {
                Politica::Sempre => true,
                Politica::SeInstalado => instalados.iter().any(|i| i.bin == a.bin),
            })
            .collect(),
    }
}

/// **O quê:** quais apps geridos estão de fato instalados nesta máquina. Toca o disco.
/// **Onde:** [`install_or_update`], para alimentar [`planejar`].
pub fn estado_dos_apps() -> Vec<&'static AppGerido> {
    APPS_GERIDOS.iter().filter(|a| a.instalado()).collect()
}

/// **O quê:** instala (1ª vez) ou atualiza tudo. `force` refaz mesmo já estando na versão-alvo.
///
/// **Onde:** os comandos `update` e `install`.
///
/// **`dry_run` não toca em nada:** imprime o plano e sai. É o que permite alguém ver o que vai
/// acontecer numa máquina de produção antes de deixar acontecer.
pub fn install_or_update(force: bool, dry_run: bool) -> Result<(), String> {
    let target = versao::target_version().ok_or_else(|| t("up.no_target"))?;
    let installed = versao::installed_app_version();
    let plano = planejar(force, installed.as_deref(), &target, &estado_dos_apps());

    if dry_run {
        imprimir_plano(&plano, installed.as_deref(), &target);
        return Ok(());
    }

    // A falha do app é GUARDADA, não propagada aqui — e a razão é o que este overdev inteiro
    // é sobre.
    //
    // Com um `?` nesta linha, um app que não compila (sem toolchain, sem lib de build, rede
    // fora no meio do clone) cortaria a função ANTES dos componentes — inclusive antes do
    // self-update do market. E o self-update é justamente quem traz a correção para esse tipo
    // de falha: cortá-lo ali cria a armadilha em que a máquina não consegue se atualizar
    // porque não conseguiu se atualizar. É a versão nova do `return Ok(())` antecipado que o
    // `planejar()` existe para não repetir.
    //
    // O erro NÃO é engolido (piso 4): ele volta ao chamador no fim, e o comando sai != 0.
    let falha_do_app = if plano.app {
        println!(
            "{}",
            tf(
                "up.app_target",
                &[("target", &target), ("installed", &ou_nenhum(installed.as_deref()))]
            )
        );
        atualizar_app(&target).err()
    } else {
        println!("{}", tf("up.app_uptodate", &[("target", &target)]));
        None
    };

    // Cada componente é INDEPENDENTE (piso 10): a falha de um nunca derruba os outros nem o
    // comando. O erro é DITO — nunca engolido —, e o `update` segue.
    for app in &plano.componentes {
        atualizar_componente(app, force);
    }

    aposentar_o_updater();

    match falha_do_app {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// **O quê:** remove o `schematize-updater` da máquina, e DIZ que fez isso.
///
/// **Onde:** o fim de [`install_or_update`], depois de o market ter feito o trabalho dele.
///
/// **Por que remover (ADR-0013):** o updater não fica só ocupando espaço — ele **responde**.
/// Um `schematize-updater update` numa máquina onde o market já assumiu executa um gestor
/// congelado, que reinstala pelo caminho antigo e desfaz o que o novo acabou de fazer. Dois
/// programas dizendo coisas diferentes sobre o mesmo sistema é o fantasma que a
/// [`super::purga`] existe para matar, e aqui ele tem nome.
///
/// **Por que só DEPOIS do update:** se o market falhar no meio, a máquina ainda tem um gestor
/// que funciona. Apagar antes trocaria uma degradação por ficar sem nada — que é exatamente o
/// modo de falha que este overdev inteiro trabalhou para evitar.
///
/// **Por que NADA some em silêncio:** a remoção é impressa com o caminho e com o motivo. Um
/// gestor que apaga binário da máquina de alguém sem dizer é um gestor de que se desconfia — e
/// a desconfiança contamina justamente as atualizações que importam.
///
/// **Não devolve erro** (piso 10): se o arquivo resistir (permissão em `/usr/bin`), o `update`
/// já terminou o que a pessoa pediu. O que resistiu vai para a tela com o comando exato.
fn aposentar_o_updater() {
    let nome = plataforma::updater_bin_name();
    for dir in [plataforma::install_dir(), crate::nucleo::util::home().join(".local/bin")] {
        let alvo = dir.join(&nome);
        if !alvo.is_file() {
            continue;
        }
        match std::fs::remove_file(&alvo) {
            Ok(()) => {
                println!("→ removido o gestor ANTIGO: {}", alvo.display());
                println!("  ({nome} foi absorvido por este programa — ADR-0013.)");
            }
            Err(e) => {
                println!("aviso: não consegui remover o gestor antigo {} ({e}).", alvo.display());
                println!("       Ele ainda responde `update` e desfaria o que acabei de fazer.");
                println!("       Apague-o: rm {}", alvo.display());
            }
        }
    }
}

/// **O quê:** imprime o que aconteceria, sem mexer em nada. **Onde:** [`install_or_update`]
/// com `--dry-run`.
fn imprimir_plano(plano: &Plano, instalada: Option<&str>, alvo: &str) {
    println!("{}\n", t("up.plan_title"));
    println!(
        "  {:<24} {}",
        t("up.plan_app"),
        linha_do_plano(&versionado(Some(alvo)), instalada, plano.app)
    );
    for app in &plano.componentes {
        let inst = app.versao_instalada();
        let ult = versao::latest_version_of(app.repo);
        let muda = versao::desatualizado(inst.as_deref(), ult.as_deref());
        println!(
            "  {:<24} {}",
            app.bin,
            linha_do_plano(&versionado(ult.as_deref()), inst.as_deref(), muda)
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

/// **O quê:** uma versão para exibição — `v0.57.0`, ou "? (rede)" quando não se sabe.
/// **Onde:** [`imprimir_plano`].
fn versionado(v: Option<&str>) -> String {
    v.map(|v| format!("v{v}")).unwrap_or_else(|| t("up.net_unknown"))
}

/// **O quê:** o "alvo X, instalado Y -> ação" de uma linha do plano.
/// **Onde:** [`imprimir_plano`], para o app e para cada componente.
fn linha_do_plano(alvo: &str, instalada: Option<&str>, muda: bool) -> String {
    let acao = if muda { t("up.act_update") } else { t("up.act_uptodate") };
    tf("up.plan_line", &[("target", alvo), ("installed", &ou_nenhum(instalada)), ("action", &acao)])
}

/// **O quê:** a versão instalada, ou a palavra traduzida para "nenhum".
/// **Onde:** toda mensagem que reporta o que há na máquina.
fn ou_nenhum(v: Option<&str>) -> String {
    v.map(String::from).unwrap_or_else(|| t("up.none"))
}

/// **O quê:** instala/atualiza o app (CLI + GUI) na versão `target`.
///
/// **Onde:** [`install_or_update`].
///
/// **Por que é uma função separada:** o caminho rápido faz `return` antes do build do fonte.
/// Enquanto a atualização dos outros apps morava lá dentro, ela **nunca rodava** quando havia
/// asset publicado — que passa a ser o caso normal depois de um release. Separar é o que
/// garante que os dois passos aconteçam, em qualquer caminho.
fn atualizar_app(target: &str) -> Result<(), String> {
    // 1) CAMINHO RÁPIDO — binário pré-compilado, se houver asset para esta plataforma.
    if let Some((cli_asset, gui_asset)) = plataforma::asset_names() {
        match binario::try_binary(target, &cli_asset, &gui_asset) {
            Ok(true) => {
                finish(target);
                return Ok(());
            }
            Ok(false) => println!("→ sem binário compatível para v{target} — compilando do fonte…"),
            Err(e) => println!("→ via binário falhou ({e}) — compilando do fonte…"),
        }
    } else {
        println!("→ sem binário pré-compilado para esta plataforma/arch — compilando do fonte…");
    }

    // 2) CAMINHO CONFIÁVEL — compila do fonte (funciona em qualquer SO com toolchain).
    fonte::build_from_source()?;
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
fn atualizar_componente(app: &AppGerido, force: bool) {
    let alvo_path = app.caminho();
    if app.politica == Politica::SeInstalado && !app.instalado() {
        return; // não instalado — e atualizar não é instalar.
    }
    let instalada = app.versao_instalada();
    let ultima = versao::latest_version_of(app.repo);

    if !force && !versao::desatualizado(instalada.as_deref(), ultima.as_deref()) {
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
        if let Err(e) = selfupdate::atualizar_a_si_mesmo(ultima.as_deref()) {
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
            aposentar_nome_legado(app);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn app(bin: &str) -> &'static AppGerido {
        APPS_GERIDOS.iter().find(|a| a.bin == bin).expect("app na tabela")
    }

    /// **O DEFEITO ORIGINAL, travado por teste:** com o app em dia, o `update` retornava cedo e
    /// os outros apps nunca eram verificados.
    #[test]
    fn app_em_dia_nao_impede_a_checagem_dos_componentes() {
        let p = planejar(false, Some("0.57.0"), "0.57.0", &[app("schematize-deployer")]);
        assert!(!p.app, "o app está em dia — não há o que fazer nele");
        assert!(
            p.componentes.iter().any(|c| c.bin == "schematize-deployer"),
            "o deployer TEM de ser verificado mesmo assim: é outro app"
        );
    }

    /// **FA · o item que o updater não cumpria:** com o OPTIMIZER instalado e desatualizado, ele
    /// entra no plano. O `deve_reconstruir_deployer` hard-coded do updater não sabia que o
    /// optimizer existia — nenhum `update` jamais o tocou.
    #[test]
    fn optimizer_instalado_entra_no_plano() {
        let p = planejar(false, Some("0.57.0"), "0.57.0", &[app("schematize-optimizer")]);
        assert!(
            p.componentes.iter().any(|c| c.bin == "schematize-optimizer"),
            "o optimizer ficou de fora — é o buraco que o updater tinha"
        );
    }

    /// E o inverso: sem o app instalado, ele não entra no plano nem com o hub desatualizado.
    /// Atualizar não instala.
    #[test]
    fn sem_o_app_instalado_ele_nao_entra_no_plano() {
        let p = planejar(false, Some("0.55.0"), "0.57.0", &[]);
        assert!(p.app);
        assert!(
            !p.componentes.iter().any(|c| c.politica == Politica::SeInstalado),
            "atualizar o app não pode instalar um app que ninguém pediu"
        );
    }

    /// **O market entra SEMPRE** — é o único que ninguém mais atualiza. Sem esta regra, a
    /// primeira correção no próprio gestor nunca chegaria em máquina nenhuma.
    #[test]
    fn o_market_entra_no_plano_mesmo_sem_nada_instalado() {
        let p = planejar(false, Some("0.57.0"), "0.57.0", &[]);
        assert!(
            p.componentes.iter().any(|c| c.bin == "schematize-market"),
            "o market tem de se manter em dia sozinho"
        );
        assert!(!p.vazio(), "há sempre pelo menos o próprio market a verificar");
    }

    /// `--force` refaz o app; a regra dos componentes não muda (ela é sobre estar instalado,
    /// não sobre a versão).
    #[test]
    fn force_refaz_o_app_e_nao_muda_a_regra_dos_componentes() {
        let sem = planejar(true, Some("0.57.0"), "0.57.0", &[]);
        assert!(sem.app);
        assert!(!sem.componentes.iter().any(|c| c.bin == "schematize-deployer"));

        let com = planejar(true, Some("0.57.0"), "0.57.0", &[app("schematize-deployer")]);
        assert!(com.componentes.iter().any(|c| c.bin == "schematize-deployer"));
    }

    /// **O BUG QUE O PORTE ACHOU.** O `platform::deployer_bin()` do updater devolvia
    /// `"deployer"`, mas o binário foi renomeado para `schematize-deployer` no commit `0fa0112`
    /// do repo do Deployer. `deve_reconstruir_deployer` procurava `~/.cargo/bin/deployer`, que
    /// nunca existiu depois disso — então o Deployer **deixou de ser atualizado em toda
    /// máquina, em silêncio**, e nenhum teste pegou porque todos passavam o nome à mão.
    ///
    /// Aqui a asserção é sobre a tabela: o nome de cada app é o `[[bin]]` do Cargo.toml dele.
    /// Se alguém renomear um binário de novo, esta lista é o que reprova.
    #[test]
    fn o_nome_do_binario_e_o_que_o_cargo_toml_publica() {
        let esperado = [
            ("schematize-deployer", "schematizeme/schematize_deployer_rs"),
            ("schematize-optimizer", "schematizeme/schematize_optimizer_rs"),
            ("schematize-market", "schematizeme/schematize_market_rs"),
        ];
        assert_eq!(APPS_GERIDOS.len(), esperado.len(), "app novo sem entrada nesta asserção");
        for (bin, repo) in esperado {
            let a = app(bin);
            assert_eq!(a.repo, repo, "{bin} aponta para o repo errado");
            // O prefixo `schematize-` é o que o ADR-0012 estabeleceu para o `schematize-<TAB>`
            // no terminal listar a casa inteira. Um binário sem ele é um binário que ninguém
            // acha — e, como se viu, que ninguém atualiza.
            assert!(a.bin.starts_with("schematize-"), "{bin} fora do padrão do ADR-0012");
        }
    }

    /// **O BUG MEDIDO, lado (b):** o nome do binário na tabela é o `[[bin]]` do Cargo.toml de
    /// cada repo. O `platform::deployer_bin()` do updater devolvia `"deployer"`, nome que o
    /// commit `0fa0112` aposentou — numa máquina instalada DEPOIS da renomeação, o deployer
    /// nunca entrava no plano.
    #[test]
    fn o_nome_do_binario_e_o_que_o_cargo_toml_publica_hoje() {
        assert_eq!(app("schematize-deployer").bin_name(), "schematize-deployer");
        assert_ne!(
            app("schematize-deployer").bin,
            "deployer",
            "é o nome ANTIGO — voltar a ele é repetir o bug de 0fa0112"
        );
    }

    /// **O BUG MEDIDO, lado (a):** numa máquina que instalou ANTES da renomeação, o arquivo em
    /// `~/.cargo/bin` chama-se `deployer`. O nome antigo tem de ser reconhecido — senão o
    /// gestor diz "não instalado" sobre um app que está lá, e a pessoa fica parada na versão
    /// que tem (medido nesta máquina: `deployer 0.5.0`) sem nunca saber por quê.
    #[test]
    fn o_nome_legado_do_deployer_e_reconhecido() {
        let dep = app("schematize-deployer");
        assert_eq!(dep.legado, Some("deployer"));
        let legado = dep.caminho_legado().expect("o deployer tem nome legado");
        assert!(legado.ends_with(format!("deployer{}", plataforma::exe_suffix())));
        assert_eq!(legado.parent(), dep.caminho().parent(), "os dois no mesmo diretório");
        assert_ne!(legado, dep.caminho(), "legado e atual são arquivos distintos");
    }

    /// Só o deployer tem nome legado — inventar um para os outros criaria uma busca por
    /// arquivo que nunca existiu, e um caminho que nenhum teste exercita de verdade.
    #[test]
    fn so_o_deployer_foi_renomeado() {
        let com_legado: Vec<&str> =
            APPS_GERIDOS.iter().filter(|a| a.legado.is_some()).map(|a| a.bin).collect();
        assert_eq!(com_legado, vec!["schematize-deployer"]);
        assert!(app("schematize-optimizer").caminho_legado().is_none());
        assert!(app("schematize-market").caminho_legado().is_none());
    }

    /// Todo app gerido tem repo, descrição e um bin sem sufixo de plataforma embutido —
    /// o `.exe` entra em [`AppGerido::bin_name`], nunca na tabela.
    #[test]
    fn a_tabela_esta_bem_formada() {
        for a in APPS_GERIDOS {
            assert!(!a.repo.is_empty() && a.repo.contains('/'), "{}: repo inválido", a.bin);
            assert!(!a.sobre.is_empty(), "{}: sem descrição — o status fica mudo", a.bin);
            assert!(!a.bin.ends_with(".exe"), "{}: sufixo não entra na tabela", a.bin);
            assert!(a.caminho().ends_with(a.bin_name()));
        }
        // Exatamente UM componente com política `Sempre`: é o próprio market. Dois seriam dois
        // programas se atualizando sozinhos, e um deles não é o que está rodando.
        assert_eq!(
            APPS_GERIDOS.iter().filter(|a| a.politica == Politica::Sempre).count(),
            1,
            "só o market se atualiza sempre"
        );
    }

    /// **O corte, do lado do market (ADR-0013).** O nome do antecessor é conhecido, está na
    /// purga, e a remoção acontece DEPOIS do update — nunca antes.
    ///
    /// Testar `aposentar_o_updater` de verdade exigiria escrever em `~/.cargo/bin`, que é a
    /// máquina de quem roda a suíte; o que se afirma aqui é o que dá para afirmar sem isso: o
    /// nome existe, bate com o da purga, e não colide com nenhum app gerido.
    #[test]
    fn o_antecessor_tem_nome_conhecido_e_nao_colide_com_app_vivo() {
        let velho = plataforma::updater_bin_name();
        assert!(velho.starts_with("schematize-updater"));
        assert!(
            !APPS_GERIDOS.iter().any(|a| a.bin_name() == velho),
            "o updater não pode estar entre os apps MANTIDOS — ele é removido, não atualizado"
        );
        // E a ordem: a remoção é a última coisa de `install_or_update`, depois dos
        // componentes. Apagar antes deixaria a máquina sem gestor se o update falhasse.
        let fonte = include_str!("mod.rs");
        let corpo = fonte.split("pub fn install_or_update").nth(1).unwrap();
        let corpo = corpo.split("\n}\n").next().unwrap();
        let i_comp = corpo.find("atualizar_componente(app, force)").expect("o laço");
        let i_apos = corpo.find("aposentar_o_updater()").expect("a aposentadoria");
        assert!(i_apos > i_comp, "o antecessor sai DEPOIS do update, nunca antes");
    }

    /// **O market é reconhecido pela POLÍTICA, não pelo nome do binário.**
    ///
    /// Casar string de nome de binário é o que deixou o update do deployer morto por um
    /// release inteiro: o nome mudou e a comparação não. `atualizar_componente` desvia para o
    /// self-update por `politica == Sempre`, e este teste é o que garante que essa condição
    /// identifica exatamente um componente — este programa.
    #[test]
    fn o_desvio_para_o_self_update_e_por_politica_e_pega_so_o_market() {
        let sempre: Vec<&str> =
            APPS_GERIDOS.iter().filter(|a| a.politica == Politica::Sempre).map(|a| a.bin).collect();
        assert_eq!(sempre, vec!["schematize-market"], "só o próprio market se autoatualiza");

        // E o código de fato desvia pela política — não por comparação de nome.
        let fonte = include_str!("mod.rs");
        let corpo = fonte.split("fn atualizar_componente").nth(1).unwrap();
        let corpo = corpo.split("\n}\n").next().unwrap();
        assert!(
            corpo.contains("app.politica == Politica::Sempre"),
            "o desvio voltou a ser por nome — é o bug do `deployer` esperando acontecer"
        );
        assert!(
            !corpo.contains("market_bin_name()"),
            "comparação por nome de binário no desvio do self-update"
        );
    }

    /// **A falha do app NÃO pode cortar o self-update.**
    ///
    /// Com um `?` na chamada de `atualizar_app`, um app que não compila (sem toolchain, sem
    /// lib de build, rede fora no meio do clone) encerraria a função antes dos componentes —
    /// inclusive antes do self-update do market, que é quem traz a correção para esse tipo de
    /// falha. A máquina ficaria presa: não consegue se atualizar porque não conseguiu se
    /// atualizar. É a versão nova do `return Ok(())` antecipado que o `planejar()` existe para
    /// não repetir, e por isso vive travado por teste no mesmo lugar.
    ///
    /// O erro segue voltando ao chamador no fim — o comando sai != 0. Guardar não é engolir.
    #[test]
    fn a_falha_do_app_nao_corta_os_componentes() {
        let fonte = include_str!("mod.rs");
        let corpo = fonte.split("pub fn install_or_update").nth(1).unwrap();
        let corpo = corpo.split("\n}\n").next().unwrap();

        assert!(
            !corpo.contains("atualizar_app(&target)?"),
            "o `?` voltou: uma falha do app cortaria o self-update, e a máquina ficaria presa"
        );
        assert!(corpo.contains("atualizar_app(&target).err()"), "a falha tem de ser GUARDADA");

        // A ordem: guardar a falha → componentes → aposentar o antecessor → devolver o erro.
        let i_falha = corpo.find("let falha_do_app").expect("a falha guardada");
        let i_comp = corpo.find("atualizar_componente(app, force)").expect("o laço");
        let i_ret = corpo.find("Some(e) => Err(e)").expect("o erro devolvido no fim");
        assert!(i_falha < i_comp, "guarda a falha antes de rodar os componentes");
        assert!(i_comp < i_ret, "os componentes rodam ANTES de o erro subir");
    }

    /// **A invariante entre as duas tabelas:** todo app que o market sabe INSTALAR sob pedido
    /// (`appsdacasa::EXTERNOS`) é um app que ele sabe MANTER EM DIA (`APPS_GERIDOS`).
    ///
    /// Sem esta asserção, as duas listas divergem — e a divergência é silenciosa do pior jeito:
    /// o `market install schematize-optimizer` instala, e o `market update` nunca mais toca no
    /// que instalou. É literalmente o problema que este overdev existe para acabar.
    #[test]
    fn tudo_que_o_market_instala_ele_tambem_atualiza() {
        for e in crate::appsdacasa::EXTERNOS {
            assert!(
                APPS_GERIDOS.iter().any(|a| a.bin == e.bin),
                "{} é instalável mas não é mantido em dia — a divergência que o ADR-0013 mata",
                e.bin
            );
        }
    }
}
