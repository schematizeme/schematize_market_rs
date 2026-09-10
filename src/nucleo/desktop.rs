//! NÚCLEO — a integração com o ambiente gráfico: ícone e entrada no menu de aplicativos.
//!
//! **O quê:** grava o `.desktop` e instala os ícones, para o app **aparecer na lista de
//! programas** e abrir sozinho — sem passar pelo schematize.
//!
//! **Onde:** `schematize-market desktop --install` / `--remover`, e o `install.sh` ao instalar o app.
//!
//! ## Por que o app instala a PRÓPRIA integração
//!
//! Se quem gravasse o `.desktop` do Deployer fosse o instalador do schematize, o Deployer
//! deixaria de ser instalável sozinho — e essa autonomia é a razão do ADR-0012. Quem instala
//! o binário direto do release, ou compila do fonte, tem de conseguir o ícone também.
//!
//! ## O ícone tem DUAS formas, e escolhe a melhor disponível
//!
//! **Com a janela instalada** (`schematize-market-gui`): `Exec=` aponta para ela e
//! `Terminal=false`. É o caso normal — o market é o único dos três apps que já tem janela,
//! e o ícone dele apontar para `list --wait` era um fio solto: a janela existia, era asset
//! do release, e o ícone não a usava. Ícone que só imprime é relatório com atalho, não
//! aplicativo.
//!
//! **Sem a janela:** cai em `list --wait` com `Terminal=true`, exatamente o que havia antes.
//! Um `.desktop` com `Terminal=false` apontando para uma CLI abriria um processo sem janela
//! que termina no mesmo instante — o clique não faria **nada**, que é pior que não ter ícone.
//! Com `Terminal=true`, o ambiente gráfico abre um terminal e roda o comando ali; e o
//! `--wait` é o que impede o terminal de fechar junto com o processo e a pessoa ver um
//! piscar. §37.48 — o software se adapta ao clique que a pessoa deu.
//!
//! **Por que a escolha é feita ao GRAVAR, e não ao clicar.** Um `Exec=` que decidisse na hora
//! precisaria de `Terminal=` fixo, e nenhum dos dois valores serve aos dois caminhos: com
//! `false` o fallback em terminal não teria terminal; com `true` a janela abriria com um
//! terminal preto pendurado atrás. Quem grava sabe o que existe na máquina, e regrava a cada
//! `desktop --install` — que o `install.sh` e o [`crate::appsdacasa::registrar_no_menu`]
//! chamam depois de instalar. Instalar a janela depois e não reabrir o menu é o custo, e ele
//! se paga na próxima atualização.

use std::path::{Path, PathBuf};

/// Nome do arquivo `.desktop` e do ícone. Um só lugar: se divergirem, o menu mostra um
/// quadrado cinza e ninguém liga a causa ao nome.
pub const ID: &str = "schematize-market";

/// Nome do executável da janela do market (`schematize_updater_gui_rs`, ADR-0014 D4).
///
/// O crate ainda se chama `schematize-updater-gui` por ser o endereço histórico; o BINÁRIO é
/// este. Nome de binário que não bate com o dono já deixou o update do deployer morto por um
/// release inteiro, e nada dá erro nesse caso — por isso o nome mora num lugar só.
pub const GUI_BIN: &str = "schematize-market-gui";

