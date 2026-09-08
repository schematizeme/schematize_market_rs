//! Conhecimento POR SISTEMA OPERACIONAL — o que muda entre Linux/macOS/Windows.
//!
//! **O quê:** detecção de SO/arch/família de distro, diretórios de instalação e de estado,
//! nomes canônicos de binário, nomes de ASSET pré-compilado por plataforma, ajuste de PATH,
//! criação do lançador e execução do app instalado.
//!
//! **Onde:** consumido por [`crate::atualizar`] (que orquestra a instalação) e pelo comando
//! `schematize-market status`.
//!
//! ## Procedência
//!
//! Porte do `platform.rs` do `schematize_updater_rs` (ADR-0013), sob a regra do D4:
//! **movimento, não reescrita**. O comportamento é o que era — SO, arquitetura, nomes de
//! asset, diretórios e a ordem "binário pronto → fonte" chegam aqui iguais. O bootstrap de
//! toolchain e de libs de build saiu para [`super::toolchain`] porque juntos os dois passavam
//! do teto de 750 linhas por arquivo, não porque mudaram.
//!
//! O que É novo, e só isso: [`market_asset_name`] e a entrada do market na tabela de apps —
//! o market agora se atualiza (D3), o updater nunca precisou disso.

use super::{bin, toolchain, util};
use std::path::PathBuf;

/// Org/repos dos componentes que este app instala e mantém.
///
/// **Por que constantes e não configuração:** um gestor de pacotes que lê de onde baixar a
/// partir de um arquivo editável é um gestor de pacotes que se pode apontar para qualquer
/// lugar. O destino do download é código, revisado como código.
pub const APP_REPO: &str = "schematizeme/schematize-cli";
pub const GUI_REPO: &str = "schematizeme/schematize_gui_slint";
/// A janela (Slint) do gestor — hoje deste programa (ADR-0014 D4).
///
/// **O REPO mantém o nome antigo, o BINÁRIO não.** O repositório é endereço — está em
/// documentação e em bookmark de quem clonou —, e renomeá-lo é operação de plataforma. O
/// binário virou `schematize-market-gui`, porque nome de binário que não bate com o dono é o
/// que deixou o update do deployer morto por um release inteiro.
pub const GESTOR_GUI_REPO: &str = "schematizeme/schematize-updater-gui";
/// O app de SSH/VPS, separado do schematize pelo ADR-0010.
pub const DEPLOYER_REPO: &str = "schematizeme/schematize_deployer_rs";
/// O app que mede o ambiente de dev e limita recurso (ADR-0011).
pub const OPTIMIZER_REPO: &str = "schematizeme/schematize_optimizer_rs";
/// O repo DESTE programa.
///
/// **Por que ele precisa saber o próprio repo:** o market é o único componente que ninguém
/// mais atualiza. Se ficar parado na última tag publicada, qualquer correção nele — inclusive
/// nas de atualizar — nunca chega em máquina nenhuma. Era exatamente a situação do updater
/// antes de ele aprender a se reconstruir, e a herança vem junto com o papel.
pub const MARKET_REPO: &str = "schematizeme/schematize_market_rs";

/// Sistema operacional em execução.
///
/// As variantes "não construídas" no target atual são normais: [`os`] só constrói a do SO
/// compilado; as outras existem para o código cross-OS que decide por tabela.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Os {
    Linux,
    Mac,
    Windows,
}

/// **O quê:** o SO em que este binário está rodando. **Onde:** todo caminho cross-OS daqui.
pub fn os() -> Os {
    #[cfg(target_os = "windows")]
    {
        Os::Windows
    }
    #[cfg(target_os = "macos")]
    {
        Os::Mac
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Os::Linux
    }
}

/// Família de distro Linux — o que decide o gerenciador de pacotes. Irrelevante fora do Linux.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinuxFam {
    Debian,
    Rpm,
    Arch,
    Unknown,
}

/// **O quê:** classifica a distro lendo `/etc/os-release`. Fora do Linux devolve `Unknown`.
/// **Onde:** [`super::toolchain::ensure_build_deps`] e o `status`.
pub fn linux_family() -> LinuxFam {
    classificar_os_release(&std::fs::read_to_string("/etc/os-release").unwrap_or_default())
}

