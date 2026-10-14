//! Testes da [`super`] (limpeza de versões antigas): tudo em sandbox temporária, com `PATH`,
//! `HOME` e diretórios do sistema INJETADOS — nenhum toca a máquina real.

use super::*;

/// **O quê:** um diretório temporário novo e vazio para um cenário.
/// **Onde:** todo teste deste módulo — nenhum toca `HOME` nem `PATH` reais.
fn sandbox(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("market-limpeza-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// **O quê:** cria `base/sub` e devolve o caminho. **Onde:** montagem dos cenários.
fn dir(base: &Path, sub: &str) -> PathBuf {
    let d = base.join(sub);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// **O quê:** grava um "binário" que responde `--version` com `versao`.
/// **Onde:** as cópias antigas e a fresca dos cenários.
fn binario(p: &Path, versao: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(p, format!("#!/bin/sh\necho \"x {versao} (abc1234)\"\n")).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// **O quê:** o dono de pacote falso. **Onde:** [`amb`].
fn dono_falso(_: &Path) -> Option<String> {
    Some("schematize".to_string())
}

/// **O quê:** remove de verdade, MENOS o que está num diretório chamado `negado` — simula
/// permissão negada sem depender de ser root ou não. **Onde:** o teste de permissão.
fn remover_negando(p: &Path) -> io::Result<Option<PathBuf>> {
    if p.components().any(|c| c.as_os_str() == "negado") {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied"));
    }
    std::fs::remove_file(p).map(|()| None)
}

/// **O quê:** um [`Ambiente`] 100% de sandbox: só os diretórios passados existem para ele.
/// **Onde:** todos os testes de comportamento.
fn amb(path: &[&Path], usuario: &[&Path], sistema: &[&Path], state: &Path) -> Ambiente {
    Ambiente {
        path_dirs: path.iter().map(|p| p.to_path_buf()).collect(),
        dirs_usuario: usuario.iter().map(|p| p.to_path_buf()).collect(),
        dirs_padrao: vec![],
        dirs_sistema: sistema.iter().map(|p| p.to_path_buf()).collect(),
        state_dir: state.to_path_buf(),
        protegidos: vec![],
        remover: remover_real,
        dono_do_pacote: dono_falso,
    }
}

/// **O quê:** o [`Alvo`] do overdev (CLI + janela) com a cópia boa em `fresco_dir`.
/// **Onde:** os testes de comportamento.
fn alvo_overdev(fresco_dir: &Path) -> Alvo {
    let cli = fresco_dir.join("schematize-overdev");
    let gui = fresco_dir.join("schematize-overdev-gui");
    Alvo {
        itens: vec![
            Item { nome: "schematize-overdev".into(), frescos: vec![cli.clone()], generico: false },
            Item { nome: "schematize-overdev-gui".into(), frescos: vec![gui], generico: false },
        ],
        ancora: vec![cli],
        repos_de_build: vec![],
        limpar_cache: false,
    }
}

/// **A COPIA ANTIGA em outro diretório do PATH sai; a fresca fica.** É o defeito do outro PC:
/// duas cópias, o PATH escolhendo a velha.
#[test]
fn copia_velha_em_outro_dir_do_path_sai_e_a_fresca_fica() {
    let b = sandbox("velha");
    let (novo, velho) = (dir(&b, ".cargo/bin"), dir(&b, ".local/bin"));
    binario(&novo.join("schematize-overdev"), "0.2.0");
    binario(&velho.join("schematize-overdev"), "0.1.0");

    let rel = limpar_em(&amb(&[&novo, &velho], &[], &[], &b.join("st")), &alvo_overdev(&novo));

    assert!(novo.join("schematize-overdev").is_file(), "apagou a cópia FRESCA");
    assert!(!velho.join("schematize-overdev").exists(), "a cópia velha tinha de sair");
    assert_eq!(rel.removidos.len(), 1, "{rel:?}");
    assert_eq!(rel.removidos[0].versao.as_deref(), Some("0.1.0"));
    assert!(rel.completo());
    let l = rel.linhas().join("\n");
    assert!(l.contains(&format!("removido: {}", velho.join("schematize-overdev").display())));
    assert!(l.contains("versão antiga 0.1.0"), "{l}");
    assert!(l.contains("COMPLETA"), "{l}");
    let _ = std::fs::remove_dir_all(&b);
}

/// **SOBRAS** (`.bak-*`, `.old`, `.novo`, `.new`, `.tmp*`, `.baixado`) no diretório de
/// instalação saem; a fresca fica; a versão do backup vai para o relatório.
#[test]
fn sobras_bak_old_new_tmp_sao_removidas() {
    let b = sandbox("sobras");
    let bin = dir(&b, ".cargo/bin");
    binario(&bin.join("schematize-overdev"), "0.2.0");
    let sobras = [
        ".schematize-overdev.bak-0.0.9",
        "schematize-overdev.old",
        "schematize-overdev.new",
        "schematize-overdev.novo",
        "schematize-overdev.tmp4242",
        ".schematize-overdev.tmp-9-0",
        "schematize-overdev.baixado",
        "schematize-overdev.compilado",
    ];
    for s in sobras {
        std::fs::write(bin.join(s), b"lixo").unwrap();
    }

    let rel = limpar_em(&amb(&[&bin], &[], &[], &b.join("st")), &alvo_overdev(&bin));

    for s in sobras {
        assert!(!bin.join(s).exists(), "{s} sobrou");
    }
    assert!(bin.join("schematize-overdev").is_file());
    assert_eq!(rel.removidos.len(), sobras.len(), "{rel:?}");
    let l = rel.linhas().join("\n");
    assert!(l.contains("backup da versão 0.0.9"), "{l}");
    let _ = std::fs::remove_dir_all(&b);
}

/// **SYMLINK para a fresca fica.** `~/.local/bin/x -> ~/.cargo/bin/x` é um atalho legítimo.
#[test]
fn symlink_para_o_fresco_fica() {
    let b = sandbox("symlink");
    let (novo, outro) = (dir(&b, ".cargo/bin"), dir(&b, ".local/bin"));
    binario(&novo.join("schematize-overdev"), "0.2.0");
    std::os::unix::fs::symlink(novo.join("schematize-overdev"), outro.join("schematize-overdev"))
        .unwrap();

    let rel = limpar_em(&amb(&[&novo, &outro], &[], &[], &b.join("st")), &alvo_overdev(&novo));

    assert!(outro.join("schematize-overdev").is_symlink(), "apagou o atalho para a fresca");
    assert!(rel.removidos.is_empty(), "{rel:?}");
    let _ = std::fs::remove_dir_all(&b);
}

/// **NUNCA seguir symlink para fora.** Um link com o nome do app que aponta para um arquivo
/// de OUTRO lugar perde só o LINK; o arquivo apontado sobrevive.
#[test]
fn symlink_para_fora_remove_o_link_e_poupa_o_alvo() {
    let b = sandbox("symfora");
    let (novo, outro, longe) = (dir(&b, ".cargo/bin"), dir(&b, ".local/bin"), dir(&b, "longe"));
    binario(&novo.join("schematize-overdev"), "0.2.0");
    std::fs::write(longe.join("dado-do-usuario"), b"importante").unwrap();
    std::os::unix::fs::symlink(longe.join("dado-do-usuario"), outro.join("schematize-overdev"))
        .unwrap();

    let rel = limpar_em(&amb(&[&novo, &outro], &[], &[], &b.join("st")), &alvo_overdev(&novo));

    assert!(!outro.join("schematize-overdev").is_symlink(), "o link tinha de sair");
    assert_eq!(std::fs::read(longe.join("dado-do-usuario")).unwrap(), b"importante");
    assert_eq!(rel.removidos.len(), 1);
    let _ = std::fs::remove_dir_all(&b);
}

/// **Nome PARECIDO não é do app.** Só o nome exato e os padrões de sobra casam.
#[test]
fn nome_parecido_nao_e_tocado() {
    let b = sandbox("parecido");
    let bin = dir(&b, ".cargo/bin");
    binario(&bin.join("schematize-overdev"), "0.2.0");
    let intocaveis = [
        "schematize-overdev-notes.txt",
        "schematize-marketing",
        "schematize-overdevx",
        "xschematize-overdev",
        "schematize-overdev.txt",
        "schematize-overdev.tmpl",
        "schematize-overdev.bak",
        ".schematize-overdev.bak-",
        "notas.bak-1",
    ];
    for n in intocaveis {
        std::fs::write(bin.join(n), b"meu").unwrap();
    }
    // E um DIRETÓRIO com o nome exato nunca é apagado.
    std::fs::create_dir_all(bin.join("outro/x")).unwrap();
    let dir_homonimo = dir(&b, "outro-bin");
    std::fs::create_dir_all(dir_homonimo.join("schematize-overdev/dentro")).unwrap();

    let rel = limpar_em(&amb(&[&bin, &dir_homonimo], &[], &[], &b.join("st")), &alvo_overdev(&bin));

    for n in intocaveis {
        assert!(bin.join(n).exists(), "{n} é de outro programa e foi apagado");
    }
    assert!(dir_homonimo.join("schematize-overdev/dentro").is_dir());
    assert!(rel.removidos.is_empty(), "{rel:?}");
    let _ = std::fs::remove_dir_all(&b);
}

/// **Cópia de PACOTE DO SISTEMA é reportada, não apagada**, com o comando para tirar — e a
/// limpeza fica INCOMPLETA.
#[test]
fn copia_do_sistema_e_reportada_nao_apagada() {
    let b = sandbox("sistema");
    let (novo, usr) = (dir(&b, ".cargo/bin"), dir(&b, "usr/bin"));
    binario(&novo.join("schematize-overdev"), "0.2.0");
    binario(&usr.join("schematize-overdev"), "0.0.1");

    let rel = limpar_em(&amb(&[&novo, &usr], &[], &[&usr], &b.join("st")), &alvo_overdev(&novo));

    assert!(usr.join("schematize-overdev").is_file(), "apagou arquivo de pacote do sistema");
    assert_eq!(rel.sistema.len(), 1);
    assert_eq!(rel.sistema[0].pacote.as_deref(), Some("schematize"));
    assert!(!rel.completo(), "cópia de sistema pendente = não está limpo");
    let l = rel.linhas().join("\n");
    assert!(l.contains("instalado pelo pacote do sistema"), "{l}");
    assert!(l.contains("sudo "), "tem de dizer o comando: {l}");
    assert!(l.contains("INCOMPLETA"), "{l}");
    let _ = std::fs::remove_dir_all(&b);
}

/// **Permissão negada: o erro é DITO com o caminho exato, a limpeza SEGUE e o resultado é
/// INCOMPLETO.** O arquivo seguinte (em outro dir) ainda é apagado.
#[test]
fn permissao_negada_e_reportada_e_a_limpeza_segue() {
    let b = sandbox("negado");
    let (novo, negado, livre) = (dir(&b, ".cargo/bin"), dir(&b, "negado"), dir(&b, "zlivre"));
    binario(&novo.join("schematize-overdev"), "0.2.0");
    binario(&negado.join("schematize-overdev"), "0.1.0");
    binario(&livre.join("schematize-overdev"), "0.1.1");
    let mut a = amb(&[&novo, &negado, &livre], &[], &[], &b.join("st"));
    a.remover = remover_negando;

    let rel = limpar_em(&a, &alvo_overdev(&novo));

    assert!(negado.join("schematize-overdev").is_file());
    assert!(!livre.join("schematize-overdev").exists(), "a limpeza parou no primeiro erro");
    assert_eq!(rel.falhas.len(), 1, "{rel:?}");
    assert_eq!(rel.falhas[0].caminho, negado.join("schematize-overdev"));
    assert!(!rel.completo());
    let l = rel.linhas().join("\n");
    assert!(l.contains(&negado.join("schematize-overdev").display().to_string()), "{l}");
    assert!(l.contains("Permission denied") && l.contains("rm "), "{l}");
    assert!(l.contains("INCOMPLETA"), "{l}");
    let _ = std::fs::remove_dir_all(&b);
}

/// **Permissão negada DE VERDADE** (diretório sem escrita), no remover real.
#[test]
fn diretorio_sem_escrita_vira_falha_real() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: geteuid não tem pré-condição nem efeito colateral.
    if unsafe { libc::geteuid() } == 0 {
        return; // root ignora permissão de diretório — o teste não prova nada ali.
    }
    let b = sandbox("ro");
    let (novo, ro) = (dir(&b, ".cargo/bin"), dir(&b, "ro"));
    binario(&novo.join("schematize-overdev"), "0.2.0");
    binario(&ro.join("schematize-overdev"), "0.1.0");
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();

    let rel = limpar_em(&amb(&[&novo, &ro], &[], &[], &b.join("st")), &alvo_overdev(&novo));

    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(rel.falhas.len(), 1, "{rel:?}");
    assert!(ro.join("schematize-overdev").is_file());
    assert!(!rel.completo());
    let _ = std::fs::remove_dir_all(&b);
}

/// **Update que FALHOU não limpa** — a cópia antiga é o fallback. Com sucesso, limpa.
#[test]
fn update_que_falhou_nao_limpa() {
    let b = sandbox("falhou");
    let (novo, velho) = (dir(&b, ".cargo/bin"), dir(&b, ".local/bin"));
    binario(&novo.join("schematize-overdev"), "0.2.0");
    binario(&velho.join("schematize-overdev"), "0.1.0");
    let a = amb(&[&novo, &velho], &[], &[], &b.join("st"));
    let alvo = alvo_overdev(&novo);

    let falha: Result<(), String> = Err("rede caiu".into());
    assert!(apos_em(&a, &falha, &alvo).is_none(), "update falho não pode limpar");
    assert!(velho.join("schematize-overdev").is_file(), "apagou o fallback");

    let ok: Result<(), String> = Ok(());
    let rel = apos_em(&a, &ok, &alvo).expect("sucesso limpa");
    assert_eq!(rel.removidos.len(), 1);
    assert!(!velho.join("schematize-overdev").exists());
    let _ = std::fs::remove_dir_all(&b);
}

/// **Sem cópia boa não há o que comparar:** nada é apagado.
#[test]
fn sem_ancora_existente_nao_apaga_nada() {
    let b = sandbox("semancora");
    let (novo, velho) = (dir(&b, ".cargo/bin"), dir(&b, ".local/bin"));
    binario(&velho.join("schematize-overdev"), "0.1.0");

    let rel = limpar_em(&amb(&[&novo, &velho], &[], &[], &b.join("st")), &alvo_overdev(&novo));

    assert!(velho.join("schematize-overdev").is_file(), "era a ÚNICA cópia");
    assert_eq!(rel, Relatorio::default());
    let _ = std::fs::remove_dir_all(&b);
}

/// **Janela nunca é executada** (casa por nome), mas a cópia antiga dela também sai. A
/// janela fresca ausente desliga o item: não se apaga a única janela da máquina.
#[test]
fn janela_antiga_sai_sem_ser_executada() {
    let b = sandbox("gui");
    let (novo, velho) = (dir(&b, ".cargo/bin"), dir(&b, ".local/bin"));
    binario(&novo.join("schematize-overdev"), "0.2.0");
    binario(&novo.join("schematize-overdev-gui"), "0.2.0");
    let marca = b.join("EXECUTOU");
    std::fs::write(
        velho.join("schematize-overdev-gui"),
        format!("#!/bin/sh\ntouch {}\n", marca.display()),
    )
    .unwrap();
    std::fs::set_permissions(
        velho.join("schematize-overdev-gui"),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();

    let rel = limpar_em(&amb(&[&novo, &velho], &[], &[], &b.join("st")), &alvo_overdev(&novo));

    assert!(!marca.exists(), "executou uma janela para descobrir a versão");
    assert!(!velho.join("schematize-overdev-gui").exists());
    assert_eq!(rel.removidos[0].versao, None);
    assert!(rel.linhas()[0].contains("versão antiga desconhecida"));

    // Janela fresca ausente: a velha é a única — fica.
    std::fs::remove_file(novo.join("schematize-overdev-gui")).unwrap();
    std::fs::write(velho.join("schematize-overdev-gui"), b"unica").unwrap();
    let rel = limpar_em(&amb(&[&novo, &velho], &[], &[], &b.join("st")), &alvo_overdev(&novo));
    assert!(velho.join("schematize-overdev-gui").is_file());
    assert!(rel.removidos.is_empty());
    let _ = std::fs::remove_dir_all(&b);
}

/// **Um `--version` que trava não trava a limpeza** (prazo de 2 s, processo morto).
#[test]
fn versao_que_trava_nao_bloqueia() {
    let b = sandbox("trava");
    let (novo, velho) = (dir(&b, ".cargo/bin"), dir(&b, ".local/bin"));
    binario(&novo.join("schematize-overdev"), "0.2.0");
    std::fs::write(velho.join("schematize-overdev"), "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(
        velho.join("schematize-overdev"),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();

    let t0 = Instant::now();
    let rel = limpar_em(&amb(&[&novo, &velho], &[], &[], &b.join("st")), &alvo_overdev(&novo));

    assert!(t0.elapsed() < Duration::from_secs(10), "a limpeza esperou o binário travado");
    assert!(!velho.join("schematize-overdev").exists());
    assert_eq!(rel.removidos[0].versao, None);
    let _ = std::fs::remove_dir_all(&b);
}

/// **PATH relativo é ignorado** (dependeria do diretório corrente) e dir inexistente é só
/// pulado.
#[test]
fn path_relativo_e_inexistente_sao_ignorados() {
    let b = sandbox("relativo");
    let novo = dir(&b, ".cargo/bin");
    binario(&novo.join("schematize-overdev"), "0.2.0");
    let mut a = amb(&[&novo], &[], &[], &b.join("st"));
    a.path_dirs.push(PathBuf::from("."));
    a.path_dirs.push(PathBuf::from(""));
    a.path_dirs.push(b.join("nao-existe"));

    let rel = limpar_em(&a, &alvo_overdev(&novo));

    assert_eq!(rel, Relatorio::default());
    let _ = std::fs::remove_dir_all(&b);
}

/// **LEGADO DO UPDATER:** `schematize-updater[-gui]` (ADR-0013/0014) sai de qualquer dir do
/// PATH — o nome é distintivo —, MAS só com o market (a âncora) instalado.
#[test]
fn updater_aposentado_sai_do_path() {
    let b = sandbox("updater");
    let (novo, velho) = (dir(&b, ".cargo/bin"), dir(&b, "opt/bin"));
    binario(&novo.join("schematize-market"), "0.4.3");
    binario(&velho.join("schematize-updater"), "0.7.2");
    std::fs::write(velho.join("schematize-updater-gui"), b"janela").unwrap();
    let item = |n: &str, f: Vec<PathBuf>| Item { nome: n.into(), frescos: f, generico: false };
    let alvo = Alvo {
        itens: vec![
            item("schematize-market", vec![novo.join("schematize-market")]),
            item("schematize-updater", vec![]),
            item("schematize-updater-gui", vec![]),
        ],
        ancora: vec![novo.join("schematize-market")],
        repos_de_build: vec![],
        limpar_cache: false,
    };

    let rel = limpar_em(&amb(&[&novo, &velho], &[], &[], &b.join("st")), &alvo);

    assert!(!velho.join("schematize-updater").exists());
    assert!(!velho.join("schematize-updater-gui").exists());
    assert!(novo.join("schematize-market").is_file());
    assert_eq!(rel.removidos.len(), 2, "{rel:?}");
    let _ = std::fs::remove_dir_all(&b);
}

/// **Nome legado GENÉRICO** (`deployer`) só sai de `~/.cargo/bin` e `~/.local/bin`: num
/// diretório alheio do PATH pode ser o Deployer do PHP, de outra pessoa.
#[test]
fn nome_generico_so_sai_dos_dirs_de_usuario() {
    let b = sandbox("generico");
    let (novo, local, alheio) = (dir(&b, ".cargo/bin"), dir(&b, ".local/bin"), dir(&b, "opt/bin"));
    binario(&novo.join("schematize-deployer"), "0.9.0");
    std::fs::write(local.join("deployer"), b"velho").unwrap();
    std::fs::write(alheio.join("deployer"), b"php").unwrap();
    let alvo = Alvo {
        itens: vec![
            Item {
                nome: "schematize-deployer".into(),
                frescos: vec![novo.join("schematize-deployer")],
                generico: false,
            },
            Item { nome: "deployer".into(), frescos: vec![], generico: true },
        ],
        ancora: vec![novo.join("schematize-deployer")],
        repos_de_build: vec![],
        limpar_cache: false,
    };

    limpar_em(&amb(&[&novo, &local, &alheio], &[&novo, &local], &[], &b.join("st")), &alvo);

    assert!(!local.join("deployer").exists(), "o legado em ~/.local/bin tinha de sair");
    assert!(alheio.join("deployer").is_file(), "apagou um `deployer` que pode não ser nosso");
    let _ = std::fs::remove_dir_all(&b);
}

/// **CACHE DE BUILD do market** (`state_dir/build/<repo>`), só de app que veio de BINÁRIO;
/// o `pin` e o checkout de outro app ficam; o `target/` compartilhado só sai quando não
/// sobrou checkout; `dl/` (temporário do download) sai.
#[test]
fn cache_de_build_so_do_app_de_binario_e_nunca_o_pin() {
    let b = sandbox("cache");
    let novo = dir(&b, ".cargo/bin");
    binario(&novo.join("schematize-overdev"), "0.2.0");
    let st = dir(&b, "st");
    for d in ["build/schematize_overdev_rs", "build/schematize_git_rs", "build/target", "dl"] {
        std::fs::create_dir_all(st.join(d)).unwrap();
        std::fs::write(st.join(d).join("x"), b"x").unwrap();
    }
    std::fs::write(st.join("pin"), "0.57.0\n").unwrap();
    let mut alvo = alvo_overdev(&novo);
    alvo.repos_de_build = vec!["schematizeme/schematize_overdev_rs"];

    // Sem o método binário: nada de cache.
    limpar_em(&amb(&[&novo], &[], &[], &st), &alvo);
    assert!(st.join("build/schematize_overdev_rs").is_dir(), "apagou cache de app de FONTE");

    alvo.limpar_cache = true;
    let rel = limpar_em(&amb(&[&novo], &[], &[], &st), &alvo);

    assert!(!st.join("build/schematize_overdev_rs").exists());
    assert!(st.join("build/schematize_git_rs").is_dir(), "checkout de OUTRO app");
    assert!(st.join("build/target").is_dir(), "ainda há checkout usando o target");
    assert!(!st.join("dl").exists());
    assert_eq!(std::fs::read_to_string(st.join("pin")).unwrap(), "0.57.0\n");
    assert!(rel.removidos.iter().all(|r| r.tipo == Tipo::Cache));

    // Último checkout: o target compartilhado sai junto; o pin continua.
    let mut alvo2 = alvo_overdev(&novo);
    alvo2.repos_de_build = vec!["schematizeme/schematize_git_rs"];
    alvo2.limpar_cache = true;
    limpar_em(&amb(&[&novo], &[], &[], &st), &alvo2);
    assert!(!st.join("build/schematize_git_rs").exists());
    assert!(!st.join("build/target").exists(), "target sem checkout é lixo");
    assert!(st.join("pin").is_file(), "o pin NUNCA se apaga");
    let _ = std::fs::remove_dir_all(&b);
}

/// **Checkout que é symlink não é seguido:** o link some do cache? Não — fica, e o alvo
/// também. Cache só se apaga se for diretório de verdade.
#[test]
fn cache_que_e_symlink_nao_e_seguido() {
    let b = sandbox("cachesym");
    let novo = dir(&b, ".cargo/bin");
    binario(&novo.join("schematize-overdev"), "0.2.0");
    let (st, longe) = (dir(&b, "st"), dir(&b, "meu-repo"));
    std::fs::create_dir_all(st.join("build")).unwrap();
    std::fs::write(longe.join("codigo.rs"), b"meu").unwrap();
    std::os::unix::fs::symlink(&longe, st.join("build/schematize_overdev_rs")).unwrap();
    let mut alvo = alvo_overdev(&novo);
    alvo.repos_de_build = vec!["schematizeme/schematize_overdev_rs"];
    alvo.limpar_cache = true;

    limpar_em(&amb(&[&novo], &[], &[], &st), &alvo);

    assert!(longe.join("codigo.rs").is_file(), "seguiu o symlink e apagou o repo do usuário");
    let _ = std::fs::remove_dir_all(&b);
}

/// **Os padrões de sobra, como função pura** — incluindo o `.exe` do Windows, em que o
/// `.old` é `<stem>.old` (`with_extension`), e o que NÃO casa.
#[test]
fn classificar_casa_os_padroes_reais_e_so_eles() {
    let s = |a, b| classificar(a, b);
    assert_eq!(s("x", "x"), Some(Achado::Copia));
    assert_eq!(s("x.old", "x"), Some(Achado::Sobra(None)));
    assert_eq!(s("x.old", "x.exe"), Some(Achado::Sobra(None)), "Windows: stem.old");
    assert_eq!(s("x.exe.old", "x.exe"), Some(Achado::Sobra(None)));
    assert_eq!(s(".x.bak-0.66.0", "x"), Some(Achado::Sobra(Some("0.66.0".into()))));
    assert_eq!(s("x.tmp", "x"), Some(Achado::Sobra(None)));
    assert_eq!(s("x.tmp123", "x"), Some(Achado::Sobra(None)));
    assert_eq!(s(".x.tmp-1-2", "x"), Some(Achado::Sobra(None)));
    assert_eq!(s("x.tmpl", "x"), None);
    assert_eq!(s("x-notes.txt", "x"), None);
    assert_eq!(s("xx", "x"), None);
    assert_eq!(s("x.bak", "x"), None);
    assert_eq!(s("y.old", "x"), None);
    assert_eq!(s(".x.bak-", "x"), None);
}

/// A janela é reconhecida com ou sem `.exe` (nome como DADO).
#[test]
fn janela_com_ou_sem_exe() {
    assert!(eh_janela("schematize-overdev-gui"));
    assert!(eh_janela("schematize-overdev-gui.exe"));
    assert!(!eh_janela("schematize-overdev"));
}

/// **O renomeado do Windows é dito como tal** na linha do relatório.
#[test]
fn renomeado_aparece_no_relatorio() {
    let rel = Relatorio {
        removidos: vec![Removido {
            caminho: PathBuf::from("/a/x.exe"),
            versao: Some("0.1.0".into()),
            tipo: Tipo::Copia,
            renomeado_para: Some(PathBuf::from("/a/x.old")),
        }],
        ..Default::default()
    };
    assert!(rel.linhas()[0].starts_with("renomeado: /a/x.exe -> /a/x.old"));
}

/// **Os alvos de verdade:** cada app da tabela tem o próprio nome, a janela (quando tem) e
/// o legado; o market leva o updater aposentado; o hub leva o interregno Overflow.
#[test]
fn alvos_reais_carregam_nomes_e_legados() {
    let nomes = |a: &Alvo| a.itens.iter().map(|i| i.nome.clone()).collect::<Vec<_>>();
    let sfx = plataforma::exe_suffix();
    for app in super::super::APPS_GERIDOS {
        let a = alvo_do_app(app, false);
        assert!(nomes(&a).contains(&app.bin_name()), "{} fora do alvo", app.bin);
        if let Some((g, _)) = app.gui_nome_e_destino() {
            assert!(nomes(&a).contains(&g), "janela de {} fora do alvo", app.bin);
        }
    }
    let dep = super::super::APPS_GERIDOS.iter().find(|a| a.bin == "schematize-deployer");
    let d = alvo_do_app(dep.unwrap(), false);
    let legado = d.itens.iter().find(|i| i.nome == format!("deployer{sfx}")).unwrap();
    assert!(legado.generico && legado.frescos.is_empty());

    let mkt = super::super::APPS_GERIDOS.iter().find(|a| a.politica == Politica::Sempre);
    let m = alvo_do_app(mkt.unwrap(), true);
    for n in ["schematize-updater", "schematize-updater-gui", "schematize-market-gui"] {
        assert!(nomes(&m).contains(&format!("{n}{sfx}")), "{n} fora do alvo do market");
    }
    assert!(m.limpar_cache && m.repos_de_build.contains(&plataforma::MARKET_REPO));

    let h = alvo_do_hub(false);
    for n in ["schematize", "schematize-gui", "overflow", "overflow-gui"] {
        assert!(nomes(&h).contains(&format!("{n}{sfx}")), "{n} fora do alvo do hub");
    }
}

/// **Sem opt-out:** o módulo não lê variável de ambiente nem flag para se desligar. É
/// compulsório (D11); este teste lê o próprio código de produção.
#[test]
fn nao_ha_como_desligar() {
    let fonte = include_str!("../limpeza.rs");
    let prod = fonte.split("#[cfg(test)]").next().unwrap();
    for proibido in ["env::var(\"", "SCHEMATIZE_", "no_clean", "skip_clean", "--no-"] {
        assert!(!prod.contains(proibido), "achei `{proibido}`: limpeza é compulsória");
    }
}
