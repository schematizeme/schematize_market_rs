//! Detecção do ambiente para o engine de environments.
//! O quê: família da distro (via /etc/os-release, como o install.sh), presença de
//! `mise`/`docker`, e se um runtime já está instalado (binário no PATH / mise / imagem).
//! Onde: consumido por `environments::mod` pra decidir métodos disponíveis e idempotência.

use crate::util;

/// Família de distribuição — decide o gerenciador de pacotes do método `distro`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    /// apt/apt-get (Debian, Ubuntu, Mint, Pop!_OS…).
    Debian,
    /// dnf (Fedora, RHEL, Rocky, Alma…).
    Fedora,
    /// zypper (openSUSE, SLES).
    Suse,
    /// pacman (Arch, Manjaro, EndeavourOS, CachyOS…).
    Arch,
    /// `pkg` (FreeBSD). Não é Linux: não há `/etc/os-release` garantido, e a elevação é
    /// `sudo` OU `doas` (ver [`escalador`]).
    FreeBsd,
    /// não classificada — o método `distro` fica indisponível (deny-by-default).
    Unknown,
}

impl Family {
    /// Rótulo curto pra logs/tabela.
    pub fn label(self) -> &'static str {
        match self {
            Family::Debian => "debian",
            Family::Fedora => "fedora",
            Family::Suse => "suse",
            Family::Arch => "arch",
            Family::FreeBsd => "freebsd",
            Family::Unknown => "unknown",
        }
    }
}

/// **O quê:** a família desta máquina. No FreeBSD é decidida pelo SO compilado (`cfg`), não por
/// arquivo: ele não garante `/etc/os-release`. Nos demais, lê o os-release (vazio se ausente).
/// **Onde:** `Machine::probe` e a montagem de todo plano.
pub fn family() -> Family {
    if cfg!(target_os = "freebsd") {
        return Family::FreeBsd;
    }
    family_from(&std::fs::read_to_string("/etc/os-release").unwrap_or_default())
}

/// Remove aspas e espaços de um valor de os-release (`ID="fedora"` → `fedora`).
fn unquote(v: &str) -> String {
    v.trim().trim_matches('"').trim_matches('\'').to_string()
}

/// Classifica a família a partir do CONTEÚDO de um os-release — função pura, testável
/// com um arquivo falso. Espelha o `case` do install.sh (ID + ID_LIKE).
pub fn family_from(os_release: &str) -> Family {
    let mut tokens: Vec<String> = Vec::new();
    for line in os_release.lines() {
        let line = line.trim();
        for key in ["ID=", "ID_LIKE="] {
            if let Some(v) = line.strip_prefix(key) {
                for t in unquote(v).split_whitespace() {
                    tokens.push(t.to_lowercase());
                }
            }
        }
    }
    let has = |k: &str| tokens.iter().any(|t| t == k);
    // FreeBSD publica `ID=freebsd` quando há os-release; vem antes de tudo (nenhuma outra
    // família o cita em ID_LIKE).
    if has("freebsd") {
        return Family::FreeBsd;
    }
    // A ORDEM importa e a razão é concreta: derivadas declaram `ID_LIKE` da mãe, e algumas
    // declaram MAIS DE UMA. Testar a família específica antes da genérica evita classificar
    // um openSUSE como Fedora só porque os dois dizem `rhel`-ish em algum campo.
    if has("debian") || has("ubuntu") || has("linuxmint") || has("pop") || has("raspbian") {
        return Family::Debian;
    }
    if has("suse")
        || has("opensuse")
        || has("sles")
        || has("opensuse-leap")
        || has("opensuse-tumbleweed")
    {
        return Family::Suse;
    }
    if has("fedora") || has("rhel") || has("centos") || has("rocky") || has("almalinux") {
        return Family::Fedora;
    }
    if has("arch")
        || has("archlinux")
        || has("manjaro")
        || has("endeavouros")
        || has("cachyos")
        || has("garuda")
    {
        return Family::Arch;
    }
    Family::Unknown
}

/// **O quê:** decide o prefixo de elevação a partir de fatos já medidos. PURA.
/// `Some("")` = já é root (sem prefixo); `Some("sudo"|"doas")` = o escalador a usar (`sudo` tem
/// preferência); `None` = sem root e sem escalador — o plano recusa com orientação.
/// **Onde:** [`escalador`] (mede os fatos) e os testes.
pub fn escalador_de(is_root: bool, tem_sudo: bool, tem_doas: bool) -> Option<&'static str> {
    if is_root {
        Some("")
    } else if tem_sudo {
        Some("sudo")
    } else if tem_doas {
        Some("doas")
    } else {
        None
    }
}

/// **O quê:** mede root/`sudo`/`doas` nesta máquina e devolve o prefixo (ver [`escalador_de`]).
/// **Onde:** os planos de FreeBSD (`pkg` exige root; o FreeBSD base traz `doas` opcional e
/// `sudo` só por pacote, então nenhum dos dois pode ser presumido).
pub fn escalador() -> Option<&'static str> {
    let is_root = util::capture("id", &["-u"]).map(|u| u == "0").unwrap_or(false);
    escalador_de(
        is_root,
        crate::nucleo::bin::binary_in_path("sudo"),
        crate::nucleo::bin::binary_in_path("doas"),
    )
}