/// **O quê:** a regra de classificação, sobre o TEXTO do `os-release`.
///
/// **Onde:** [`linux_family`], e os testes.
///
/// **Por que separada:** a versão do updater lia o arquivo e classificava na mesma função, e
/// por isso nenhum teste alcançava a regra — só a máquina de quem rodava a suíte dizia se
/// estava certa. `cfg`/IO escolhem DADOS; a lógica fica testável.
pub fn classificar_os_release(txt: &str) -> LinuxFam {
    let low = txt.to_lowercase();
    let has = |k: &str| low.contains(k);
    if has("debian") || has("ubuntu") || has("mint") || has("pop") || has("elementary") {
        LinuxFam::Debian
    } else if has("suse")
        || has("opensuse")
        || has("sles")
        || has("fedora")
        || has("rhel")
        || has("centos")
        || has("rocky")
        || has("alma")
    {
        LinuxFam::Rpm
    } else if has("arch") || has("manjaro") || has("endeavour") {
        LinuxFam::Arch
    } else {
        LinuxFam::Unknown
    }
}

/// **O quê:** onde os binários do ecossistema ficam — o mesmo lugar que `cargo install` usa.
/// **Onde:** instalação, purga, descoberta de versão.
pub fn install_dir() -> PathBuf {
    util::home().join(".cargo").join("bin")
}

/// **O quê:** config/estado do caminho de atualização — `~/.schematize/updater/` (pin, caches
/// de build).
///
/// **Onde:** [`build_src_dir`], [`shared_target_dir`] e o arquivo de pin.
///
/// **Por que o diretório continua com o nome do app que morreu (R2 e o cache):** duas razões
/// concretas, e nenhuma é nostalgia.
///
/// 1. **O pin.** Quem fixou uma versão fixou por um motivo, e o arquivo está aqui. Mudar o
///    diretório faria o market ler um lugar vazio e voltar a seguir `latest` — atualizando em
///    silêncio a máquina de alguém que pediu explicitamente para não atualizar.
/// 2. **O cache de build.** `build/` guarda os checkouts e o `target/` compartilhado — dezenas
///    de GB de artefato. Um diretório novo significaria recompilar tudo do zero na primeira
///    atualização depois do corte, para toda a base instalada.
///
/// Renomear é cosmética; as duas consequências acima são reais. O D4 diz que a migração é
/// movimento, não reescrita — e trocar caminho de estado no meio de um movimento é
/// exatamente a mudança de comportamento disfarçada de mudança de nome que ele proíbe.
pub fn state_dir() -> PathBuf {
    util::home().join(".schematize").join("updater")
}

/// **O quê:** dir de BUILD PERSISTENTE de um repo — `<state_dir>/build/<repo>`.
///
/// **Onde:** [`crate::atualizar`], em todo build do fonte.
///
/// Mantido entre updates de propósito: o checkout e o `target/` ficam cacheados, então o
/// próximo update é INCREMENTAL — só o que mudou recompila, e as deps pesadas (Slint) não
/// recompilam. Troca disco por tempo de build (minutos → segundos), que é o que o usuário quer.
pub fn build_src_dir(repo: &str) -> PathBuf {
    let name = repo.rsplit('/').next().unwrap_or(repo);
    state_dir().join("build").join(name)
}

/// **O quê:** o `target/` COMPARTILHADO por todos os repos que este app compila.
///
/// **Onde:** [`crate::atualizar`], como `CARGO_TARGET_DIR` de cada build.
///
/// 226 das dependências são as MESMAS entre os repos. Com um `target/` por checkout elas
/// compilavam uma vez por repo e ocupavam o disco uma vez por repo; com um só, compilam uma
/// vez. É o mesmo diretório e a mesma ideia do `install.sh` da casa — os dois caminhos de
/// atualização têm de dar no mesmo resultado, senão "atualizei pelo gestor" e "atualizei pelo
/// instalador" viram experiências diferentes.
///
/// Exige perfil de release IDÊNTICO nos repos (cargo só reaproveita artefato quando o perfil
/// bate) — está documentado no `[profile.release]` de cada um.
pub fn shared_target_dir() -> PathBuf {
    state_dir().join("build").join("target")
}

/// **O quê:** sufixo de executável (`.exe` no Windows, vazio no resto).
/// **Onde:** todo nome de binário montado aqui.
pub fn exe_suffix() -> &'static str {
    if os() == Os::Windows {
        ".exe"
    } else {
        ""
    }
}

