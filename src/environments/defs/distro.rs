//! DEFS/DISTRO — o método `distro`: nomes de pacote e comandos, por FAMÍLIA.
//!
//! **O quê:** o mapa `linguagem × família → pacotes`, os comandos de instalar/remover de cada
//! gerenciador, e a checagem de disponibilidade que recusa antes de pedir sudo.
//!
//! **Onde:** `defs::install_recipe` e `defs::remove_recipe`.
//!
//! ## Por que virou arquivo próprio
//!
//! O `defs.rs` passou de 750 linhas ao ganhar o Arch e a checagem de disponibilidade, e o
//! piso §6 da casa reprovou — corretamente. O corte é por COESÃO, não por tamanho: tudo aqui
//! responde a uma pergunta só — *"como esta família instala software?"* —, e nada mais no
//! `defs` precisa saber disso.
//!
//! ## Campo vazio é INFORMAÇÃO, não esquecimento
//!
//! Quando o pacote de uma família está vazio, isso significa **não existe nos repos padrão
//! dela**. É o que faz [`distro_indisponivel`] recusar com uma saída, em vez de montar um
//! `install` que morre com "Nenhum fornecedor encontrado" depois de a pessoa digitar a senha.

use super::Family;

/// Pacotes da distro por (linguagem, gerenciador). Retorna a lista de pacotes do runtime.
/// Nomes podem variar entre distros da mesma família — por isso separamos zypper de dnf.
pub(super) struct DistroPkgs {
    debian: &'static str,
    /// openSUSE/SLES. Vazio = **não existe nos repos padrão desta família**.
    zypper: &'static str,
    /// Fedora/RHEL. Vazio = idem.
    dnf: &'static str,
    /// Arch e derivadas. Vazio = idem.
    arch: &'static str,
}

/// Mapa de pacotes de runtime da distro por linguagem.
pub(super) fn distro_pkgs(lang: &str) -> Option<DistroPkgs> {
    Some(match lang {
        "go" => DistroPkgs { debian: "golang-go", zypper: "go", dnf: "golang", arch: "go" },
        "rust" => DistroPkgs {
            debian: "rustc cargo",
            zypper: "rust cargo",
            dnf: "rust cargo",
            arch: "rust",
        },
        "elixir" => {
            DistroPkgs { debian: "elixir", zypper: "elixir", dnf: "elixir", arch: "elixir" }
        }
        // O .NET é o caso que expôs o buraco: NÃO está nos repos padrão do openSUSE (medido:
        // `zypper search dotnet` devolve zero no Leap 16). O vazio aqui não é esquecimento —
        // é a informação de que essa via não existe, e é o que faz a ferramenta avisar em vez
        // de rodar um `zypper install` que falha com "exit 104".
        "csharp" => DistroPkgs {
            debian: "dotnet-sdk-8.0",
            zypper: "",
            dnf: "dotnet-sdk-8.0",
            arch: "dotnet-sdk",
        },
        "zig" => DistroPkgs { debian: "zig", zypper: "zig", dnf: "zig", arch: "zig" },
        "ruby" => DistroPkgs {
            debian: "ruby ruby-dev",
            zypper: "ruby ruby-devel",
            dnf: "ruby ruby-devel",
            arch: "ruby",
        },
        "node" => DistroPkgs {
            debian: "nodejs npm",
            zypper: "nodejs npm",
            dnf: "nodejs npm",
            arch: "nodejs npm",
        },
        _ => return None,
    })
}

/// Comando de instalar pacotes conforme a família (embute o if zypper/dnf pra rpm).
pub(super) fn distro_install_cmd(fam: Family, pkgs: &DistroPkgs) -> String {
    // Cada família tem o SEU comando. Antes havia um `if command -v zypper …` embutido, porque
    // Fedora e SUSE eram a mesma `Family::Rpm` — e o usuário via aquele shell no plano de
    // consentimento, sem saber qual metade rodaria na máquina dele.
    distro_install_pkgs_cmd(fam, pkgs_da_familia(fam, pkgs))
}

