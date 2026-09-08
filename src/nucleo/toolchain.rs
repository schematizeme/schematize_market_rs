//! Bootstrap do que é preciso para COMPILAR do fonte: o toolchain de Rust e as libs de build.
//!
//! **O quê:** [`ensure_toolchain`] garante `cargo` (instalando o rustup se faltar) e
//! [`ensure_build_deps`] instala, por SO e por família de distro, o que a GUI (Slint) precisa
//! para linkar — X11/Wayland/GL/xcb/fontconfig no Linux, Command Line Tools no macOS, MSVC
//! Build Tools no Windows.
//!
//! **Onde:** [`crate::atualizar`], no caminho do fonte — o que roda quando não há binário
//! pré-compilado para a plataforma, ou quando o baixado não executa aqui.
//!
//! ## Procedência e por que é arquivo próprio
//!
//! Porte do `platform.rs` do `schematize_updater_rs` (ADR-0013), sob o D4 — mesmo
//! comportamento, mesmos pacotes, mesmas mensagens. Saiu para arquivo separado porque
//! `plataforma.rs` + este passavam do teto de 750 linhas por arquivo; a fronteira é natural:
//! ali mora o que o SO **é**, aqui o que se **instala** nele.
//!
//! ## O piso que este módulo cumpre (§37.48 — "prever macacos")
//!
//! Quem instala o app não tem de saber o que é rustup, PATH ou um pacote `-devel`. Toda
//! ausência aqui é tratada como algo que o software resolve; quando não dá, a mensagem diz o
//! comando exato e o que fazer depois — nunca "instale as dependências e tente de novo".

use super::{plataforma, util};
use plataforma::{LinuxFam, Os};
use std::path::PathBuf;

/// **O quê:** garante o Rust/cargo e devolve o caminho ABSOLUTO do `cargo`.
///
/// **Onde:** todo build do fonte, como primeiro passo.
///
/// **Por que o caminho absoluto e não o nome:** logo depois de o rustup instalar, o PATH
/// **deste processo** ainda não contém `~/.cargo/bin` — ele foi montado antes. Devolver
/// `"cargo"` faria o passo seguinte falhar com "comando não encontrado" logo após uma
/// instalação bem-sucedida, que é o tipo de contradição que faz a pessoa desistir.
pub fn ensure_toolchain() -> Result<PathBuf, String> {
    let cargo_abs = plataforma::install_dir().join(format!("cargo{}", plataforma::exe_suffix()));
    if super::bin::binary_in_path("cargo") {
        return Ok(PathBuf::from("cargo"));
    }
    if cargo_abs.is_file() {
        return Ok(cargo_abs);
    }
    println!("→ instalando Rust (rustup, perfil mínimo)…");
    match plataforma::os() {
        Os::Windows => instalar_rustup_windows()?,
        _ => {
            // Unix: o instalador oficial, não interativo, perfil mínimo.
            util::run_inherit(
                "sh",
                &["-c", "curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal"],
            )?;
        }
    }
    if cargo_abs.is_file() {
        Ok(cargo_abs)
    } else if super::bin::binary_in_path("cargo") {
        Ok(PathBuf::from("cargo"))
    } else {
        Err("o rustup instalou mas o `cargo` não apareceu. Reabra o terminal e rode de novo."
            .into())
    }
}

/// **O quê:** baixa e roda o `rustup-init.exe`. **Onde:** [`ensure_toolchain`] no Windows.
fn instalar_rustup_windows() -> Result<(), String> {
    let tmp = plataforma::state_dir().join("rustup-init.exe");
    let _ = std::fs::create_dir_all(plataforma::state_dir());
    if !super::rede::download("https://win.rustup.rs/x86_64", &tmp) {
        return Err("não consegui baixar o rustup-init.exe. \
                    Instale o Rust manualmente em https://rustup.rs"
            .into());
    }
    util::run_inherit(tmp.to_str().unwrap_or_default(), &["-y", "--profile", "minimal"])
}