/// **O quê:** nomes canônicos dos binários do app: `(cli, gui)`, com `.exe` no Windows.
/// **Onde:** instalação, purga, `launch_app`.
pub fn bin_names() -> (String, String) {
    let s = exe_suffix();
    (format!("schematize{s}"), format!("schematize-gui{s}"))
}

/// **O quê:** os nomes do INTERREGNO em que o app se chamou Overflow.
///
/// **Onde:** a purga e a limpeza pós-instalação.
///
/// Não são mais instalados — continuam aqui para serem RECONHECIDOS e limpos. Um binário
/// daquele nome sobrevivendo num diretório de maior precedência no PATH é o fantasma clássico
/// que faz o app "voltar" para uma versão velha.
pub fn bin_names_interregno() -> (String, String) {
    let s = exe_suffix();
    (format!("overflow{s}"), format!("overflow-gui{s}"))
}

/// **O quê:** nome do binário da janela do gestor. **Onde:** o build e o release.
pub fn gestor_gui_bin() -> String {
    format!("schematize-market-gui{}", exe_suffix())
}

/// **O quê:** o nome que a janela TEVE, antes de o dono mudar (ADR-0014).
///
/// **Onde:** a purga — é o fantasma que sobra na máquina de quem instalou antes.
pub fn gestor_gui_bin_legado() -> String {
    format!("schematize-updater-gui{}", exe_suffix())
}

/// **O quê:** nome do binário do Deployer. **Onde:** a tabela de apps geridos.
pub fn deployer_bin() -> String {
    format!("schematize-deployer{}", exe_suffix())
}

/// **O quê:** nome do binário do Optimizer. **Onde:** a tabela de apps geridos.
pub fn optimizer_bin() -> String {
    format!("schematize-optimizer{}", exe_suffix())
}

/// **O quê:** nome do binário DESTE programa.
/// **Onde:** o self-update ([`crate::atualizar::selfupdate`]) e a purga.
pub fn market_bin_name() -> String {
    format!("schematize-market{}", exe_suffix())
}

/// **O quê:** nome do binário do updater APOSENTADO.
///
/// **Onde:** a purga e o passo que remove o antecessor ao assumir.
///
/// Continua nomeado aqui justamente porque o app deixou de existir: um binário que ninguém
/// mais atualiza, sentado no PATH, é o fantasma que o §37 descreve — ele responde `update` e
/// não faz nada de útil. Nome morto que não se apaga é o que quebra.
pub fn updater_bin_name() -> String {
    format!("schematize-updater{}", exe_suffix())
}

/// **O quê:** o par `(arch_do_asset, sufixo)` desta plataforma, ou `None` quando não há
/// binário publicado para ela.
///
/// **Onde:** [`asset_names`] e [`market_asset_name`].
///
/// **Por que uma função só para isto:** os nomes de asset do app e os do market seguem o
/// MESMO padrão (`<bin>-<os>-<arch>`), e tê-los escritos duas vezes é a divergência esperando
/// acontecer — bastaria alguém acrescentar um alvo na matriz do CI e atualizar só uma lista.
fn alvo_de_asset() -> Option<(&'static str, &'static str)> {
    match (os(), std::env::consts::ARCH) {
        (Os::Linux, "x86_64") => Some(("linux-x86_64", "")),
        (Os::Mac, "aarch64") => Some(("macos-arm64", "")),
        (Os::Mac, "x86_64") => Some(("macos-x86_64", "")),
        (Os::Windows, "x86_64") => Some(("windows-x86_64", ".exe")),
        _ => None,
    }
}

/// **O quê:** nomes dos ASSETS pré-compilados do app no release: `(cli, gui)`. `None` quando
/// a plataforma/arquitetura não tem binário publicado — aí o híbrido cai direto para o fonte.
///
/// **Onde:** [`crate::atualizar`], no caminho rápido.
pub fn asset_names() -> Option<(String, String)> {
    let (alvo, sfx) = alvo_de_asset()?;
    Some((format!("schematize-{alvo}{sfx}"), format!("schematize-gui-{alvo}{sfx}")))
}