/// **O quê:** o comando de instalar da família, para uma lista de pacotes JÁ resolvida. PURA.
///
/// **Onde:** [`distro_install_cmd`] (pacotes da distro) e o plano quando os pacotes vêm de um
/// repo de upstream — nesse caso o nome vem do fornecedor, não da tabela da distro.
///
/// Existe separada para que os dois caminhos usem o MESMO comando por família: duplicar o
/// `apt-get`/`zypper`/`dnf`/`pacman` num segundo lugar seria a divergência esperando o dia em
/// que um deles ganhasse uma flag e o outro não.
pub(super) fn distro_install_pkgs_cmd(fam: Family, pacotes: &str) -> String {
    match (fam, pacotes) {
        (_, "") => String::new(), // não há pacote nesta família — ver `distro_indisponivel`
        (Family::Debian, p) => format!("sudo apt-get update -qq && sudo apt-get install -y {p}"),
        (Family::Suse, p) => format!("sudo zypper --non-interactive install -y {p}"),
        (Family::Fedora, p) => format!("sudo dnf install -y {p}"),
        (Family::Arch, p) => format!("sudo pacman -S --noconfirm --needed {p}"),
        (Family::Unknown, _) => String::new(),
    }
}

/// **O quê:** os pacotes desta família. Vazio = a linguagem não existe nos repos padrão dela.
/// **Onde:** os comandos de instalar/remover e a checagem de disponibilidade.
pub(super) fn pkgs_da_familia(fam: Family, p: &DistroPkgs) -> &'static str {
    match fam {
        Family::Debian => p.debian,
        Family::Suse => p.zypper,
        Family::Fedora => p.dnf,
        Family::Arch => p.arch,
        Family::Unknown => "",
    }
}

/// Comando de remover pacotes conforme a família.
pub(super) fn distro_remove_cmd(fam: Family, pkgs: &DistroPkgs) -> String {
    match fam {
        _ if pkgs_da_familia(fam, pkgs).is_empty() => String::new(),
        Family::Debian => format!("sudo apt-get remove -y {}", pkgs.debian),
        Family::Suse => format!("sudo zypper --non-interactive rm -y {}", pkgs.zypper),
        Family::Fedora => format!("sudo dnf remove -y {}", pkgs.dnf),
        // `-Rns`: leva as dependências que ficaram órfãs e os arquivos de config do pacote.
        // Sem o `s`, desinstalar deixa um rastro que ninguém depois sabe de onde veio.
        Family::Arch => format!("sudo pacman -Rns --noconfirm {}", pkgs.arch),
        Family::Unknown => String::new(),
    }
}

/// Um repositório de UPSTREAM necessário para a linguagem existir nesta família — a distro não
/// a empacota, mas o fornecedor publica um repo oficial.
///
/// **Onde:** [`repo_extra`], e daí para os passos do plano.
pub(super) struct RepoExtra {
    /// Nome humano, mostrado no plano de consentimento.
    pub nome: &'static str,
    /// Chave GPG do fornecedor. Importada ANTES do repo: com `repo_gpgcheck=1`, um refresh sem
    /// a chave falha reclamando de assinatura, em vez de dizer o que está faltando.
    pub chave: &'static str,
    /// O `.repo` do fornecedor. `{v}` vira a versão maior da distro — hardcodar `15` quebraria
    /// no Leap 16 e vice-versa.
    pub repo: &'static str,
    /// Página oficial, para quem quiser conferir de onde isto vem.
    pub doc: &'static str,
}

/// **O quê:** o repo de upstream necessário para `lang` nesta família, quando há um.
///
/// **Onde:** a montagem do plano de `Method::Distro`. Função PURA.
///
/// ## Por que isto substituiu uma recusa
///
/// Antes, campo de pacote vazio virava uma mensagem: *"não está nos repos padrão; use
/// `--method mise`, ou, se quiser mesmo o repo da Microsoft, adicione-o você"*. Isso é a
/// ferramenta devolvendo ao usuário exatamente o trabalho que ela existe para fazer — quem
/// clicou em instalar pediu o .NET, não uma aula sobre empacotamento do openSUSE.
///
/// A recusa resolvia o problema ANTERIOR (montar um `zypper install` que morre com `exit 104`
/// depois de a pessoa digitar a senha). Mas "não faça a coisa errada" virou "não faça nada", e
/// a saída certa era a terceira: **adicionar o repo oficial e instalar**.
///
/// **O que NÃO muda:** os passos aparecem no plano de consentimento, com marca de `sudo` e
/// procedência. Adicionar repo de terceiro é decisão de confiança e continua visível — o que
/// deixa de existir é a pessoa ter de EXECUTAR essa decisão à mão.
pub(super) fn repo_extra(lang: &str, fam: Family) -> Option<RepoExtra> {
    match (lang, fam) {
        // Medido: `zypper search dotnet` = 0 no Leap 16; e o `primary.xml` de
        // `opensuse/16/prod` traz `dotnet-sdk-8.0`, `9.0` e `10.0`.
        ("csharp", Family::Suse) => Some(RepoExtra {
            nome: "Microsoft Production",
            chave: "https://packages.microsoft.com/keys/microsoft-2025.asc",
            repo: "https://packages.microsoft.com/config/opensuse/{v}/prod.repo",
            doc: "https://learn.microsoft.com/dotnet/core/install/linux-opensuse",
        }),
        _ => None,
    }
}