/// **O quê:** instala as libs de BUILD necessárias para compilar a GUI (Slint) do fonte.
/// **Onde:** [`crate::atualizar`], antes do primeiro `cargo build`.
pub fn ensure_build_deps() -> Result<(), String> {
    match plataforma::os() {
        Os::Linux => ensure_build_deps_linux(),
        Os::Mac => ensure_build_deps_mac(),
        Os::Windows => ensure_build_deps_windows(),
    }
}

/// **O quê:** o prefixo de elevação — `"sudo"` ou vazio. **Onde:** [`pkg_install`].
///
/// Vazio quando já se é root (sudo em cima de root é ruído) ou quando não há `sudo` na
/// máquina — nesse caso o comando roda como está e o gerenciador de pacotes é quem diz que
/// falta permissão, com a mensagem dele, que é melhor que uma nossa adivinhando.
#[cfg(not(windows))]
fn sudo() -> &'static str {
    let is_root = util::capture("id", &["-u"]).map(|u| u == "0").unwrap_or(false);
    if is_root || !super::bin::binary_in_path("sudo") {
        ""
    } else {
        "sudo"
    }
}

/// **O quê:** roda um comando de gerenciador de pacotes, com elevação se preciso.
/// **Onde:** [`ensure_build_deps_linux`].
#[cfg(not(windows))]
fn pkg_install(mgr_args: &[&str]) -> Result<(), String> {
    let s = sudo();
    let palavras: Vec<&str> = if s.is_empty() {
        mgr_args.to_vec()
    } else {
        let mut v = vec![s];
        v.extend_from_slice(mgr_args);
        v
    };
    let (cmd, args) = palavras.split_first().ok_or("comando vazio")?;
    util::run_inherit(cmd, args)
}

/// Os pacotes de build por família de distro: `(obrigatórios, fontes)`.
///
/// **Onde:** [`ensure_build_deps_linux`] e o teste que confere a tabela.
///
/// **Por que uma função pura e não um `match` no meio da instalação:** o updater tinha as
/// quatro listas embutidas em `if`s dentro da função que instala, e nenhum teste as
/// alcançava — um pacote com nome errado só aparecia na máquina de quem tentasse compilar
/// naquela distro. As FONTES vão separadas porque são best-effort: sem elas a GUI abre, só
/// não desenha alguns alfabetos; falhar a instalação inteira por causa de fonte seria trocar
/// uma degradação por uma parada.
#[cfg(not(windows))]
fn pacotes_de_build(fam: LinuxFam) -> Option<(Vec<&'static str>, Vec<&'static str>)> {
    match fam {
        LinuxFam::Debian => Some((
            vec![
                "apt-get",
                "install",
                "-y",
                "build-essential",
                "pkg-config",
                "libx11-dev",
                "libxcursor-dev",
                "libxrandr-dev",
                "libxi-dev",
                "libxkbcommon-dev",
                "libwayland-dev",
                "libgl1-mesa-dev",
                "libxcb1-dev",
                "libxcb-render0-dev",
                "libxcb-shape0-dev",
                "libxcb-xfixes0-dev",
                "libfontconfig1-dev",
            ],
            vec![
                "apt-get",
                "install",
                "-y",
                "fonts-noto-core",
                "fonts-noto-cjk",
                "fonts-dejavu-core",
            ],
        )),
        LinuxFam::Rpm if super::bin::binary_in_path("zypper") => Some((
            vec![
                "zypper",
                "--non-interactive",
                "install",
                "-y",
                "gcc",
                "gcc-c++",
                "make",
                "pkg-config",
                "libX11-devel",
                "libXcursor-devel",
                "libXrandr-devel",
                "libXi-devel",
                "libxkbcommon-devel",
                "wayland-devel",
                "Mesa-libGL-devel",
                "libxcb-devel",
                "fontconfig-devel",
            ],
            vec![
                "zypper",
                "--non-interactive",
                "install",
                "-y",
                "noto-sans-fonts",
                "noto-sans-cjk-fonts",
                "dejavu-fonts",
            ],
        )),
        LinuxFam::Rpm => Some((
            vec![
                "dnf",
                "install",
                "-y",
                "gcc",
                "gcc-c++",
                "make",
                "pkg-config",
                "libX11-devel",
                "libXcursor-devel",
                "libXrandr-devel",
                "libXi-devel",
                "libxkbcommon-devel",
                "wayland-devel",
                "Mesa-libGL-devel",
                "libxcb-devel",
                "fontconfig-devel",
            ],
            vec![
                "dnf",
                "install",
                "-y",
                "google-noto-sans-fonts",
                "google-noto-sans-cjk-fonts",
                "dejavu-sans-fonts",
            ],
        )),
        LinuxFam::Arch => Some((
            vec![
                "pacman",
                "-S",
                "--needed",
                "--noconfirm",
                "base-devel",
                "pkgconf",
                "libx11",
                "libxcursor",
                "libxrandr",
                "libxi",
                "libxkbcommon",
                "wayland",
                "mesa",
                "libxcb",
                "fontconfig",
                "noto-fonts",
                "noto-fonts-cjk",
            ],
            // Arch já traz as fontes na lista principal — não há segundo passo.
            vec![],
        )),
        LinuxFam::Unknown => None,
    }
}