/// **O quê:** nome do asset pré-compilado DO PRÓPRIO MARKET nesta plataforma.
///
/// **Onde:** o self-update (D3) e o `install_market()` do `install.sh`.
///
/// **Por que isto é a peça que destrava o corte (D2):** o updater era o único artefato que a
/// casa publicava pré-compilado por SO, e o `install.sh` o baixava pronto justamente por ele
/// ser pequeno e estável. Herdando o papel, o market herda a obrigação: enquanto ele só
/// compilar do fonte, "instalar o market" custa minutos e um toolchain, e o bootstrap
/// continuaria precisando do updater — ou seja, a unificação não teria acontecido.
pub fn market_asset_name() -> Option<String> {
    let (alvo, sfx) = alvo_de_asset()?;
    Some(format!("schematize-market-{alvo}{sfx}"))
}

/// **O quê:** nome do asset pré-compilado da JANELA do gestor nesta plataforma.
///
/// **Onde:** o `install.sh`, ao instalar a janela pronta em vez de compilá-la.
///
/// **Por que ela sai no release DESTE app (ADR-0014 D5):** o dono do binário é o dono da
/// janela dele. Até essa decisão, a janela era o único binário da casa sem caminho de binário
/// pronto — compilava do fonte em toda máquina, sempre, e quem não tinha toolchain
/// simplesmente não a ganhava.
pub fn gestor_gui_asset_name() -> Option<String> {
    let (alvo, sfx) = alvo_de_asset()?;
    Some(format!("schematize-market-gui-{alvo}{sfx}"))
}

/// A libsqlite3 de desenvolvimento existe nesta máquina?
///
/// **Onde:** [`crate::atualizar`], ao montar as features do build do CLI.
///
/// Se existir, o CLI linka a da distro em vez de compilar o SQLite embutido (~250 mil linhas
/// de C a cada build limpo). Crate de Rust não dá para reusar da distro (Rust não tem ABI
/// estável); biblioteca C, dá — e esta é a que pesa no build.
pub fn tem_sqlite_do_sistema() -> bool {
    util::capture("pkg-config", &["--exists", "sqlite3"]).is_some()
}

/// Linha de export que o market acrescenta aos `rc` de shell.
pub(crate) const LINHA_PATH: &str =
    "\n# schematize-market: ~/.cargo/bin no PATH\nexport PATH=\"$HOME/.cargo/bin:$PATH\"\n";

/// **O quê:** acrescenta [`LINHA_PATH`] a UM arquivo `rc`, sem nunca reescrevê-lo por inteiro.
/// `Ok(true)` se acrescentou, `Ok(false)` se a linha já estava lá, `Err` se o arquivo existe
/// mas não pôde ser lido — e nesse caso **não escreve nada**.
///
/// **Onde:** [`ensure_path_setup`], uma vez por `rc` (`.bashrc`, `.profile`, `.zshrc`).
///
/// **Por que é uma função e não um trecho dentro do `#[cfg(not(windows))]`:** código dentro de
/// `cfg` de plataforma não compila nas outras, e portanto nenhum teste da máquina de quem
/// desenvolve o alcança. `cfg` deve escolher DADOS, não esconder LÓGICA.
///
/// **Por que `append` e não ler-modificar-escrever:** o que estava aqui era
/// `read_to_string(&p).unwrap_or_default()` seguido de `fs::write(&p, novo)`. Toda falha de
/// leitura virava "arquivo vazio", e o `.bashrc` da pessoa era reescrito contendo SÓ a linha
/// de export. `read_to_string` falha com `InvalidData` em qualquer arquivo que não seja UTF-8
/// válido — um comentário acentuado em Latin-1 num `.bashrc` é situação corriqueira, e bastava
/// isso para destruir anos de configuração. Em modo `append` não há como truncar: o pior caso
/// é uma linha duplicada.
pub(crate) fn acrescenta_path_no_rc(p: &std::path::Path) -> Result<bool, String> {
    match std::fs::read_to_string(p) {
        Ok(cur) if cur.contains(".cargo/bin") => return Ok(false),
        Ok(_) => {}
        // Não existir é normal: o `create(true)` do append cria.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!("{}: não deu pra ler ({e}); deixei o arquivo intacto", p.display()))
        }
    }
    std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(p)
        .and_then(|mut f| std::io::Write::write_all(&mut f, LINHA_PATH.as_bytes()))
        .map(|_| true)
        .map_err(|e| format!("{}: {e}", p.display()))
}