/// **O quê:** o caminho ABSOLUTO da janela do market, ou `None` se ela não estiver instalada.
///
/// **Onde:** [`instalar`], para decidir entre as duas formas do `.desktop`; e os testes.
///
/// **A ordem importa.** Primeiro ao lado do próprio binário do market: é onde o `install.sh`
/// põe os dois, e é o par que se atualiza junto. Só depois o `$PATH` e os diretórios de
/// fallback — que podem ter uma cópia velha de outra instalação.
pub fn resolver_gui(bin_do_market: &Path) -> Option<PathBuf> {
    let nomes: &[&str] =
        if cfg!(windows) { &["schematize-market-gui.exe", GUI_BIN] } else { &[GUI_BIN] };
    if let Some(dir) = bin_do_market.parent() {
        for n in nomes {
            let c = dir.join(n);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    super::bin::resolve_bin(GUI_BIN)
}

/// **O quê:** o conteúdo do `.desktop`, exatamente como vai para o disco. `gui` é o caminho da
/// janela quando ela existe; `None` produz a forma de terminal.
///
/// **Onde:** [`instalar`] e os testes. Função PURA — as duas formas são afirmáveis sem
/// escrever em `~/.local/share` e sem ter janela nenhuma instalada.
///
/// **`Exec` com caminho ABSOLUTO nas duas formas:** o lançador do desktop dá **PATH mínimo**
/// — não lê `~/.bashrc` nem `~/.profile` —, então `Exec=schematize-market` falharia em achar
/// o binário em `~/.cargo/bin`. É a mesma armadilha que já quebrou o lançador do schematize,
/// e ela vale igual para o caminho da janela.
pub fn render(bin: &Path, icone: &Path, gui: Option<&Path>) -> String {
    // A janela não recebe `--wait`: ela é uma janela, e fica aberta porque tem event loop.
    // A CLI recebe, senão o terminal fecha na cara de quem clicou.
    let (exec, terminal) = match gui {
        Some(g) => (format!("{}", g.display()), "false"),
        None => (format!("{} list --wait", bin.display()), "true"),
    };
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=schematize Market\n\
         GenericName=Instalar e remover programas\n\
         Comment=Linguagens, ferramentas de dev e os apps do ecossistema\n\
         Exec={exec}\n\
         Icon={}\n\
         Terminal={terminal}\n\
         Categories=Development;System;PackageManager;\n\
         Keywords=instalar;install;programa;linguagem;runtime;pacote;market;\n\
         StartupNotify=false\n",
        icone.display()
    )
}

/// **O quê:** onde os `.desktop` do usuário moram.
pub fn dir_apps(home: &Path) -> PathBuf {
    home.join(".local/share/applications")
}

/// **O quê:** o caminho do `.desktop` deste app.
pub fn arquivo_desktop(home: &Path) -> PathBuf {
    dir_apps(home).join(format!("{ID}.desktop"))
}

/// **O quê:** instala ícone + `.desktop`, e devolve o caminho do `.desktop`.
///
/// **Onde:** `schematize-market desktop --install` e o `install.sh`.
///
/// **`bin` é o caminho do próprio executável**, resolvido pelo chamador — gravar um caminho
/// adivinhado faria o ícone abrir outra coisa (ou nada) na máquina de quem instalou fora do
/// lugar padrão.
///
/// **A janela é procurada AQUI**, e não pelo chamador: quem chama (`desktop --install`, o
/// `install.sh`, o `registrar_no_menu`) não tem por que saber que existe uma janela. Se ela
/// estiver instalada, o ícone abre a janela; se não, o ícone segue abrindo a lista no
/// terminal, como sempre abriu. Nenhum dos dois caminhos deixa o ícone morto.
pub fn instalar(home: &Path, bin: &Path) -> Result<PathBuf, String> {
    instalar_com_gui(home, bin, resolver_gui(bin).as_deref())
}

/// **O quê:** o mesmo que [`instalar`], com a janela DADA em vez de procurada.
///
/// **Onde:** [`instalar`] em produção, e os testes — que precisam das duas formas sem
/// depender do que está instalado na máquina de quem roda a suíte. A primeira versão disto
/// procurava a janela aqui dentro, e o teste do fallback passou a falhar na máquina do
/// desenvolvedor **porque a janela estava instalada**: teste que muda de resultado com o
/// ambiente não prova nem uma coisa nem outra.
pub fn instalar_com_gui(home: &Path, bin: &Path, gui: Option<&Path>) -> Result<PathBuf, String> {
    // O ícone primeiro: um `.desktop` apontando para ícone inexistente vira quadrado cinza,
    // e a pessoa conclui que o app não instalou direito.
    let icone =
        super::icone::install_all(home).map_err(|e| format!("não consegui gerar o ícone: {e}"))?;
    let dir = dir_apps(home);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("não consegui criar {}: {e}", dir.display()))?;
    let p = arquivo_desktop(home);
    std::fs::write(&p, render(bin, &icone, gui))
        .map_err(|e| format!("não consegui gravar {}: {e}", p.display()))?;
    // Best-effort: alguns ambientes só releem o menu depois disto. Falhar aqui não invalida
    // a instalação — no pior caso o ícone aparece no próximo login.
    let _ = std::process::Command::new("update-desktop-database").arg(&dir).status();
    Ok(p)
}