/// **O quê:** no-op — não há distro Linux para preparar quando o alvo é Windows.
/// **Onde:** [`ensure_build_deps`], pela variante da tabela de SO.
///
/// Existe para que o `match` de [`ensure_build_deps`] cubra os três SO em toda compilação:
/// um braço que só existe numa plataforma faria o código nem compilar nas outras.
#[cfg(windows)]
fn ensure_build_deps_linux() -> Result<(), String> {
    Ok(())
}

/// **O quê:** instala as deps do Slint na distro corrente. **Onde:** [`ensure_build_deps`].
#[cfg(not(windows))]
fn ensure_build_deps_linux() -> Result<(), String> {
    let fam = plataforma::linux_family();
    let Some((obrigatorios, fontes)) = pacotes_de_build(fam) else {
        return Err("distro Linux não reconhecida: instale manualmente as libs de \
                    X11/Wayland/GL/fontconfig (dev) e rode de novo."
            .into());
    };
    if fam == LinuxFam::Debian {
        // Índice desatualizado faz o `install` falhar por pacote "inexistente" que existe.
        // Best-effort: se o update falhar (rede), o install ainda pode achar tudo em cache.
        let _ = pkg_install(&["apt-get", "update", "-qq"]);
    }
    pkg_install(&obrigatorios)?;
    if !fontes.is_empty() {
        // Fontes de cobertura ampla (alfabetos não-latinos) — best-effort de propósito: sem
        // elas a GUI abre, só não desenha alguns scripts. Parar a instalação por causa disso
        // trocaria uma degradação visível por uma falha total.
        let _ = pkg_install(&fontes);
    }
    Ok(())
}

/// **O quê:** no-op — não há macOS para preparar quando o alvo é Windows.
/// **Onde:** [`ensure_build_deps`]. Mesmo motivo do irmão acima.
#[cfg(windows)]
fn ensure_build_deps_mac() -> Result<(), String> {
    Ok(())
}

/// **O quê:** garante as Command Line Tools do Xcode. **Onde:** [`ensure_build_deps`] no macOS.
///
/// Slint no Mac usa frameworks do sistema — não há brew envolvido. O `xcode-select --install`
/// abre o instalador gráfico e retorna na hora, então o passo seguinte é da pessoa: por isso
/// devolve `Err` com a instrução, em vez de fingir que já deu certo.
#[cfg(not(windows))]
fn ensure_build_deps_mac() -> Result<(), String> {
    let clt_ok = util::capture("xcode-select", &["-p"]).map(|p| !p.is_empty()).unwrap_or(false);
    if !clt_ok {
        println!("→ instalando as Command Line Tools do Xcode (aceite o popup)…");
        let _ = util::run_inherit("xcode-select", &["--install"]);
        return Err(
            "conclua a instalação das Command Line Tools do Xcode (popup) e rode de novo.".into()
        );
    }
    Ok(())
}

/// **O quê:** no-op — não há MSVC para preparar fora do Windows.
/// **Onde:** [`ensure_build_deps`]. Mesmo motivo dos irmãos acima.
#[cfg(not(windows))]
fn ensure_build_deps_windows() -> Result<(), String> {
    Ok(())
}