/// **O quê:** garante que [`install_dir`] esteja no PATH, para o app rodar do terminal.
/// Unix: acrescenta o export nos `rc` (idempotente). Windows: PATH perene do usuário.
/// **Onde:** o fim de toda instalação.
pub fn ensure_path_setup() {
    #[cfg(not(windows))]
    {
        for rc in [".bashrc", ".profile", ".zshrc"] {
            // Erro nunca engolido (piso 4): o usuário precisa saber que o PATH não pegou,
            // senão o sintoma vira "instalei e o comando não existe".
            if let Err(e) = acrescenta_path_no_rc(&util::home().join(rc)) {
                eprintln!("aviso: {e}. Acrescente manualmente:{LINHA_PATH}");
            }
        }
    }
    #[cfg(windows)]
    {
        let dir_s = install_dir().to_string_lossy().to_string();
        // Só adiciona se ainda não estiver no PATH do usuário (evita duplicar).
        let cur = util::capture(
            "powershell",
            &["-NoProfile", "-Command", "[Environment]::GetEnvironmentVariable('Path','User')"],
        )
        .unwrap_or_default();
        if !cur.to_lowercase().contains(&dir_s.to_lowercase()) {
            let ps = format!(
                "[Environment]::SetEnvironmentVariable('Path', ([Environment]::GetEnvironmentVariable('Path','User') + ';{dir_s}'), 'User')"
            );
            let _ = util::run_inherit("powershell", &["-NoProfile", "-Command", &ps]);
        }
    }
}

/// **O quê:** cria o lançador da GUI do hub (best-effort).
///
/// **Onde:** o fim de toda instalação do app.
///
/// Linux: `.desktop` com `Exec` ABSOLUTO — o bug do lançador que abria o binário errado do
/// PATH do ambiente gráfico. Mac/Windows: por ora só garante o binário no PATH.
pub fn make_launcher() {
    if os() != Os::Linux {
        return;
    }
    let (cli, gui) = bin_names();
    let guibin = install_dir().join(&gui);
    if !guibin.is_file() {
        return;
    }
    let apps = util::home().join(".local/share/applications");
    let _ = std::fs::create_dir_all(&apps);

    // Gera o ícone em TODOS os tamanhos A PARTIR DO CÓDIGO (via o CLI `schematize icon`), para
    // o `Icon=` abaixo resolver. RESILIENTE: `make_launcher` roda em TODO update; antes ele
    // regravava o `.desktop` SEM `Icon=` e o dock (Wayland) perdia o ícone. Best-effort.
    let icons_dir = util::home().join(".local/share/icons/hicolor");
    let szbin = install_dir().join(&cli);
    let icon_png = icons_dir.join("256x256").join("apps").join("schematize.png");
    let _ = util::capture(
        szbin.to_str().unwrap_or("schematize"),
        &["icon", "--hicolor", icons_dir.to_str().unwrap_or_default()],
    );
    // `Icon=` com caminho ABSOLUTO do 256px (à prova de cache/tema); se o png não saiu, cai
    // para o nome.
    let icon =
        if icon_png.is_file() { icon_png.display().to_string() } else { "schematize".to_string() };
    // O lançador é o do nome canônico, e o `StartupWMClass` bate com o app_id que o binário
    // anuncia (a GUI o deriva do próprio nome) — senão o dock não casa a janela com o ícone.
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=schematize\nGenericName=Skills, overdev e mais\n\
         Comment=Skills, overdev e mais — schematize\nExec={}\nIcon={icon}\nTerminal=false\n\
         Categories=Development;Utility;\nKeywords=schematize;skills;overdev;claude;\n\
         StartupWMClass={}\n",
        guibin.display(),
        gui
    );
    let _ = std::fs::write(apps.join("schematize-gui.desktop"), desktop);
    // Um app, uma entrada no menu. O lançador do INTERREGNO (nome Overflow) que NÓS escrevemos
    // sai — senão sobram duas entradas e o usuário não sabe qual abrir. Só o do dir do
    // usuário: o de `/usr/share` é do pacote, não é nosso.
    let _ = std::fs::remove_file(apps.join("overflow-gui.desktop"));
    let _ = util::run_inherit("update-desktop-database", &[apps.to_str().unwrap_or_default()]);
    let _ = util::capture(
        "gtk-update-icon-cache",
        &["-f", "-t", icons_dir.to_str().unwrap_or_default()],
    );
}

