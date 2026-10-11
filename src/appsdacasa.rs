//! OS APPS DA CASA — descoberta e instalação dos apps do ecossistema.
//!
//! **O quê:** descobre se os apps da casa estão instalados, qual versão, e os instala do
//! fonte quando o usuário pede.
//!
//! **Onde:** `schematize deployer <sub>` (CLI) e, adiante, o card da GUI.
//!
//! ## As três regras desta ponte
//!
//! **1. A ausência do Deployer nunca derruba o schematize** (piso 10). Todo caminho aqui
//! devolve "não instalado" como *estado*, não como erro. O schematize funcionava sem ele
//! ontem e continua funcionando hoje — a ponte é conveniência, não dependência.
//!
//! **2. Subprocesso, não biblioteca.** Ligar os dois crates faria o schematize compilar o
//! Deployer junto, e aí não haveria dois apps: haveria um monólito com dois nomes. É o que o
//! ADR-0010 decidiu evitar.
//!
//! **3. O Deployer nunca devolve segredo por aqui — e é o ponto.** Esta ponte lê **versão** e
//! **código de saída**. Se um dia alguém quiser trazer a passphrase do cofre ou o conteúdo de
//! uma chave por este canal, a resposta é não: o motivo de o Deployer existir é justamente
//! que o segredo **não** transite pelo processo onde o agente opera.

use crate::agentrun::resolve_bin;
use std::path::PathBuf;

/// Um app EXTERNO do ecossistema — instalável à parte, e que o schematize apenas conhece.
///
/// **Por que uma tabela e não um módulo por app:** a lógica de descobrir, versionar e
/// instalar é idêntica; duplicá-la por app seria a divergência esperando acontecer. O que
/// muda entre eles é só o nome do binário, a flag e a frase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppExterno {
    /// Nome do binário no `$PATH`.
    pub bin: &'static str,
    /// Uma linha sobre o que ele faz — vai no `status`.
    pub sobre: &'static str,
}

// O campo `flag` (a flag do `install.sh` que instalava este app) SAIU — ver
// [`instalar_do_fonte`]. Ele só existia para montar o `curl | bash`, que deixou de ser o
// caminho de instalação.

/// Os apps externos que o schematize conhece.
pub const EXTERNOS: &[AppExterno] = &[
    AppExterno {
        bin: "schematize-deployer",
        sobre: "SSH, VPS, DNS e cofre — opera servidor com a credencial fora do agente",
    },
    AppExterno {
        bin: "schematize-optimizer",
        sobre: "mede o ambiente de dev e põe cada software no seu teto de recurso",
    },
    AppExterno {
        bin: "schematize-database",
        sobre: "lê o schema de um banco, modela e emite SQL ou migration expand-contract",
    },
    AppExterno {
        bin: "schematize-git",
        sobre: "contas, o que ainda não saiu desta máquina, e os repositórios de cada conta",
    },
    AppExterno {
        bin: "schematize-skills",
        sobre: "o catálogo de skills, o que está instalado, a autoria e o fork",
    },
    AppExterno {
        bin: "schematize-overdev",
        sobre: "o laço de desenvolvimento contínuo: checklist, caixa, notas e a execução do agente",
    },
];

// O `schematize-skills` (E5) e o `schematize-overdev` (E6) entram na tabela quando viram
// repositório com `[[bin]]` próprio e janela no mesmo repo — o ADR-0012 F4 os extraiu do hub
// com a ressalva de que um overdev meio extraído seria pior que um hub com overdev dentro, e a
// extradição só conta como feita com o repo existindo. O skills já é público. O overdev está no
// código do market (`main`) mas o repo `schematizeme/schematize_overdev_rs` é PRIVADO até o dono
// virar a chave: antes disso o `install` falha com a causa `not_found` dita
// ([`avaliar_acesso`]), nunca com o erro cru do `git`. Os apps que o hub PUBLICADO ainda não
// delega são marcados em `schematize_cli_rs/src/instalardica.rs` (`mercado`), não aqui: esta
// tabela diz o que o market de `main` gerencia; aquela diz o que o market PUBLICADO gerencia.