/// **O quê:** remove o `.desktop` deste app. Devolve `true` se havia um.
///
/// **Onde:** `schematize-market desktop --remover`. Os ícones ficam: são inertes, e apagá-los sem
/// necessidade mexeria numa árvore (`hicolor`) compartilhada com outros apps.
pub fn remover(home: &Path) -> Result<bool, String> {
    let p = arquivo_desktop(home);
    if !p.exists() {
        return Ok(false);
    }
    std::fs::remove_file(&p).map_err(|e| format!("não consegui remover {}: {e}", p.display()))?;
    let _ = std::process::Command::new("update-desktop-database").arg(dir_apps(home)).status();
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A linha `Exec=` de um render, sem o prefixo.
    fn exec_de(t: &str) -> String {
        t.lines().find(|l| l.starts_with("Exec=")).expect("sem Exec= a entrada é ignorada")[5..]
            .to_string()
    }

    /// **O `Exec` tem de ser ABSOLUTO — nas DUAS formas.** O lançador do desktop dá PATH
    /// mínimo (não lê `~/.bashrc` nem `~/.profile`), então um `Exec=schematize-market` não
    /// acharia o binário em `~/.cargo/bin`. É a armadilha que já quebrou o lançador do
    /// schematize, e ela vale igual para o caminho da janela.
    #[test]
    fn o_exec_e_absoluto_com_e_sem_janela() {
        let bin = Path::new("/home/u/.cargo/bin/schematize-market");
        let gui = Path::new("/home/u/.cargo/bin/schematize-market-gui");

        let sem = exec_de(&render(bin, Path::new("/i/x.png"), None));
        assert!(sem.starts_with("/home/u/.cargo/bin/schematize-market "), "{sem}");

        let com = exec_de(&render(bin, Path::new("/i/x.png"), Some(gui)));
        assert_eq!(com, "/home/u/.cargo/bin/schematize-market-gui", "{com}");
        assert!(!com.starts_with("schematize"), "relativo depende do PATH do DE: {com}");
    }

    /// **A JANELA é o caminho normal — este é o fio solto que a F0 conserta.** A janela do
    /// market existe, é asset do release e foi testada; o ícone é que apontava para
    /// `list --wait`. Com ela instalada, o ícone abre a janela e `Terminal=false` — um
    /// `Terminal=true` aqui deixaria um terminal preto pendurado atrás da janela.
    #[test]
    fn com_a_janela_instalada_o_icone_abre_a_janela() {
        let t = render(
            Path::new("/b/schematize-market"),
            Path::new("/i/x.png"),
            Some(Path::new("/b/schematize-market-gui")),
        );
        assert!(t.contains("Terminal=false"), "janela com Terminal=true abre terminal atrás");
        let exec = exec_de(&t);
        assert!(exec.ends_with("schematize-market-gui"), "{exec}");
        assert!(!exec.contains("list"), "a janela não recebe subcomando: {exec}");
        assert!(!exec.contains("--wait"), "`--wait` é da CLI, e travaria a janela: {exec}");
    }

    /// **Sem a janela o ícone NÃO fica quebrado:** cai no `list --wait` em terminal, que é
    /// exatamente o que havia antes. Ícone que abre nada é pior que ícone que abre pouco
    /// (§37.48) — e é por isso que o fallback é um caso de primeira classe, não um erro.
    #[test]
    fn sem_a_janela_cai_no_terminal_e_nao_fecha_na_cara() {
        let t = render(Path::new("/b/schematize-market"), Path::new("/i/x.png"), None);
        assert!(t.contains("Terminal=true"), "CLI sem terminal não mostra nada");
        assert!(t.contains("--wait"), "sem isto o terminal fecha antes de a pessoa ler");
        assert!(!t.contains("-gui"), "sem janela instalada não se aponta para janela nenhuma");
    }

    /// O `.desktop` tem os campos que o menu exige — **nas duas formas**. Sem eles a entrada
    /// é ignorada em silêncio, que é o modo de falha mais difícil de diagnosticar.
    #[test]
    fn tem_os_campos_obrigatorios_nas_duas_formas() {
        let gui = PathBuf::from("/b/schematize-market-gui");
        for g in [None, Some(gui.as_path())] {
            let t = render(Path::new("/b/schematize-market"), Path::new("/i/x.png"), g);
            for campo in ["[Desktop Entry]", "Type=Application", "Name=", "Exec=", "Icon="] {
                assert!(t.contains(campo), "faltou {campo} (gui={g:?}):\n{t}");
            }
            // `Terminal=` sem valor é entrada malformada; o menu a descarta calado.
            assert!(
                t.contains("Terminal=true") || t.contains("Terminal=false"),
                "Terminal= tem de ter valor"
            );
        }
    }

    /// **A janela ao lado do market ganha do `$PATH`.** É o par que o `install.sh` põe junto
    /// e que se atualiza junto; uma cópia velha noutra pasta do `$PATH` conversaria com o
    /// gestor errado. Sem nenhuma das duas, `None` — e o `.desktop` cai no terminal.
    #[test]
    fn resolver_gui_prefere_a_janela_irma() {
        let h = std::env::temp_dir().join(format!("mkt-gui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(&h).unwrap();
        let bin = h.join("schematize-market");
        std::fs::write(&bin, b"#!/bin/sh\n").unwrap();

        // Sem a irmã: não pode inventar um caminho que não existe.
        if let Some(p) = resolver_gui(&bin) {
            assert!(p.is_file(), "só devolve o que existe: {}", p.display());
            assert_ne!(p, h.join(GUI_BIN), "a irmã não foi criada ainda");
        }

        // Com a irmã ao lado: é ela, e por caminho absoluto.
        let irma = h.join(GUI_BIN);
        std::fs::write(&irma, b"#!/bin/sh\n").unwrap();
        let achado = resolver_gui(&bin).expect("a janela está ao lado do binário");
        assert_eq!(achado, irma, "a irmã ao lado ganha do PATH");
        assert!(achado.is_absolute());

        let _ = std::fs::remove_dir_all(&h);
    }

    /// Instalar e remover deixa o diretório como estava.
    #[test]
    fn instalar_e_remover_volta_ao_estado_anterior() {
        let h = std::env::temp_dir().join(format!("mkt-desk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(&h).unwrap();

        // SEM janela: a forma de terminal, de ponta a ponta e não só no `render`.
        let p = instalar_com_gui(&h, Path::new("/b/schematize-market"), None).unwrap();
        assert!(p.exists());
        let txt = std::fs::read_to_string(&p).unwrap();
        assert!(txt.contains("schematize Market"));
        assert!(txt.contains("Exec=/b/schematize-market list --wait"), "{txt}");
        assert!(txt.contains("Terminal=true"), "{txt}");

        // COM janela: regravar troca a forma. É o que faz instalar a janela depois valer no
        // próximo `desktop --install`, em vez de deixar o ícone preso no terminal.
        let gui = PathBuf::from("/b/schematize-market-gui");
        let p2 = instalar_com_gui(&h, Path::new("/b/schematize-market"), Some(&gui)).unwrap();
        assert_eq!(p2, p, "é o mesmo arquivo, regravado");
        let txt = std::fs::read_to_string(&p).unwrap();
        assert!(txt.contains("Exec=/b/schematize-market-gui\n"), "{txt}");
        assert!(txt.contains("Terminal=false"), "{txt}");
        // O ícone tem de existir: `.desktop` apontando para ícone ausente vira quadrado cinza.
        assert!(
            h.join(".local/share/icons/hicolor/256x256/apps/schematize-market.png").exists(),
            "o ícone não foi gerado — o menu mostraria um quadrado cinza"
        );

        assert!(remover(&h).unwrap());
        assert!(!p.exists());
        assert!(!remover(&h).unwrap(), "remover o que não existe é false, não erro");
        let _ = std::fs::remove_dir_all(&h);
    }
}