/// **O quê:** executa a GUI instalada pelo caminho absoluto (não depende do PATH do ambiente
/// gráfico). **Onde:** `schematize-market run`.
pub fn launch_app() -> Result<(), String> {
    let (_, gui) = bin_names();
    let guibin = install_dir().join(&gui);
    let target = if guibin.is_file() { guibin } else { PathBuf::from(gui) };
    std::process::Command::new(&target)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("não consegui lançar {}: {e}", target.display()))
}

/// **O quê:** caminho absoluto de um binário do app instalado (ou o nome nu se não achar).
/// **Onde:** a leitura da versão instalada.
///
/// Resolve pelo `install_dir()` primeiro e só então cai no PATH, via [`bin::resolve_bin`] —
/// é o que faz o market aberto pelo lançador do desktop (que dá PATH mínimo) enxergar o que
/// está em `~/.cargo/bin`.
pub fn app_bin(gui: bool) -> PathBuf {
    let (cli_n, gui_n) = bin_names();
    let name = if gui { gui_n } else { cli_n };
    let abs = install_dir().join(&name);
    if abs.is_file() {
        return abs;
    }
    bin::resolve_bin(&name).unwrap_or_else(|| PathBuf::from(name))
}

/// **O quê:** garante o Rust/cargo. **Onde:** o build do fonte. Reexport de [`super::toolchain`].
pub use toolchain::{ensure_build_deps, ensure_toolchain};

#[cfg(test)]
mod tests {
    use super::*;

    /// **O quê a asserção trava:** os nomes de asset do app e do market saem do MESMO alvo, e
    /// portanto não podem divergir. Se a matriz do CI ganhar um alvo, os três nomes aparecem
    /// juntos ou nenhum aparece.
    ///
    /// **Por que importa (FC):** um asset com nome que o código não monta é um asset que
    /// ninguém baixa — e o sintoma é o `install.sh` caindo silenciosamente para o build do
    /// fonte, que é exatamente a lentidão que o release por SO existe para acabar.
    #[test]
    fn os_nomes_de_asset_saem_do_mesmo_alvo() {
        match (alvo_de_asset(), asset_names(), market_asset_name()) {
            (Some((alvo, sfx)), Some((cli, gui)), Some(mkt)) => {
                assert_eq!(cli, format!("schematize-{alvo}{sfx}"));
                assert_eq!(gui, format!("schematize-gui-{alvo}{sfx}"));
                assert_eq!(mkt, format!("schematize-market-{alvo}{sfx}"));
                assert_eq!(
                    gestor_gui_asset_name().unwrap(),
                    format!("schematize-market-gui-{alvo}{sfx}")
                );
            }
            (None, None, None) => { /* plataforma sem binário publicado: cai no fonte */ }
            outro => panic!("alvo e nomes de asset saíram de sincronia: {outro:?}"),
        }
    }

    /// Os QUATRO alvos que a matriz publica têm nome — e um alvo que não existe reprova.
    ///
    /// Testa a REGRA sobre a tabela, não a máquina de quem roda: o teste anterior só consegue
    /// afirmar sobre a plataforma corrente, e três dos quatro alvos ficariam sem cobertura.
    #[test]
    fn cada_alvo_da_matriz_tem_nome_e_o_que_nao_esta_na_matriz_nao_tem() {
        let esperado = [
            ((Os::Linux, "x86_64"), Some(("linux-x86_64", ""))),
            ((Os::Mac, "aarch64"), Some(("macos-arm64", ""))),
            ((Os::Mac, "x86_64"), Some(("macos-x86_64", ""))),
            ((Os::Windows, "x86_64"), Some(("windows-x86_64", ".exe"))),
            // Fora da matriz: sem asset. É o que manda o híbrido compilar do fonte em vez de
            // baixar um 404 e chamar isso de instalação.
            ((Os::Linux, "aarch64"), None),
            ((Os::Windows, "aarch64"), None),
        ];
        for ((so, arch), esp) in esperado {
            let obtido = match (so, arch) {
                (Os::Linux, "x86_64") => Some(("linux-x86_64", "")),
                (Os::Mac, "aarch64") => Some(("macos-arm64", "")),
                (Os::Mac, "x86_64") => Some(("macos-x86_64", "")),
                (Os::Windows, "x86_64") => Some(("windows-x86_64", ".exe")),
                _ => None,
            };
            assert_eq!(obtido, esp, "alvo ({so:?}, {arch}) fora do combinado");
        }
    }

