//! NÚCLEO — a integração com o ambiente gráfico: ícone e entrada no menu de aplicativos.
//!
//! **O quê:** grava o `.desktop` e instala os ícones, para o app **aparecer na lista de
//! programas** e abrir sozinho — sem passar pelo schematize.
//!
//! **Onde:** `schematize-market desktop --install` / `--remover`, e o `install.sh` ao instalar o app.
//!
//! **ADR-0026 (F4a):** o market só DESCREVE o atalho ([`entrada`]: textos, categorias, os PNGs que
//! ele mesmo desenha) e entrega à costura `plat.atalhos`, que sabe onde e como gravá-lo no SO.
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

use crate::ctx::Ctx;
use schematize_plataforma::seams::atalhos::{resolver_gui_irma, EntradaAtalho};
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
    resolver_gui_irma(bin_do_market, GUI_BIN).or_else(|| super::bin::resolve_bin(GUI_BIN))
}

/// **O quê:** o caminho do `.desktop` deste app (convenção freedesktop, sob `home`).
/// **Onde:** `desktop` sem flags (relato de estado) e os testes.
pub fn arquivo_desktop(home: &Path) -> PathBuf {
    home.join(".local/share/applications").join(format!("{ID}.desktop"))
}

/// **O quê:** descreve o atalho do market para a costura: `gui` é a janela quando ela existe
/// (`Terminal=false`); `None` produz a forma de terminal (`list --wait`, `Terminal=true`). Os
/// ícones são desenhados AQUI, em todos os tamanhos hicolor.
///
/// **Onde:** [`instalar_com_gui`], [`remover`] e os testes.
///
/// **`bin` e `gui` com caminho ABSOLUTO:** o lançador do desktop dá **PATH mínimo** — não lê
/// `~/.bashrc` nem `~/.profile` —, então `Exec=schematize-market` falharia em achar o binário
/// em `~/.cargo/bin`. É a armadilha que já quebrou o lançador do schematize.
///
/// **`list --wait` só na CLI:** a janela é janela e fica aberta pelo event loop; a CLI sem
/// `--wait` fecharia o terminal na cara de quem clicou.
pub fn entrada(
    home: &Path,
    bin: &Path,
    gui: Option<&Path>,
    icones: Vec<(u32, Vec<u8>)>,
) -> EntradaAtalho {
    EntradaAtalho {
        home: home.to_path_buf(),
        id: ID.into(),
        nome: "schematize Market".into(),
        generico: Some("Instalar e remover programas".into()),
        comentario: Some("Linguagens, ferramentas de dev e os apps do ecossistema".into()),
        bin: Some(bin.to_path_buf()),
        args_sem_gui: "list --wait".into(),
        gui: gui.map(Path::to_path_buf),
        categorias: ["Development", "System", "PackageManager"].map(String::from).to_vec(),
        palavras_chave: ["instalar", "install", "programa", "linguagem", "runtime", "pacote"]
            .into_iter()
            .chain(["market"])
            .map(String::from)
            .collect(),
        startup_notify: Some(false),
        startup_wm_class: None,
        icone_nome: ID.into(),
        icones,
        icone_extras: true,
        icone_obrigatorio: true,
        cache_icones: false,
        legados: vec![],
    }
}

/// **O quê:** instala ícone + `.desktop` pela costura e devolve o caminho do `.desktop`.
///
/// **Onde:** `schematize-market desktop --install` e o `install.sh`.
///
/// **`bin` é o caminho do próprio executável**, resolvido pelo chamador — gravar um caminho
/// adivinhado faria o ícone abrir outra coisa (ou nada) na máquina de quem instalou fora do
/// lugar padrão.
///
/// **A janela é procurada AQUI**, e não pelo chamador: se estiver instalada, o ícone abre a
/// janela; se não, segue abrindo a lista no terminal. Nenhum dos dois caminhos deixa o ícone
/// morto.
pub fn instalar(ctx: &Ctx, home: &Path, bin: &Path) -> Result<PathBuf, String> {
    instalar_com_gui(ctx, home, bin, resolver_gui(bin).as_deref())
}