/// **O quê:** decide, a partir da sonda do GitHub, se vale tentar clonar `repo`. Função PURA.
///
/// **Onde:** [`checar_acesso_ao_repo`], antes do primeiro `git clone` de [`instalar_do_fonte`].
///
/// **Por que existe:** um repo ainda PRIVADO (o `schematize-overdev` é, até o dono publicá-lo)
/// faz o `git clone` pedir usuário/senha ou morrer com `fatal: could not read Username` —
/// causa verdadeira, mensagem inútil para quem não sabe o que é um repo privado. A
/// [`Causa::NotFound`](crate::nucleo::rede::Causa) é a MESMA que o `status` já mostra em
/// `latest_release_error`; aqui ela ganha o texto acionável.
///
/// **Só o 404 barra:** rede fora, cota e qualquer outra resposta seguem para o clone, que dirá
/// a própria causa. Barrar por uma sonda que falhou por OUTRO motivo mentiria.
pub fn avaliar_acesso(
    bin: &str,
    repo: &str,
    sonda: Result<String, (crate::nucleo::rede::Causa, String)>,
) -> Result<(), String> {
    match sonda {
        Err((crate::nucleo::rede::Causa::NotFound, _)) => Err(format!(
            "não consegui instalar o `{bin}`: o repositório {repo} não foi encontrado no GitHub \
             (not_found) — ele é privado ou ainda não foi publicado.\n  \
             Nada foi alterado na sua máquina. O `{bin}` não é pré-requisito do resto: o que já \
             está instalado segue funcionando.\n  \
             Se você TEM acesso ao repositório, entre no GitHub (`gh auth login`) e repita:\n    \
             schematize-market install {bin}\n  \
             Se não, repita quando ele for publicado."
        )),
        _ => Ok(()),
    }
}

/// **O quê:** sonda o GitHub e aplica [`avaliar_acesso`] — mas só quando um clone novo vai
/// acontecer. **Onde:** [`instalar_do_fonte`].
///
/// **Checkout existente pula a sonda:** quem já tem o fonte em `build/` (ou o semeou de um
/// clone local) não depende do 404 público. **Quem tem credencial passa:** o 404 anônimo do
/// `curl` é o que um repo privado diz a TODOS; um `git ls-remote` sem prompt pergunta com a
/// credencial da pessoa, e só se ele também falhar o install é barrado.
fn checar_acesso_ao_repo(bin: &str, repo: &str) -> Result<(), String> {
    if crate::nucleo::plataforma::build_src_dir(repo).join(".git").is_dir() {
        return Ok(());
    }
    let sonda = crate::nucleo::rede::url_do_ultimo_release_tipado(repo);
    let veredito = avaliar_acesso(bin, repo, sonda);
    if veredito.is_ok() {
        return Ok(());
    }
    let url = format!("https://github.com/{repo}");
    // **Sem prompt de NENHUM tipo:** `GIT_TERMINAL_PROMPT=0` sozinho não basta — medido, o git
    // cai no `ssh-askpass` e abre um diálogo gráfico "Username for github.com" na tela da pessoa
    // (e trava esperando). `GIT_ASKPASS=true` responde vazio na hora, e sem `DISPLAY` nem há
    // onde desenhar. A sonda pergunta com a credencial que JÁ existe; nunca pede uma nova.
    let mut sonda = std::process::Command::new("git");
    sonda
        .args(["ls-remote", "--exit-code", &url, "HEAD"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "true")
        .env("SSH_ASKPASS", "true")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY");
    let com_credencial = sonda
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match com_credencial {
        Ok(st) if st.success() => Ok(()),
        // Falha ao executar o git ou resposta negativa: o 404 anônimo vale, e é dito.
        _ => veredito,
    }
}

/// **O quê:** o nome que este binário TEVE antes, se ele foi renomeado. Uma fonte só —
/// [`crate::atualizar::APPS_GERIDOS`].
///
/// **Onde:** [`descobrir_app`], como segunda tentativa.
///
/// **Por que existe, e por que consulta a tabela em vez de repetir a lista:** o Deployer se
/// chamou `deployer` até o commit `0fa0112`. O `atualizar` já reconhecia o nome velho — mas o
/// `descobrir_app`, que é quem o `list` e a GUI usam, não. O resultado, medido nesta máquina:
/// `schematize-market list` dizia **"não instalado"** sobre um deployer 0.5.0 que estava lá.
///
/// A lição não é "faltou um lugar": é que a correção tinha sido PONTUAL onde precisava ser da
/// TABELA. Repetir a lista aqui recriaria exatamente a divergência que o bug original é.
fn nome_legado(bin: &str) -> Option<&'static str> {
    crate::atualizar::APPS_GERIDOS.iter().find(|a| a.bin == bin).and_then(|a| a.legado)
}