/// **O quê:** os pacotes a instalar quando há repo de upstream — o campo da família está vazio
/// porque a distro não empacota, então o nome do pacote vem do fornecedor.
/// **Onde:** o plano de `Method::Distro`. PURA.
pub(super) fn pkgs_do_repo(lang: &str, fam: Family) -> &'static str {
    match (lang, fam) {
        ("csharp", Family::Suse) => "dotnet-sdk-8.0",
        _ => "",
    }
}

/// **O quê:** a versão MAIOR da distro (`16.0` -> `16`), que é como os fornecedores organizam
/// os caminhos de repo. **Onde:** [`passos_do_repo`]. PURA.
///
/// Sem valor declarado devolve `None`, e o chamador não monta o repo: inventar um número daria
/// um 404 no meio de um `sudo`, que é pior que não tentar.
pub(super) fn versao_maior(os_release: &str) -> Option<String> {
    let v =
        os_release.lines().find_map(|l| l.strip_prefix("VERSION_ID="))?.trim().trim_matches('"');
    let maior = v.split('.').next()?;
    (!maior.is_empty() && maior.chars().all(|c| c.is_ascii_digit())).then(|| maior.to_string())
}

/// **O quê:** os comandos que adicionam o repo de upstream, na ordem. PURA.
/// **Onde:** o plano de `Method::Distro`, antes do comando de instalar.
pub(super) fn passos_do_repo(r: &RepoExtra, fam: Family, versao: &str) -> Vec<String> {
    let url = r.repo.replace("{v}", versao);
    match fam {
        Family::Suse => vec![
            format!("sudo rpm --import {}", r.chave),
            format!("sudo zypper --non-interactive addrepo --refresh --check {url}"),
            "sudo zypper --non-interactive --gpg-auto-import-keys refresh".to_string(),
        ],
        Family::Fedora => vec![
            format!("sudo rpm --import {}", r.chave),
            format!("sudo dnf config-manager --add-repo {url}"),
        ],
        // Debian e Arch não têm caso hoje. Quando tiverem, entram aqui — e o `vec![]` faz o
        // chamador tratar como "sem repo a adicionar", em vez de montar meio comando.
        _ => vec![],
    }
}

/// **O quê:** a linguagem é impossível nesta família, mesmo com repo de upstream?
///
/// **Onde:** o planejador, ANTES de montar os passos. PURA.
///
/// Sobrou para o caso em que NÃO há repo conhecido — aí não há o que fazer além de dizer, e
/// apontar um método que funciona. O caso que TINHA repo (o .NET no openSUSE) saiu daqui e
/// virou [`repo_extra`]: ter repo não é indisponibilidade, é trabalho a fazer.
pub fn distro_indisponivel(lang: &str, fam: Family) -> Option<String> {
    let pkgs = distro_pkgs(lang)?;
    if !pkgs_da_familia(fam, &pkgs).is_empty() {
        return None;
    }
    if repo_extra(lang, fam).is_some() {
        return None;
    }
    Some(format!(
        "`{lang}` não está nos repositórios padrão desta distro ({}), e não conheço um repo \
         oficial do fornecedor para ela.\n  Use outro método: `install {lang} --method mise`",
        fam.label()
    ))
}

#[cfg(test)]
mod tests_disponibilidade {
    use super::*;