/// Um binário está no PATH? (usa `command -v` num shell de login.)
pub fn has_bin(bin: &str) -> bool {
    util::run(util::login_shell(), &["-lc", &format!("command -v {bin}")])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// `mise` está instalado nesta máquina?
pub fn has_mise() -> bool {
    has_bin("mise")
}

/// `docker` está instalado nesta máquina?
pub fn has_docker() -> bool {
    has_bin("docker")
}

/// O `mise` gerencia (tem instalado) a ferramenta `tool` (ex.: "go", "node")?
pub fn mise_has(tool: &str) -> bool {
    util::run(util::login_shell(), &["-lc", &format!("mise ls --installed {tool} 2>/dev/null")])
        .map(|s| s.lines().any(|l| !l.trim().is_empty()))
        .unwrap_or(false)
}

/// A imagem docker `img` já está presente localmente? (`docker image inspect`.)
pub fn docker_image_present(img: &str) -> bool {
    util::run("docker", &["image", "inspect", img]).is_ok()
}

#[cfg(test)]
mod tests_familias {
    use super::*;

    fn osr(id: &str, like: &str) -> String {
        if like.is_empty() {
            format!("ID={id}\n")
        } else {
            format!("ID={id}\nID_LIKE=\"{like}\"\n")
        }
    }

    /// **As quatro famílias que o pedido nomeou**, cada uma com o gerenciador certo.
    /// Antes Fedora e SUSE eram a MESMA `Family::Rpm`, e o comando tinha de escolher em
    /// runtime com um `if command -v zypper` — que o usuário via no plano de consentimento
    /// sem saber qual metade rodaria.
    #[test]
    fn as_quatro_familias_principais() {
        assert_eq!(family_from(&osr("debian", "")), Family::Debian);
        assert_eq!(family_from(&osr("ubuntu", "debian")), Family::Debian);
        assert_eq!(family_from(&osr("fedora", "")), Family::Fedora);
        assert_eq!(family_from(&osr("opensuse-leap", "suse opensuse")), Family::Suse);
        assert_eq!(family_from(&osr("arch", "")), Family::Arch);
    }

    /// FreeBSD é família própria (`pkg`), nunca cai em Unknown nem em Linux.
    #[test]
    fn freebsd_e_familia_propria() {
        assert_eq!(family_from("NAME=FreeBSD\nID=freebsd\nVERSION_ID=\"14.2\"\n"), Family::FreeBsd);
        assert_eq!(Family::FreeBsd.label(), "freebsd");
    }

    /// Elevação: root não prefixa; sudo vence doas; sem nenhum, `None` (o plano recusa).
    #[test]
    fn escalador_prefere_root_depois_sudo_depois_doas() {
        assert_eq!(escalador_de(true, true, true), Some(""));
        assert_eq!(escalador_de(false, true, true), Some("sudo"));
        assert_eq!(escalador_de(false, false, true), Some("doas"));
        assert_eq!(escalador_de(false, false, false), None);
    }

    /// **As derivadas** — é o que "e derivados" significa na prática.
    #[test]
    fn derivadas_caem_na_familia_da_mae() {
        for (id, like, esperado) in [
            ("linuxmint", "ubuntu debian", Family::Debian),
            ("pop", "ubuntu debian", Family::Debian),
            ("raspbian", "debian", Family::Debian),
            ("rocky", "rhel centos fedora", Family::Fedora),
            ("almalinux", "rhel centos fedora", Family::Fedora),
            ("sles", "suse", Family::Suse),
            ("opensuse-tumbleweed", "opensuse suse", Family::Suse),
            ("manjaro", "arch", Family::Arch),
            ("endeavouros", "arch", Family::Arch),
            ("cachyos", "arch", Family::Arch),
        ] {
            assert_eq!(family_from(&osr(id, like)), esperado, "{id} classificou errado");
        }
    }

    /// **A ordem do teste importa, e este é o caso que a prova.** O openSUSE declara
    /// `ID_LIKE="suse opensuse"`, mas há distros RPM que citam mais de uma família. Testar a
    /// específica antes da genérica é o que impede um SUSE virar Fedora — e receber `dnf`.
    #[test]
    fn suse_nunca_e_classificado_como_fedora() {
        let confuso = "ID=opensuse-leap\nID_LIKE=\"suse opensuse fedora\"\n";
        assert_eq!(family_from(confuso), Family::Suse, "SUSE receberia comandos `dnf`");
    }

    /// Distro fora do rol é `Unknown` — e o método `distro` fica indisponível, em vez de
    /// tentar um gerenciador adivinhado. Deny-by-default.
    #[test]
    fn distro_desconhecida_nao_vira_palpite() {
        assert_eq!(family_from(&osr("gentoo", "")), Family::Unknown);
        assert_eq!(family_from(&osr("nixos", "")), Family::Unknown);
        assert_eq!(family_from(""), Family::Unknown);
    }
}