/// **O quê:** acha um app externo pelo nome do binário.
/// **Onde:** a CLI, ao despachar `schematize <app> …`.
pub fn externo(bin: &str) -> Option<&'static AppExterno> {
    EXTERNOS.iter().find(|a| a.bin == bin)
}

// As constantes `BIN`, `REPO` e `INSTALL_SH` saíram daqui (ADR-0013). As duas primeiras
// duplicavam o que a tabela `atualizar::APPS_GERIDOS` já diz — e duas fontes para o mesmo
// fato é como o nome do binário do Deployer ficou errado por um release inteiro. A terceira
// era a URL do `curl | bash`, que deixou de existir como caminho de instalação.

/// O que se sabe do Deployer nesta máquina.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Estado {
    /// Instalado, com o caminho e a versão que ele reporta.
    Instalado { caminho: PathBuf, versao: String },
    /// Não está no `$PATH` nem nos diretórios de fallback.
    Ausente,
    /// O binário existe mas não respondeu `--version` — instalação quebrada.
    ///
    /// É um terceiro estado de propósito: dizer "ausente" para um binário que ESTÁ lá mandaria
    /// a pessoa reinstalar o que já tem, e esconderia a causa real (permissão, biblioteca
    /// faltando, arquitetura errada).
    Quebrado { caminho: PathBuf, erro: String },
}

impl Estado {
    /// **O quê:** `true` só quando dá para usar o Deployer agora.
    /// **Onde:** a GUI e o `doctor`, para decidir entre "abrir" e "instalar".
    pub fn utilizavel(&self) -> bool {
        matches!(self, Estado::Instalado { .. })
    }
}

/// **O quê:** descobre o Deployer nesta máquina.
///
/// **Onde:** `schematize deployer status`, o `doctor` e a GUI.
///
/// Usa o mesmo `resolve_bin` do resto do app — que varre o `$PATH` **e** os diretórios de
/// fallback. Sem isso, o app aberto pelo lançador do desktop (que dá PATH mínimo) diria
/// "não instalado" sobre um Deployer que está em `~/.cargo/bin`.
pub fn descobrir_app(bin: &str) -> Estado {
    let Some(caminho) = resolve_bin(bin).or_else(|| nome_legado(bin).and_then(resolve_bin)) else {
        return Estado::Ausente;
    };
    match crate::util::run(&caminho.to_string_lossy(), &["--version"]) {
        Ok(saida) => {
            let versao = saida.split_whitespace().nth(1).unwrap_or("?").to_string();
            Estado::Instalado { caminho, versao }
        }
        Err(e) => Estado::Quebrado { caminho, erro: e },
    }
}