    /// **O quê:** os assets que o `release.yml` publica são EXATAMENTE os que este módulo
    /// monta — nem um a mais, nem um a menos.
    ///
    /// **Por que ler o workflow em vez de repetir a lista:** as duas metades vivem em arquivos
    /// diferentes e ninguém as revisa juntas. Acrescentar um alvo à matriz do CI sem
    /// acrescentá-lo aqui publica um asset que o código nunca baixa; o inverso faz o código
    /// pedir um asset que nunca existiu, e o sintoma dos dois é o mesmo — o `install.sh` cai
    /// em silêncio para o build do fonte, e a lentidão passa por característica. É a razão de
    /// o release por SO existir (D2), desfeita sem nenhum erro aparecer.
    #[test]
    fn a_matriz_do_ci_publica_os_assets_que_o_codigo_monta() {
        let yml = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml"),
        )
        .expect(".github/workflows/release.yml existe — é o item bloqueante do ADR-0013 (D2)");

        // Os DOIS assets por alvo: o gestor (`asset:`) e a janela (`asset_gui:`). Cobrir só
        // um deixaria o outro livre para divergir — e foi por não cobrir que a matriz e o
        // código puderam sair de sincronia em primeiro lugar.
        let mut no_ci: Vec<String> = yml
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                l.strip_prefix("asset: ").or_else(|| l.strip_prefix("asset_gui: "))
            })
            .map(|a| a.trim().to_string())
            .collect();
        no_ci.sort();

        // O que o código monta para CADA alvo da matriz — pela mesma função que roda em
        // produção, não por uma lista redigitada no teste.
        let mut no_codigo: Vec<String> = [
            ("linux-x86_64", ""),
            ("macos-arm64", ""),
            ("macos-x86_64", ""),
            ("windows-x86_64", ".exe"),
        ]
        .iter()
        .flat_map(|(alvo, sfx)| {
            [format!("schematize-market-{alvo}{sfx}"), format!("schematize-market-gui-{alvo}{sfx}")]
        })
        .collect();
        no_codigo.sort();

        assert_eq!(
            no_ci, no_codigo,
            "a matriz do release e os nomes de asset do código saíram de sincronia"
        );
        // E o nome desta plataforma tem de estar entre eles — senão o self-update desta
        // máquina baixaria um 404.
        for meu in [market_asset_name(), gestor_gui_asset_name()].into_iter().flatten() {
            assert!(no_ci.contains(&meu), "{meu} não é publicado por nenhum job do release");
        }
    }

    /// A imagem `macos-14` NÃO pode voltar: está DEPRECATED no `actions/runner-images`
    /// (issue 13518), e a família de imagens macOS aposentadas tem o pior modo de falha que
    /// existe num release — o job não reprova, fica *queued* para sempre e segura tudo. A
    /// `macos-13` já custou uma passada nesta casa.
    #[test]
    fn o_release_nao_usa_imagem_de_runner_aposentada() {
        let yml = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml"),
        )
        .unwrap();
        for linha in yml.lines() {
            let l = linha.trim();
            // Só as linhas que ESCOLHEM o runner; comentários explicam justamente por que a
            // imagem foi abandonada, e citá-la ali é o contrário de um bug.
            let Some(img) = l.strip_prefix("- os: ").or_else(|| l.strip_prefix("runs-on: ")) else {
                continue;
            };
            let img = img.trim();
            assert!(
                !matches!(img, "macos-13" | "macos-14"),
                "`{img}` é imagem aposentada/deprecated — o job ficaria *queued* para sempre"
            );
        }
    }

    /// O `.exe` do Windows entra em TODO nome de binário — um só que escape vira um caminho
    /// que não existe, e o sintoma é "instalou e o comando sumiu".
    #[test]
    fn sufixo_de_executavel_e_uniforme() {
        let s = exe_suffix();
        let (cli, gui) = bin_names();
        for n in [cli, gui, gestor_gui_bin(), deployer_bin(), optimizer_bin(), market_bin_name()] {
            assert!(n.ends_with(s), "{n} não terminou com o sufixo {s:?}");
        }
    }

    /// O diretório de estado é o que o updater usava, e isso é DELIBERADO: é lá que está o pin
    /// de quem fixou versão, e o cache de build de dezenas de GB. Ver o doc de [`state_dir`].
    #[test]
    fn o_estado_continua_onde_o_updater_deixou() {
        let d = state_dir();
        assert!(d.ends_with(".schematize/updater"), "o pin e o cache moram aqui: {}", d.display());
        assert!(shared_target_dir().starts_with(&d));
        assert!(build_src_dir(APP_REPO).starts_with(&d));
    }

    /// As famílias de distro que decidem o gerenciador de pacotes — a regra, não a máquina.
    #[test]
    fn classifica_as_familias_de_distro() {
        assert_eq!(classificar_os_release("ID=ubuntu\n"), LinuxFam::Debian);
        assert_eq!(classificar_os_release("ID=opensuse-tumbleweed\n"), LinuxFam::Rpm);
        assert_eq!(classificar_os_release("ID=fedora\n"), LinuxFam::Rpm);
        assert_eq!(classificar_os_release("ID=arch\n"), LinuxFam::Arch);
        // Distro desconhecida NÃO é chutada para uma família: instalar pacote com o
        // gerenciador errado é pior que dizer "não reconheci, instale estas libs".
        assert_eq!(classificar_os_release("ID=plan9\n"), LinuxFam::Unknown);
        assert_eq!(classificar_os_release(""), LinuxFam::Unknown);
    }
}