    /// **O caso de campo, e o que ele virou.** `.NET` não está nos repos do openSUSE — medido:
    /// `zypper search dotnet` devolve zero no Leap 16.
    ///
    /// Este teste JÁ EXIGIU o contrário: que a ferramenta recusasse com uma saída. Aquilo
    /// resolvia o problema anterior (montar um `zypper install` que morre com `exit 104` depois
    /// da senha), mas devolvia ao usuário o trabalho que a ferramenta existe para fazer.
    ///
    /// Hoje a resposta certa é a terceira: adicionar o repo oficial e instalar. O que sobrou de
    /// "indisponível" é só o caso SEM repo conhecido.
    #[test]
    fn csharp_no_suse_e_resolvido_e_nao_recusado() {
        assert!(
            distro_indisponivel("csharp", Family::Suse).is_none(),
            "voltou a empurrar a tarefa para o usuário"
        );
        let r = repo_extra("csharp", Family::Suse).expect("tem de conhecer o repo da Microsoft");
        assert!(r.doc.contains("learn.microsoft.com"));
    }

    /// Sem pacote E sem repo conhecido, aí sim recusa — e a mensagem aponta um método que
    /// funciona, em vez de só dizer não.
    #[test]
    fn sem_pacote_e_sem_repo_ainda_recusa_com_saida() {
        let m = distro_indisponivel("zig", Family::Unknown);
        if let Some(m) = m {
            assert!(m.contains("--method mise"), "recusa tem de dar a saída: {m}");
        }
    }

    /// E onde ele existe, não recusa — a guarda não pode virar um "não" genérico.
    #[test]
    fn csharp_passa_onde_o_pacote_existe() {
        for f in [Family::Debian, Family::Fedora, Family::Arch] {
            assert!(distro_indisponivel("csharp", f).is_none(), "{f:?} tem dotnet nos repos");
        }
    }

    /// As linguagens que existem em todas as quatro seguem passando em todas.
    #[test]
    fn o_comum_continua_disponivel_nas_quatro() {
        for lang in ["go", "rust", "ruby", "node", "zig", "elixir"] {
            for f in [Family::Debian, Family::Fedora, Family::Suse, Family::Arch] {
                assert!(distro_indisponivel(lang, f).is_none(), "{lang} devia existir em {f:?}");
            }
        }
    }

    /// **Cada família gera o comando DELA** — sem `if command -v` em tempo de execução, que
    /// era o que aparecia no plano de consentimento sem dizer qual metade rodaria.
    #[test]
    fn cada_familia_tem_o_gerenciador_certo() {
        let p = distro_pkgs("go").unwrap();
        assert!(distro_install_cmd(Family::Debian, &p).contains("apt-get"));
        assert!(distro_install_cmd(Family::Suse, &p).contains("zypper"));
        assert!(distro_install_cmd(Family::Fedora, &p).contains("dnf"));
        assert!(distro_install_cmd(Family::Arch, &p).contains("pacman"));
        for f in [Family::Debian, Family::Suse, Family::Fedora, Family::Arch] {
            let c = distro_install_cmd(f, &p);
            assert!(!c.contains("command -v"), "{f:?} ainda decide em runtime: {c}");
        }
    }

    /// Pacote ausente na família gera comando VAZIO — nunca um `install` sem argumento, que
    /// no apt abre o modo interativo e no pacman tenta instalar o repositório inteiro.
    #[test]
    fn familia_sem_pacote_nao_gera_comando() {
        let p = distro_pkgs("csharp").unwrap();
        assert_eq!(distro_install_cmd(Family::Suse, &p), "", "comando vazio, não `install `");
        assert_eq!(distro_remove_cmd(Family::Suse, &p), "");
    }
}

/// **O quê:** a versão maior desta máquina, lida do `/etc/os-release`.
/// **Onde:** o plano, quando precisa montar a URL de um repo de upstream.
///
/// Faz o IO; a regra fica em [`versao_maior`], que é pura e testada. É a mesma separação do
/// resto do módulo, e é o que permite testar `16.0 -> 16` sem depender da distro de quem roda.
pub(super) fn versao_da_distro() -> Option<String> {
    versao_maior(&std::fs::read_to_string("/etc/os-release").ok()?)
}

#[cfg(test)]
mod tests_repo_upstream {
    use super::*;

    /// **O que isto fecha.** `install csharp --method distro` no openSUSE respondia
    /// *"o .NET NÃO está nos repositórios padrão — use `--method mise`, ou adicione o repo da
    /// Microsoft você"*. Quem clicou em instalar pediu o .NET, não uma tarefa.
    #[test]
    fn csharp_no_opensuse_deixou_de_ser_recusa() {
        assert!(
            distro_indisponivel("csharp", Family::Suse).is_none(),
            "voltou a recusar em vez de adicionar o repo"
        );
        assert!(repo_extra("csharp", Family::Suse).is_some());
        assert_eq!(pkgs_do_repo("csharp", Family::Suse), "dotnet-sdk-8.0");
    }