/// **O quê:** instala um app da casa COMPILANDO do fonte, aqui mesmo. `Err` com a causa se
/// não der.
///
/// **Onde:** `schematize-market install <app>`, e o `install.sh --deployer|--optimizer`, que
/// agora delega para cá.
///
/// ## O LOOP que esta função existe para desfazer (ADR-0013, D1)
///
/// O que estava aqui montava `curl -fsSL <install.sh> | bash -s -- --deployer` e executava.
/// Enquanto o `install.sh` era o único que sabia compilar, isso era razoável. Deixou de ser
/// por duas razões, e a segunda é fatal:
///
/// 1. **O market já sabe compilar.** [`crate::atualizar::fonte::build_one`] faz exatamente o
///    que aquele trecho do script fazia — checkout persistente, `target/` compartilhado, build
///    incremental. Manter os dois era manter duas verdades sobre "como se instala um app da
///    casa", que é a duplicação que o ADR-0013 existe para acabar.
/// 2. **O `install.sh` passou a delegar para cá.** Se esta função continuasse chamando o
///    script, `install.sh --deployer` → `market install` → `install.sh --deployer` seria um
///    **loop infinito**, recompilando o mundo a cada volta.
///
/// De quebra some um `curl | bash` de dentro de uma ferramenta: baixar e executar shell
/// arbitrário no meio de um programa é o tipo de elo que uma cadeia de suprimentos não
/// precisa ter.
///
/// **Herda o terminal de propósito:** a compilação leva minutos e pode pedir sudo para as
/// libs de build. Capturar a saída deixaria a pessoa olhando um cursor parado, e o pedido de
/// senha não teria onde aparecer.
pub fn instalar_do_fonte(bin: &str) -> Result<(), String> {
    let app = crate::atualizar::APPS_GERIDOS
        .iter()
        .find(|a| a.bin == bin)
        .ok_or_else(|| format!("não sei de que repositório vem o `{bin}`"))?;

    checar_acesso_ao_repo(app.bin, app.repo)?;

    let cargo = crate::nucleo::plataforma::ensure_toolchain()?;
    // As libs de build do sistema: o Deployer e o Optimizer não desenham janela, mas o
    // `ensure_build_deps` também traz o compilador C e o `pkg-config`, sem os quais nem as
    // deps nativas comuns linkam. É o mesmo passo que o `install.sh` fazia antes de compilar.
    crate::nucleo::plataforma::ensure_build_deps()?;

    crate::atualizar::fonte::build_one(
        cargo.to_str().unwrap_or("cargo"),
        app.repo,
        &[],
        None,
        &app.bin_name(),
        &app.caminho(),
    )?;
    crate::atualizar::metodo::registrar(app.repo, crate::atualizar::alvo::Metodo::Fonte);

    // A JANELA vem junto quando sai do mesmo repo (ADR-0020): o `build_one` copia UM binário e o
    // `cargo build` já produziu os dois. Sem este passo o `install` deixava a janela no
    // `target/` e a aba delegada do hub dizia "não instalada". Falha aqui não desfaz o app —
    // mas é dita, nunca engolida.
    if let Some((nome, dst)) = app.gui_nome_e_destino() {
        let built = crate::nucleo::plataforma::shared_target_dir().join("release").join(&nome);
        match crate::atualizar::binario::substitui_binario(&built, &dst) {
            Ok(()) => println!("✓ {nome}"),
            Err(e) => println!("aviso: `{}` está instalado, mas a janela não: {e}", app.bin),
        }
    }
    Ok(())
}

/// **O quê:** instala um app da casa — binário de release primeiro, fonte só se não houver.
///
/// **Onde:** `schematize-market install <app>` ([`crate::cli`]).
///
/// **O quê (detalhe):** tenta o asset `<bin>-<os>-<arch>` do último release (e o da janela);
/// compila SÓ se o repo não tem release casando. Rede/cota fora é `Err`, não build. Registra o
/// método `binario` (D8); o `fonte` é registrado por [`instalar_do_fonte`].
pub fn instalar_do_release_ou_fonte(bin: &str) -> Result<(), String> {
    use crate::atualizar::{alvo::Metodo, metodo, release};
    let app = crate::atualizar::APPS_GERIDOS
        .iter()
        .find(|a| a.bin == bin)
        .ok_or_else(|| format!("não sei de que repositório vem o `{bin}`"))?;
    let assets = release::assets_do_app(app);
    let via = release::instalar_ou_fonte(
        &release::RedeReal,
        app.repo,
        assets.as_ref(),
        &release::destino_do_app(app),
        || instalar_do_fonte(bin),
    )?;
    if matches!(via, release::Via::Release(_)) {
        metodo::registrar(app.repo, Metodo::Binario);
    }
    Ok(())
}

