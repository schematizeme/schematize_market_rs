//! Pós-instalação do app `schematize`: PATH e lançador de menu, pelas costuras.
//!
//! **O quê:** [`garantir_path`] põe `~/.cargo/bin` no PATH do usuário (`plat.path`) e
//! [`criar_lancador`] descreve o atalho de menu da GUI do hub e o entrega a `plat.atalhos`.
//! Antes (ADR-0026) eram `plataforma::ensure_path_setup` e `plataforma::make_launcher`, com o
//! PowerShell/`.desktop` embutidos; agora o SO é da costura.
//!
//! **Onde:** o fim de toda instalação/atualização do app ([`crate::atualizar::componentes`]).

use super::{plataforma, util};
use crate::ctx::Ctx;
use plataforma::Os;
use schematize_plataforma::seams::atalhos::EntradaAtalho;
use schematize_plataforma::seams::pathsetup::PathSetup;
use std::path::Path;

/// **O quê:** garante que [`plataforma::install_dir`] esteja no PATH, para o app rodar do
/// terminal. Idempotente. Erro nunca engolido (piso 4): o usuário precisa saber que o PATH
/// não pegou, senão o sintoma vira "instalei e o comando não existe".
/// **Onde:** o fim de toda instalação.
pub fn garantir_path(ctx: &Ctx) {
    garantir_path_com(&*ctx.plat.path)
}

/// **O quê:** [`garantir_path`] com a costura dada. **Onde:** [`garantir_path`] e os testes.
fn garantir_path_com(path: &dyn PathSetup) {
    if let Err(e) = path.garantir_no_path(&plataforma::install_dir()) {
        eprintln!("aviso: {e}");
    }
}

/// **O quê:** descreve o atalho de menu da GUI do hub (`schematize-gui`). PURA.
/// **Onde:** [`criar_lancador`] e os testes.
///
/// `Exec` ABSOLUTO — o bug do lançador que abria o binário errado do PATH do ambiente gráfico.
/// O `StartupWMClass` bate com o app_id que o binário anuncia (a GUI o deriva do próprio
/// nome), senão o dock não casa a janela com o ícone. `legados`: o lançador do INTERREGNO
/// (nome Overflow) que NÓS escrevemos sai — um app, uma entrada no menu.
pub fn entrada_do_hub(
    home: &Path,
    guibin: &Path,
    gui_nome: &str,
    icones: Vec<(u32, Vec<u8>)>,
) -> EntradaAtalho {
    EntradaAtalho {
        home: home.to_path_buf(),
        id: "schematize-gui".into(),
        nome: "schematize".into(),
        generico: Some("Skills, overdev e mais".into()),
        comentario: Some("Skills, overdev e mais — schematize".into()),
        bin: None,
        args_sem_gui: String::new(),
        gui: Some(guibin.to_path_buf()),
        categorias: ["Development", "Utility"].map(String::from).to_vec(),
        palavras_chave: ["schematize", "skills", "overdev", "claude"].map(String::from).to_vec(),
        startup_notify: None,
        startup_wm_class: Some(gui_nome.to_string()),
        icone_nome: "schematize".into(),
        icones,
        icone_extras: false,
        icone_obrigatorio: false,
        cache_icones: true,
        legados: vec!["overflow-gui".into()],
    }
}

/// **O quê:** cria o lançador da GUI do hub (best-effort: a falha é DITA, nunca derruba a
/// instalação). No macOS não há `.desktop`: não faz nada.
///
/// **Onde:** o fim de toda instalação do app.
///
/// O ícone vem do próprio CLI (`schematize icon --hicolor`), que o gera A PARTIR DO CÓDIGO
/// antes do atalho — `make_launcher` roda em TODO update e antes regravava o `.desktop` SEM
/// `Icon=`, e o dock (Wayland) perdia o ícone. Os bytes do PNG de 256px são lidos do disco e
/// entregues à costura, que os regrava no mesmo lugar e aponta o `Icon=` para eles.
pub fn criar_lancador(ctx: &Ctx) {
    if plataforma::os() == Os::Mac {
        return;
    }
    let (cli, gui) = plataforma::bin_names();
    let guibin = plataforma::install_dir().join(&gui);
    if !guibin.is_file() {
        return;
    }
    let home = util::home();
    let icons_dir = home.join(".local/share/icons/hicolor");
    let szbin = plataforma::install_dir().join(&cli);
    let _ = util::capture(
        szbin.to_str().unwrap_or("schematize"),
        &["icon", "--hicolor", icons_dir.to_str().unwrap_or_default()],
    );
    let png256 = icons_dir.join("256x256").join("apps").join("schematize.png");
    let icones = std::fs::read(&png256).map(|b| vec![(256, b)]).unwrap_or_default();
    if let Err(e) = ctx.plat.atalhos.instalar(&entrada_do_hub(&home, &guibin, &gui, icones)) {
        eprintln!("aviso: não consegui gravar o lançador: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schematize_plataforma::seams::atalhos::render;
    use std::sync::Mutex;

    /// PathSetup falso que registra o diretório pedido (e pode falhar).
    struct PathFalso(Mutex<Vec<String>>, bool);
    impl PathSetup for PathFalso {
        fn garantir_no_path(&self, dir: &Path) -> Result<(), String> {
            self.0.lock().unwrap().push(dir.display().to_string());
            if self.1 {
                Err("rc ilegível".into())
            } else {
                Ok(())
            }
        }
        fn orientacao(
            &self,
            _: &Path,
        ) -> Option<schematize_plataforma::seams::pathsetup::Orientacao> {
            None
        }
    }

    /// O PATH garantido é o diretório de instalação do market, mesmo quando a costura falha
    /// (a falha vai ao stderr, não derruba nada).
    #[test]
    fn garantir_path_pede_o_diretorio_de_instalacao() {
        let p = PathFalso(Mutex::new(vec![]), false);
        garantir_path_com(&p);
        let ruim = PathFalso(Mutex::new(vec![]), true);
        garantir_path_com(&ruim);
        let esperado = plataforma::install_dir().display().to_string();
        assert_eq!(*p.0.lock().unwrap(), vec![esperado.clone()]);
        assert_eq!(*ruim.0.lock().unwrap(), vec![esperado]);
    }

    /// **O `.desktop` do hub é BYTE A BYTE o que `make_launcher` escrevia** antes da costura.
    #[test]
    fn o_desktop_do_hub_e_identico_ao_de_antes() {
        let guibin = Path::new("/h/.cargo/bin/schematize-gui");
        let e = entrada_do_hub(Path::new("/h"), guibin, "schematize-gui", vec![]);
        let icon = "/h/.local/share/icons/hicolor/256x256/apps/schematize.png";
        let antes = format!(
            "[Desktop Entry]\nType=Application\nName=schematize\nGenericName=Skills, overdev e mais\n\
             Comment=Skills, overdev e mais — schematize\nExec={}\nIcon={icon}\nTerminal=false\n\
             Categories=Development;Utility;\nKeywords=schematize;skills;overdev;claude;\n\
             StartupWMClass={}\n",
            guibin.display(),
            "schematize-gui"
        );
        assert_eq!(render(&e, icon).unwrap(), antes);
        assert_eq!(e.legados, vec!["overflow-gui".to_string()]);
    }
}