/// **O quê:** garante o toolchain MSVC (`link.exe`). **Onde:** [`ensure_build_deps`] no Windows.
#[cfg(windows)]
fn ensure_build_deps_windows() -> Result<(), String> {
    if super::bin::binary_in_path("link") || super::bin::binary_in_path("cl") {
        return Ok(());
    }
    if super::bin::binary_in_path("winget") {
        println!("→ instalando o Visual Studio Build Tools (MSVC) via winget…");
        let _ = util::run_inherit(
            "winget",
            &[
                "install",
                "--id",
                "Microsoft.VisualStudio.2022.BuildTools",
                "-e",
                "--accept-source-agreements",
                "--accept-package-agreements",
                "--override",
                "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools",
            ],
        );
        return Err(
            "instalei o MSVC Build Tools — REABRA o terminal (para o PATH pegar) e rode de novo."
                .into(),
        );
    }
    Err("no Windows a compilação do fonte precisa do MSVC Build Tools. Instale o \
         'Visual Studio Build Tools' (workload C++) e rode de novo, ou aguarde um binário \
         pré-compilado."
        .into())
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    /// **O que trava:** toda família RECONHECIDA tem lista de pacotes, e a desconhecida NÃO
    /// tem — porque instalar pacote com o gerenciador errado é pior que dizer "não reconheci".
    #[test]
    fn familia_reconhecida_tem_pacotes_e_desconhecida_nao() {
        for fam in [LinuxFam::Debian, LinuxFam::Rpm, LinuxFam::Arch] {
            let p = pacotes_de_build(fam);
            assert!(p.is_some(), "{fam:?} ficou sem lista de pacotes");
            let (obrig, _) = p.unwrap();
            assert!(obrig.len() > 5, "{fam:?} com lista suspeitamente curta: {obrig:?}");
        }
        assert!(pacotes_de_build(LinuxFam::Unknown).is_none());
    }

    /// **O que trava:** as libs que o Slint precisa para LINKAR estão em toda família. Um
    /// nome que falte aqui vira um erro de linker no fim de um build de vinte minutos — o
    /// pior momento possível para descobrir que faltava um pacote.
    #[test]
    fn toda_familia_cobre_as_libs_que_o_slint_linka() {
        // O mesmo grupo em cada convenção de nome de distro.
        let grupos: [&[&str]; 6] = [
            &["libx11-dev", "libX11-devel", "libx11"],
            &["libxkbcommon-dev", "libxkbcommon-devel", "libxkbcommon"],
            &["libwayland-dev", "wayland-devel", "wayland"],
            &["libgl1-mesa-dev", "Mesa-libGL-devel", "mesa"],
            &["libxcb1-dev", "libxcb-devel", "libxcb"],
            &["libfontconfig1-dev", "fontconfig-devel", "fontconfig"],
        ];
        for fam in [LinuxFam::Debian, LinuxFam::Rpm, LinuxFam::Arch] {
            let (obrig, _) = pacotes_de_build(fam).unwrap();
            for grupo in grupos {
                assert!(
                    grupo.iter().any(|p| obrig.contains(p)),
                    "{fam:?} não cobre nenhum de {grupo:?} — vira erro de linker no fim do build"
                );
            }
        }
    }

    /// O primeiro token de cada lista é o GERENCIADOR — é o que o `pkg_install` separa como
    /// comando. Uma lista que comece com um pacote faria o shell tentar executar o pacote.
    #[test]
    fn a_lista_comeca_pelo_gerenciador() {
        for fam in [LinuxFam::Debian, LinuxFam::Rpm, LinuxFam::Arch] {
            let (obrig, fontes) = pacotes_de_build(fam).unwrap();
            assert!(
                matches!(obrig[0], "apt-get" | "zypper" | "dnf" | "pacman"),
                "{fam:?} não começa por um gerenciador: {:?}",
                obrig[0]
            );
            if !fontes.is_empty() {
                assert_eq!(fontes[0], obrig[0], "{fam:?}: fontes com gerenciador diferente");
            }
        }
    }
}