/// **O quê:** o mesmo que [`instalar`], com a janela DADA em vez de procurada.
///
/// **Onde:** [`instalar`] em produção, e os testes — que precisam das duas formas sem
/// depender do que está instalado na máquina de quem roda a suíte (teste que muda de
/// resultado com o ambiente não prova nem uma coisa nem outra).
pub fn instalar_com_gui(
    ctx: &Ctx,
    home: &Path,
    bin: &Path,
    gui: Option<&Path>,
) -> Result<PathBuf, String> {
    // O ícone primeiro (a costura grava ícones antes do `.desktop`): um `.desktop` apontando
    // para ícone inexistente vira quadrado cinza.
    let icones =
        super::icone::icones_hicolor().map_err(|e| format!("não consegui gerar o ícone: {e}"))?;
    ctx.plat.atalhos.instalar(&entrada(home, bin, gui, icones))?;
    Ok(arquivo_desktop(home))
}

/// **O quê:** remove o atalho pela costura. Devolve `true` se havia um.
///
/// **Onde:** `schematize-market desktop --remover`. Os ícones ficam: são inertes, e apagá-los
/// sem necessidade mexeria numa árvore (`hicolor`) compartilhada com outros apps.
pub fn remover(ctx: &Ctx, home: &Path) -> Result<bool, String> {
    let havia = arquivo_desktop(home).exists();
    ctx.plat.atalhos.remover(&entrada(home, Path::new("/"), None, vec![]))?;
    Ok(havia)
}

#[cfg(test)]
mod tests {
    use super::*;
    use schematize_plataforma::seams::atalhos::render;

    /// O texto do `.desktop` de uma entrada do market, como a costura o grava.
    fn texto(bin: &Path, icone: &str, gui: Option<&Path>) -> String {
        render(&entrada(Path::new("/h"), bin, gui, vec![]), icone).expect("entrada válida")
    }

    /// A linha `Exec=` de um render, sem o prefixo.
    fn exec_de(t: &str) -> String {
        t.lines().find(|l| l.starts_with("Exec=")).expect("sem Exec= a entrada é ignorada")[5..]
            .to_string()
    }

    /// **O `.desktop` do market é BYTE A BYTE o de antes do ADR-0026** (forma de terminal e
    /// forma de janela). A costura passou a escrevê-lo; o texto não pode ter mudado.
    #[test]
    fn o_desktop_e_identico_ao_de_antes_da_costura() {
        let bin = Path::new("/h/.cargo/bin/schematize-market");
        let gui = Path::new("/h/.cargo/bin/schematize-market-gui");
        let comum = "Type=Application\nName=schematize Market\n\
                     GenericName=Instalar e remover programas\n\
                     Comment=Linguagens, ferramentas de dev e os apps do ecossistema\n";
        let fim = "Categories=Development;System;PackageManager;\n\
                   Keywords=instalar;install;programa;linguagem;runtime;pacote;market;\n\
                   StartupNotify=false\n";
        assert_eq!(
            texto(bin, "/i/x.png", None),
            format!(
                "[Desktop Entry]\n{comum}Exec=/h/.cargo/bin/schematize-market list --wait\n\
                 Icon=/i/x.png\nTerminal=true\n{fim}"
            )
        );
        assert_eq!(
            texto(bin, "/i/x.png", Some(gui)),
            format!(
                "[Desktop Entry]\n{comum}Exec=/h/.cargo/bin/schematize-market-gui\n\
                 Icon=/i/x.png\nTerminal=false\n{fim}"
            )
        );
    }

    /// **O `Exec` tem de ser ABSOLUTO — nas DUAS formas** (PATH mínimo do lançador do desktop).
    #[test]
    fn o_exec_e_absoluto_com_e_sem_janela() {
        let bin = Path::new("/home/u/.cargo/bin/schematize-market");
        let gui = Path::new("/home/u/.cargo/bin/schematize-market-gui");
        let sem = exec_de(&texto(bin, "/i/x.png", None));
        assert!(sem.starts_with("/home/u/.cargo/bin/schematize-market "), "{sem}");
        let com = exec_de(&texto(bin, "/i/x.png", Some(gui)));
        assert_eq!(com, "/home/u/.cargo/bin/schematize-market-gui", "{com}");
        assert!(!com.contains("--wait"), "`--wait` é da CLI, e travaria a janela: {com}");
    }