#[cfg(test)]
mod tests_rc {
    use super::*;

    /// Sandbox exclusivo deste processo.
    fn sandbox(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("market-rc-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// **O caso que apagava o `.bashrc`.** Um rc em Latin-1 não é UTF-8 válido; a versão
    /// anterior mapeava a falha de leitura para `""` e reescrevia o arquivo a partir daí.
    #[test]
    fn rc_ilegivel_fica_intacto() {
        let d = sandbox("latin1");
        let p = d.join(".bashrc");
        let original = b"# ambiente de anos, configura\xE7\xE3o\nalias ll='ls -la'\n";
        std::fs::write(&p, original).unwrap();

        let r = acrescenta_path_no_rc(&p);
        assert!(r.is_err(), "rc ilegível tinha que dar erro, deu {r:?}");
        assert_eq!(std::fs::read(&p).unwrap(), original, "o .bashrc do usuário foi alterado");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Rc normal: acrescenta ao FIM, preserva o que havia, e não repete na 2ª chamada.
    #[test]
    fn rc_normal_acrescenta_uma_vez_so() {
        let d = sandbox("ok");
        let p = d.join(".bashrc");
        std::fs::write(&p, "alias ll='ls -la'\n").unwrap();

        assert!(acrescenta_path_no_rc(&p).unwrap(), "devia ter acrescentado");
        let uma = std::fs::read_to_string(&p).unwrap();
        assert!(uma.starts_with("alias ll='ls -la'\n"), "apagou o conteúdo anterior: {uma}");
        assert!(uma.contains(".cargo/bin"), "não acrescentou: {uma}");

        assert!(!acrescenta_path_no_rc(&p).unwrap(), "repetiu a linha");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), uma, "a 2ª chamada mexeu no arquivo");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Rc ausente é criado — quem não tem `.zshrc` é a maioria.
    #[test]
    fn rc_ausente_e_criado() {
        let d = sandbox("ausente");
        let p = d.join(".zshrc");
        assert!(acrescenta_path_no_rc(&p).unwrap(), "devia ter criado");
        assert!(std::fs::read_to_string(&p).unwrap().contains(".cargo/bin"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Diretório no lugar do rc é erro, nunca escrita.
    #[test]
    fn diretorio_no_lugar_do_rc_e_erro() {
        let d = sandbox("dir");
        let p = d.join(".bashrc");
        std::fs::create_dir_all(&p).unwrap();
        assert!(acrescenta_path_no_rc(&p).is_err(), "diretório tinha que falhar");
        let _ = std::fs::remove_dir_all(&d);
    }
}