/// **O quê:** põe o app recém-instalado no menu de aplicativos. Best-effort, mas **nunca
/// mudo**.
///
/// **Onde:** depois de [`instalar_do_fonte`], uma vez por app.
///
/// **Por que best-effort e por que falante:** sem ícone é chato; derrubar a instalação por
/// causa dele é pior. Mas best-effort **não é mudo** — o `install.sh` chamava
/// `desktop --instalar >/dev/null 2>&1`, e quando a CLI dos apps foi traduzida a flag virou
/// `--install`: as chamadas passaram a falhar **em silêncio** e dois apps sumiram do menu sem
/// uma linha de erro. Aqui a falha diz o comando exato para repetir à mão.
pub fn registrar_no_menu(bin: &str) {
    let caminho = match resolve_bin(bin) {
        Some(p) => p,
        None => return,
    };
    match crate::util::run(&caminho.to_string_lossy(), &["desktop", "--install"]) {
        Ok(_) => println!("✓ {bin} já aparece no menu de aplicativos."),
        Err(e) => {
            println!("aviso: {bin} foi instalado, mas não consegui pôr o ícone no menu.");
            println!("  o que falhou: {} desktop --install", caminho.display());
            println!("  disse: {}", e.lines().next().unwrap_or("").trim());
            println!("  rode o comando acima para tentar de novo; o app funciona pelo");
            println!("  terminal do mesmo jeito.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `utilizavel` só é verdade no estado que de fato dá para usar. Sem esta distinção, um
    /// binário quebrado seria tratado como bom e a GUI abriria o que não abre.
    #[test]
    fn so_o_instalado_e_utilizavel() {
        let bom = Estado::Instalado { caminho: "/x/deployer".into(), versao: "0.2.1".into() };
        assert!(bom.utilizavel());
        assert!(!Estado::Ausente.utilizavel());
        let ruim = Estado::Quebrado { caminho: "/x/deployer".into(), erro: "libc".into() };
        assert!(!ruim.utilizavel(), "binário que não responde não pode contar como instalado");
    }

    /// **O LOOP, travado por teste.** Nenhum app da casa se instala por `curl | bash` do
    /// `install.sh` — o market compila ele mesmo. Se alguém reintroduzir aquele caminho, o
    /// `install.sh --deployer` (que hoje delega para cá) volta a reentrar em si mesmo.
    #[test]
    fn instalar_app_da_casa_nao_passa_por_curl_nem_pelo_install_sh() {
        let fonte = include_str!("appsdacasa.rs");
        // Só o código de PRODUÇÃO: este próprio teste cita os literais proibidos (é o que
        // ele procura), e os comentários acima explicam por que o `curl | bash` saiu —
        // citá-los ali é o contrário de um bug.
        let producao = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
        for (n, linha) in producao.lines().enumerate() {
            let l = linha.trim();
            if l.starts_with("//") || l.starts_with("///") {
                continue;
            }
            for proibido in ["curl ", "install.sh", "bash -s"] {
                assert!(
                    !l.contains(proibido),
                    "linha {}: `{proibido}` voltou ao caminho de instalar app da casa — \
                     é o loop do ADR-0013",
                    n + 1
                );
            }
        }
    }

    /// **O nome LEGADO é reconhecido também por aqui** — não só pelo `atualizar`.
    ///
    /// O `descobrir_app` alimenta o `market list` e a aba do Mercado na GUI. Enquanto ele
    /// ignorava o nome velho, as duas afirmavam "não instalado" sobre um deployer que estava
    /// na máquina — e essa é a resposta que manda a pessoa instalar o que já tem.
    #[test]
    fn descobrir_app_conhece_o_nome_legado() {
        assert_eq!(nome_legado("schematize-deployer"), Some("deployer"));
        assert_eq!(nome_legado("schematize-optimizer"), None);
        assert_eq!(nome_legado("nao-existe"), None);
        // E a fonte é a tabela, não uma lista repetida aqui: se o `legado` mudar lá, muda aqui.
        for a in crate::atualizar::APPS_GERIDOS {
            assert_eq!(nome_legado(a.bin), a.legado, "{} fora de sincronia com a tabela", a.bin);
        }
    }

    /// Todo app instalável tem de ter repo conhecido — senão [`instalar_do_fonte`] não sabe de
    /// onde compilar e a pessoa recebe um erro em vez de um app.
    #[test]
    fn todo_app_instalavel_sabe_de_onde_vem() {
        for e in EXTERNOS {
            let a = crate::atualizar::APPS_GERIDOS.iter().find(|a| a.bin == e.bin);
            assert!(a.is_some(), "{} é instalável mas não tem repo em APPS_GERIDOS", e.bin);
            assert!(a.unwrap().repo.contains('/'), "{}: repo inválido", e.bin);
        }
    }

    /// **A regra que a ponte existe para cumprir:** descobrir NUNCA falha. Numa máquina sem
    /// Deployer o resultado é `Ausente` — um estado —, não um erro que suba pro chamador.
    #[test]
    fn descobrir_nunca_falha_mesmo_sem_deployer() {
        // Não afirmamos QUAL estado (a máquina de quem roda pode ter o Deployer instalado);
        // afirmamos que a função retorna, sem panicar e sem `Result`.
        let e = descobrir_app("schematize-deployer");
        assert!(matches!(e, Estado::Instalado { .. } | Estado::Ausente | Estado::Quebrado { .. }));
    }

    /// **Todo app que o `update` conhece (e mantém se instalado) também é instalável pelo
    /// `install`/`list`.** Foi a assimetria que o overdev teria: na tabela do `atualizar` e fora
    /// de `EXTERNOS`, ele seria atualizado mas nunca instalável — e o `list` não o mostraria.
    #[test]
    fn todo_app_mantido_pelo_update_e_instalavel_e_listado() {
        for a in crate::atualizar::APPS_GERIDOS {
            if a.politica == crate::atualizar::Politica::Sempre {
                continue; // o market não se instala por aqui: é quem está rodando.
            }
            assert!(externo(a.bin).is_some(), "{} está em APPS_GERIDOS mas não em EXTERNOS", a.bin);
        }
    }

    /// O overdev é um app de dois binários no mesmo repo, como o skills: o headless E a janela.
    #[test]
    fn overdev_e_externo_com_janela_no_mesmo_repo() {
        assert!(externo("schematize-overdev").is_some());
        let a = crate::atualizar::APPS_GERIDOS
            .iter()
            .find(|a| a.bin == "schematize-overdev")
            .expect("overdev na tabela");
        assert_eq!(a.repo, "schematizeme/schematize_overdev_rs");
        assert_eq!(a.gui, Some("schematize-overdev-gui"));
        // D7/D8: o overdev publica `<bin>-<os>-<arch>` — o install tenta o release primeiro.
        assert_eq!(
            crate::atualizar::alvo::publica_binario(a.repo),
            crate::atualizar::release::assets_do_app(a).is_some()
        );
        let assets = crate::atualizar::release::assets_do_app(a);
        assert!(
            assets.is_none_or(|x| x.gui.as_deref().is_some_and(|g| g.starts_with(a.gui.unwrap())))
        );
    }

    /// **O repo privado (404) vira a causa DITA, não o erro cru do `git clone`.** Pura: a sonda
    /// entra como valor, sem rede.
    #[test]
    fn repo_privado_vira_mensagem_acionavel_e_nao_erro_de_git() {
        use crate::nucleo::rede::{classificar_redirect, Causa};
        let repo = "schematizeme/schematize_overdev_rs";
        let sonda = classificar_redirect(repo, "404 ");
        assert!(matches!(sonda, Err((Causa::NotFound, _))));
        let e = avaliar_acesso("schematize-overdev", repo, sonda).unwrap_err();
        assert!(e.contains("schematize-overdev"), "{e}");
        assert!(e.contains(repo), "{e}");
        assert!(e.contains("privado"), "diz que pode ser privado: {e}");
        assert!(e.contains("not_found"), "cita o slug estável do status: {e}");
        assert!(!e.contains("fatal:") && !e.contains("git clone"), "sem erro cru de git: {e}");
        assert!(
            e.contains("schematize-market install schematize-overdev"),
            "diz como repetir: {e}"
        );
    }

    /// Só o 404 barra. Rede fora, cota e release existente seguem para o clone, que dirá a
    /// própria causa — barrar aqui por uma sonda que falhou por OUTRO motivo mentiria.
    #[test]
    fn so_o_404_barra_o_install() {
        use crate::nucleo::rede::{classificar_redirect, Causa};
        let repo = "a/b";
        assert!(
            avaliar_acesso("x", repo, Ok("https://github.com/a/b/releases/tag/v1".into())).is_ok()
        );
        for (saida, causa) in
            [("000", Causa::Network), ("429 ", Causa::RateLimited), ("500 ", Causa::Unexpected)]
        {
            let sonda = classificar_redirect(repo, saida);
            assert!(matches!(&sonda, Err((c, _)) if *c == causa), "{saida}");
            assert!(avaliar_acesso("x", repo, sonda).is_ok(), "{saida} não pode barrar");
        }
    }

    /// O CLI instala por `instalar_do_release_ou_fonte` (release primeiro). Chamar `instalar_do_fonte` direto
    /// de lá traria de volta o build para todo leigo (§37.48).
    #[test]
    fn a_cli_instala_pelo_release_primeiro() {
        let cli = include_str!("cli/instalar.rs");
        let producao = cli.split("#[cfg(test)]").next().unwrap_or(cli);
        assert!(
            producao.contains("instalar_do_release_ou_fonte(a.bin)"),
            "a CLI deve chamar instalar_do_release_ou_fonte"
        );
        assert!(!producao.contains("instalar_do_fonte(a.bin)"), "build direto voltou à CLI");
        let aqui = include_str!("appsdacasa.rs");
        let corpo = aqui.split("pub fn instalar_do_release_ou_fonte").nth(1).unwrap_or("");
        let corpo = corpo.split("pub fn registrar_no_menu").next().unwrap_or("");
        assert!(corpo.contains("instalar_ou_fonte") && corpo.contains("Metodo::Binario"));
    }
}