    /// Sem a janela o ícone NÃO fica quebrado: cai no `list --wait` em terminal.
    #[test]
    fn sem_a_janela_cai_no_terminal_e_nao_fecha_na_cara() {
        let t = texto(Path::new("/b/schematize-market"), "/i/x.png", None);
        assert!(t.contains("Terminal=true") && t.contains("--wait"), "{t}");
        assert!(!t.contains("-gui"), "sem janela não se aponta para janela nenhuma: {t}");
    }

    /// O `.desktop` tem os campos que o menu exige — nas duas formas.
    #[test]
    fn tem_os_campos_obrigatorios_nas_duas_formas() {
        let gui = PathBuf::from("/b/schematize-market-gui");
        for g in [None, Some(gui.as_path())] {
            let t = texto(Path::new("/b/schematize-market"), "/i/x.png", g);
            for campo in ["[Desktop Entry]", "Type=Application", "Name=", "Exec=", "Icon="] {
                assert!(t.contains(campo), "faltou {campo} (gui={g:?}):\n{t}");
            }
            assert!(t.contains("Terminal=true") || t.contains("Terminal=false"));
        }
    }

    /// A janela ao lado do market ganha do `$PATH`; sem ela, `None`/só o que existe.
    #[test]
    fn resolver_gui_prefere_a_janela_irma() {
        let h = std::env::temp_dir().join(format!("mkt-gui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(&h).unwrap();
        let bin = h.join("schematize-market");
        std::fs::write(&bin, b"#!/bin/sh\n").unwrap();
        if let Some(p) = resolver_gui(&bin) {
            assert!(p.is_file(), "só devolve o que existe: {}", p.display());
            assert_ne!(p, h.join(GUI_BIN), "a irmã não foi criada ainda");
        }
        let irma = h.join(GUI_BIN);
        std::fs::write(&irma, b"#!/bin/sh\n").unwrap();
        let achado = resolver_gui(&bin).expect("a janela está ao lado do binário");
        assert_eq!(achado, irma, "a irmã ao lado ganha do PATH");
        let _ = std::fs::remove_dir_all(&h);
    }

    /// Instalar e remover pela costura REAL deixa o diretório como estava, e regravar troca a
    /// forma (terminal -> janela).
    #[cfg(unix)]
    #[test]
    fn instalar_e_remover_volta_ao_estado_anterior() {
        let ctx = Ctx::de_teste();
        let h = std::env::temp_dir().join(format!("mkt-desk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(&h).unwrap();

        let p = instalar_com_gui(&ctx, &h, Path::new("/b/schematize-market"), None).unwrap();
        let txt = std::fs::read_to_string(&p).unwrap();
        assert!(txt.contains("Exec=/b/schematize-market list --wait"), "{txt}");
        assert!(txt.contains("Terminal=true"), "{txt}");

        let gui = PathBuf::from("/b/schematize-market-gui");
        let p2 = instalar_com_gui(&ctx, &h, Path::new("/b/schematize-market"), Some(&gui)).unwrap();
        assert_eq!(p2, p, "é o mesmo arquivo, regravado");
        let txt = std::fs::read_to_string(&p).unwrap();
        assert!(txt.contains("Exec=/b/schematize-market-gui\n"), "{txt}");
        assert!(txt.contains("Terminal=false"), "{txt}");
        // O ícone tem de existir e ser o `Icon=`: `.desktop` apontando para ícone ausente vira
        // quadrado cinza.
        let png = h.join(".local/share/icons/hicolor/256x256/apps/schematize-market.png");
        assert!(png.exists(), "o ícone não foi gerado — o menu mostraria um quadrado cinza");
        assert!(txt.contains(&format!("Icon={}", png.display())), "{txt}");

        assert!(remover(&ctx, &h).unwrap());
        assert!(!p.exists());
        assert!(!remover(&ctx, &h).unwrap(), "remover o que não existe é false, não erro");
        let _ = std::fs::remove_dir_all(&h);
    }
}