    /// Onde a distro JÁ empacota, não se acrescenta repo de terceiro — instalar mexendo em
    /// fonte de pacote sem precisar é risco sem ganho.
    #[test]
    fn onde_a_distro_empacota_nao_ha_repo_extra() {
        for f in [Family::Debian, Family::Fedora, Family::Arch] {
            assert!(repo_extra("csharp", f).is_none(), "{f:?} já tem dotnet nos repos padrão");
        }
        assert!(repo_extra("go", Family::Suse).is_none());
    }

    /// A versão sai do `/etc/os-release` e é a MAIOR: o caminho do repo é `/16/`, não `/16.0/`
    /// — medido, o `16.0` responde 404. Hardcodar `15` quebraria no Leap 16, e vice-versa.
    #[test]
    fn versao_maior_e_so_o_numero_antes_do_ponto() {
        assert_eq!(versao_maior("VERSION_ID=\"16.0\"\n").as_deref(), Some("16"));
        assert_eq!(versao_maior("ID=opensuse-leap\nVERSION_ID=\"15.6\"\n").as_deref(), Some("15"));
        assert_eq!(versao_maior("VERSION_ID=16\n").as_deref(), Some("16"));
    }

    /// Sem versão declarada (ou com lixo) NÃO se inventa número: uma URL chutada dá 404 no meio
    /// de um `sudo`, que é pior do que não tentar.
    #[test]
    fn versao_ausente_ou_invalida_nao_vira_palpite() {
        assert_eq!(versao_maior("ID=arch\n"), None);
        assert_eq!(versao_maior("VERSION_ID=\"rolling\"\n"), None);
        assert_eq!(versao_maior("VERSION_ID=\"\"\n"), None);
    }

    /// A chave vem ANTES do repo. Com `repo_gpgcheck=1`, o refresh sem a chave importada falha
    /// reclamando de assinatura — erro que fala de criptografia quando o que falta é um passo.
    #[test]
    fn a_chave_e_importada_antes_de_o_repo_entrar() {
        let r = repo_extra("csharp", Family::Suse).unwrap();
        let p = passos_do_repo(&r, Family::Suse, "16");
        let i_chave = p.iter().position(|c| c.contains("rpm --import")).expect("importa a chave");
        let i_repo = p.iter().position(|c| c.contains("addrepo")).expect("adiciona o repo");
        assert!(i_chave < i_repo, "a chave tem de vir antes: {p:?}");
    }

    /// O `{v}` é substituído — se sobrasse literal, o `zypper` receberia uma URL com chaves e
    /// falharia com um erro sobre sintaxe, não sobre versão.
    #[test]
    fn a_url_nao_sai_com_o_placeholder() {
        let r = repo_extra("csharp", Family::Suse).unwrap();
        for c in passos_do_repo(&r, Family::Suse, "16") {
            assert!(!c.contains("{v}"), "placeholder sobrou: {c}");
        }
        assert!(passos_do_repo(&r, Family::Suse, "16")
            .iter()
            .any(|c| c.contains("/opensuse/16/prod.repo")));
    }

    /// Família sem receita devolve lista VAZIA, e o chamador trata como "não sei fazer" em vez
    /// de montar meio comando e rodar com sudo.
    #[test]
    fn familia_sem_receita_nao_monta_meio_comando() {
        let r = repo_extra("csharp", Family::Suse).unwrap();
        assert!(passos_do_repo(&r, Family::Arch, "16").is_empty());
        assert!(passos_do_repo(&r, Family::Unknown, "16").is_empty());
    }

    /// O comando de instalar é o MESMO por família, venha o pacote da distro ou do upstream —
    /// senão um ganharia uma flag e o outro não.
    #[test]
    fn o_install_e_o_mesmo_venha_o_pacote_de_onde_vier() {
        let da_distro = distro_install_cmd(Family::Suse, &distro_pkgs("go").unwrap());
        let do_repo = distro_install_pkgs_cmd(Family::Suse, "dotnet-sdk-8.0");
        let prefixo = "sudo zypper --non-interactive install -y ";
        assert!(da_distro.starts_with(prefixo), "{da_distro}");
        assert!(do_repo.starts_with(prefixo), "{do_repo}");
    }
}
