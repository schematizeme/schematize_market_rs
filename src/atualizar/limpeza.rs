//! LIMPEZA — o `update` e o `install` REMOVEM as versões antigas, sempre.
//!
//! **O quê:** depois de um app (ou o próprio market) ser instalado ou atualizado COM SUCESSO,
//! apaga toda cópia antiga dele que sobrou na máquina: binários de mesmo nome em outro diretório
//! do `PATH` ou nos diretórios padrão de instalação, nomes legados, sobras de instalação
//! (`.<nome>.bak-*`, `<nome>.old|.new|.novo|.baixado|.compilado|.tmp*`) e o cache de build do
//! market que ficou sem uso.
//!
//! **Onde:** o fim de [`super::componentes`] (hub, apps geridos e self-update do market) e de
//! [`crate::appsdacasa::instalar_app_da_casa`]. É COMPULSÓRIO (D11): não há flag que
//! desligue. Quem quiser ver o que seria feito usa o `--dry-run`, que não instala nem limpa.
//!
//! ## Por que existe
//!
//! Um market velho falhava no outro PC e a causa provável era a AMBIGUIDADE: várias cópias do
//! mesmo binário em `PATH`, e a primeira do `PATH` ganhando — muitas vezes a mais velha. A
//! [`super::purga`] resolve isso só no caminho "compilar do fonte"; o caminho do binário pronto
//! (que é o normal) nunca limpava nada.
//!
//! ## Evidência dos nomes e diretórios legados (nada aqui é chute)
//!
//! - `schematize-updater` — [`plataforma::updater_bin_name`]; aposentado pelo ADR-0013.
//! - `schematize-updater-gui` — [`plataforma::gestor_gui_bin_legado`]; ADR-0014 D4.
//! - `overflow`, `overflow-gui` — [`plataforma::bin_names_interregno`]; e
//!   `overflow-updater[-gui]` na [`super::purga`] (`nomes()`).
//! - `deployer` — [`super::AppGerido::legado`] (renomeado no commit `0fa0112`).
//! - `~/.schematize/updater/build/<repo>` e `build/target` — o updater (`schematize_updater_rs`
//!   `platform.rs:113,127`) e o market ([`plataforma::build_src_dir`],
//!   [`plataforma::shared_target_dir`]) criam EXATAMENTE esses caminhos; é cache dele, não do
//!   usuário. `~/.schematize/updater/` em si NÃO se apaga: guarda o `pin` (versao.rs).
//! - `<nome>.novo`, `.baixado`, `.compilado` — [`super::binario::caminho_temporario`] e
//!   [`super::selfupdate`]; `<nome>.old` — `binario::substitui_binario` (Windows) e `install.sh`;
//!   `.<nome>.tmp-<pid>-<n>` — `schematize_cli_rs/src/util.rs::escreve_atomico`;
//!   `<arquivo>.tmp<pid>` — `metodo::gravar_registro_em`. O `.<nome>.bak-<versão>` é o backup
//!   que o procedimento de instalação do hub deixa (`.schematize.bak-0.66.0` nesta máquina): é
//!   padrão de CONVENÇÃO, nenhum código dos repos o grava.
//!
//! ## Regras de segurança (D11)
//!
//! 1. Nunca apaga o arquivo recém-instalado, nem symlink que resolva para ele, nem o binário em
//!    execução.
//! 2. Symlink: só o LINK é removido; o alvo nunca. Diretório de nome igual: nunca.
//! 3. Janela (`*-gui`) nunca é executada; casa por nome exato. CLI antigo pode responder
//!    `--version` (2 s, sem `DISPLAY`) só para o relatório — o prazo estourar não bloqueia nada.
//! 4. Arquivo de pacote do sistema (`/usr/bin`…) é REPORTADO com o comando para tirar, nunca
//!    apagado, e nunca se chama `sudo`.
//! 5. Erro (permissão…) é dito com o caminho exato, a limpeza segue, e o relatório fica
//!    INCOMPLETO. Nada é engolido (piso 4).
//! 6. Nome legado GENÉRICO (`deployer`, `overflow*`) só é apagado em `~/.cargo/bin` e
//!    `~/.local/bin`: em outro diretório do `PATH` pode ser OUTRO programa do usuário.
//! 7. Update que FALHOU não limpa: a cópia antiga é o fallback ([`apos`]).
//! 8. No Windows um `.exe` em uso não se apaga: vira `<nome>.old` e sai na próxima execução.

use super::{AppGerido, Politica};
use crate::nucleo::{plataforma, util};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Prazo de uma consulta `--version` a uma cópia antiga. Estourou, a versão fica "desconhecida".
const PRAZO_VERSAO: Duration = Duration::from_secs(2);

/// **O quê:** um nome de arquivo que pertence a um app, e onde mora a cópia boa dele.
///
/// **Onde:** [`Alvo::itens`], montado por [`alvo_do_app`] e [`alvo_do_hub`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Nome exato do arquivo (com `.exe` no Windows).
    pub nome: String,
    /// Caminhos da cópia BOA. Vazio = nome legado (limpa-se quando o app âncora existe).
    pub frescos: Vec<PathBuf>,
    /// Nome genérico: só se apaga nos diretórios de usuário (regra 6).
    pub generico: bool,
}

/// **O quê:** o que limpar depois de instalar/atualizar um app.
///
/// **Onde:** entrada de [`limpar_em`], [`apos`] e [`apos_sucesso`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Alvo {
    /// Os nomes do app (vivos e legados).
    pub itens: Vec<Item>,
    /// A cópia boa do app: se nenhuma existir, NADA é apagado (não há com o que comparar).
    pub ancora: Vec<PathBuf>,
    /// Repos cujo checkout em `build/` é apagável (o app veio de binário).
    pub repos_de_build: Vec<&'static str>,
    /// Pode apagar o cache de build? Só quando o método do app é `binario`.
    pub limpar_cache: bool,
}

/// **O quê:** o mundo em que a limpeza roda — injetável, para os testes nunca tocarem no real.
///
/// **Onde:** [`limpar_em`]; produção usa [`Ambiente::real`].
pub struct Ambiente {
    /// Diretórios do `PATH` (só os absolutos valem).
    pub path_dirs: Vec<PathBuf>,
    /// `~/.cargo/bin` e `~/.local/bin` — onde nomes genéricos também se apagam.
    pub dirs_usuario: Vec<PathBuf>,
    /// Outros diretórios padrão (`/usr/local/bin`).
    pub dirs_padrao: Vec<PathBuf>,
    /// Diretórios de pacote do sistema: o que há neles é reportado, nunca apagado.
    pub dirs_sistema: Vec<PathBuf>,
    /// `~/.schematize/updater` (cache de build e `dl`).
    pub state_dir: PathBuf,
    /// Caminhos que jamais se apagam (o binário em execução).
    pub protegidos: Vec<PathBuf>,
    /// Remove um arquivo; devolve `Some(novo)` se só o renomeou (Windows, em uso).
    pub remover: fn(&Path) -> io::Result<Option<PathBuf>>,
    /// Qual pacote do sistema é dono do arquivo, se der para saber.
    pub dono_do_pacote: fn(&Path) -> Option<String>,
}

impl Ambiente {
    /// **O quê:** o ambiente desta máquina: `PATH`, `HOME`, diretórios do sistema.
    /// **Onde:** [`apos`] e [`apos_sucesso`].
    pub fn real() -> Self {
        let home = util::home();
        let path_dirs = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let sistema: &[&str] =
            if cfg!(windows) { &[] } else { &["/usr/bin", "/bin", "/usr/sbin", "/sbin"] };
        Ambiente {
            path_dirs,
            dirs_usuario: vec![home.join(".cargo").join("bin"), home.join(".local").join("bin")],
            dirs_padrao: if cfg!(windows) { vec![] } else { vec![PathBuf::from("/usr/local/bin")] },
            dirs_sistema: sistema.iter().map(PathBuf::from).collect(),
            state_dir: plataforma::state_dir(),
            protegidos: std::env::current_exe().into_iter().collect(),
            remover: remover_real,
            dono_do_pacote: dono_do_pacote_real,
        }
    }
}

/// **O quê:** o que a limpeza achou de cada tipo.
/// **Onde:** [`Removido::tipo`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    /// Cópia antiga do binário (mesmo nome ou nome legado).
    Copia,
    /// Sobra de instalação (`.bak-*`, `.old`, `.novo`, `.tmp*`…).
    Sobra,
    /// Cache de build do market sem uso.
    Cache,
}

/// **O quê:** um item removido (ou renomeado, no Windows).
/// **Onde:** [`Relatorio::removidos`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removido {
    /// O caminho que sumiu.
    pub caminho: PathBuf,
    /// A versão antiga, quando deu para saber.
    pub versao: Option<String>,
    /// Que tipo de coisa era.
    pub tipo: Tipo,
    /// `Some(novo)` quando só foi renomeado (Windows, em uso).
    pub renomeado_para: Option<PathBuf>,
}

/// **O quê:** uma cópia que é de pacote do sistema e por isso NÃO foi tocada.
/// **Onde:** [`Relatorio::sistema`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoSistema {
    /// Onde está.
    pub caminho: PathBuf,
    /// O pacote dono, se `rpm`/`dpkg`/`pacman` souberam dizer.
    pub pacote: Option<String>,
}

/// **O quê:** o que a limpeza NÃO conseguiu fazer, com a causa.
/// **Onde:** [`Relatorio::falhas`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Falha {
    /// O caminho exato.
    pub caminho: PathBuf,
    /// A causa, como o SO a disse.
    pub erro: String,
}

/// **O quê:** o resultado de uma limpeza.
/// **Onde:** devolvido por [`limpar_em`], [`apos`] e [`apos_sucesso`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Relatorio {
    /// Tudo que saiu.
    pub removidos: Vec<Removido>,
    /// Cópias de pacote do sistema, intocadas.
    pub sistema: Vec<DoSistema>,
    /// O que resistiu.
    pub falhas: Vec<Falha>,
    /// Avisos que não tornam a limpeza incompleta (ex.: diretório do `PATH` ilegível).
    pub avisos: Vec<String>,
}

impl Relatorio {
    /// **O quê:** a limpeza deixou a máquina sem nenhuma cópia antiga conhecida?
    /// **Onde:** o resumo impresso e os testes. Falha ou cópia de sistema = `false`.
    pub fn completo(&self) -> bool {
        self.falhas.is_empty() && self.sistema.is_empty()
    }

    /// **O quê:** nada achado, nada a dizer. **Onde:** [`Relatorio::imprimir`].
    fn vazio(&self) -> bool {
        self.removidos.is_empty()
            && self.sistema.is_empty()
            && self.falhas.is_empty()
            && self.avisos.is_empty()
    }

    /// **O quê:** as linhas que a pessoa lê: cada remoção, cada pendência e o resumo.
    /// **Onde:** [`Relatorio::imprimir`] e os testes.
    pub fn linhas(&self) -> Vec<String> {
        if self.vazio() {
            return vec![];
        }
        let mut l = Vec::new();
        for r in &self.removidos {
            l.push(linha_removido(r));
        }
        for s in &self.sistema {
            l.push(mensagem_sistema(s));
        }
        for f in &self.falhas {
            let p = f.caminho.display();
            l.push(format!(
                "erro: não consegui remover {p}: {}. Remova à mão: `rm {p}` \
                 (se o arquivo for de outro usuário: `sudo rm {p}`).",
                f.erro
            ));
        }
        for a in &self.avisos {
            l.push(format!("aviso: {a}"));
        }
        let estado = if self.completo() { "COMPLETA" } else { "INCOMPLETA" };
        l.push(format!(
            "limpeza de versões antigas: {} removido(s), {} do pacote do sistema, {} erro(s) — {estado}.",
            self.removidos.len(),
            self.sistema.len(),
            self.falhas.len()
        ));
        l
    }

    /// **O quê:** imprime [`Relatorio::linhas`]. **Onde:** [`apos`] e [`apos_sucesso`].
    pub fn imprimir(&self) {
        for l in self.linhas() {
            println!("{l}");
        }
    }
}

/// **O quê:** a linha `removido: <path> (…)` de um item.
/// **Onde:** [`Relatorio::linhas`].
fn linha_removido(r: &Removido) -> String {
    let p = r.caminho.display();
    let motivo = match (r.tipo, &r.versao) {
        (Tipo::Copia, Some(v)) => format!("versão antiga {v}"),
        (Tipo::Copia, None) => "versão antiga desconhecida".to_string(),
        (Tipo::Sobra, Some(v)) => format!("sobra de instalação, backup da versão {v}"),
        (Tipo::Sobra, None) => "sobra de instalação anterior".to_string(),
        (Tipo::Cache, _) => "cache de build do market sem uso — este app veio do binário".into(),
    };
    match &r.renomeado_para {
        Some(n) => format!(
            "renomeado: {p} -> {} ({motivo}; está em uso, apago na próxima execução)",
            n.display()
        ),
        None => format!("removido: {p} ({motivo})"),
    }
}

/// **O quê:** a mensagem acionável para uma cópia que é de pacote do sistema.
/// **Onde:** [`Relatorio::linhas`].
fn mensagem_sistema(s: &DoSistema) -> String {
    let p = s.caminho.display();
    let como = match (&s.pacote, plataforma::linux_family()) {
        (Some(pk), plataforma::LinuxFam::Debian) => format!("sudo apt remove {pk}"),
        (Some(pk), plataforma::LinuxFam::Rpm) => {
            format!("sudo zypper rm {pk}  (ou, em Fedora/RHEL: sudo dnf remove {pk})")
        }
        (Some(pk), plataforma::LinuxFam::Arch) => format!("sudo pacman -R {pk}"),
        (Some(pk), _) => format!("remova o pacote `{pk}` com o gerenciador da sua distro"),
        (None, _) => "descubra o pacote com `rpm -qf` / `dpkg -S` e remova-o".to_string(),
    };
    format!(
        "instalado pelo pacote do sistema: {p} — não mexo nele (nunca uso sudo). \
         Para tirar a cópia antiga rode: {como}"
    )
}

/// **O quê:** o que um nome de arquivo é em relação a um binário `bin`.
/// **Onde:** [`classificar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Achado {
    /// O próprio binário (ou um nome legado dele).
    Copia,
    /// Sobra de instalação; `Some(versão)` quando o nome diz de que versão era o backup.
    Sobra(Option<String>),
}

/// **O quê:** `arquivo` é o binário `bin` ou uma sobra de instalação dele? Função PURA, por
/// NOME EXATO — nada de glob solto: `schematize-overdev-notes.txt` e `schematize-marketing` não
/// casam com nada.
///
/// **Onde:** [`varrer_dir`]. Os padrões vêm do código real (ver o doc do módulo).
pub fn classificar(arquivo: &str, bin: &str) -> Option<Achado> {
    if arquivo == bin {
        return Some(Achado::Copia);
    }
    let stem = bin.strip_suffix(".exe").unwrap_or(bin);
    for base in [bin, stem] {
        for suf in [".old", ".new", ".novo", ".baixado", ".compilado"] {
            if arquivo.strip_prefix(base).is_some_and(|r| r == suf) {
                return Some(Achado::Sobra(None));
            }
        }
        if let Some(r) = arquivo.strip_prefix(base).and_then(|r| r.strip_prefix(".tmp")) {
            if r.is_empty() || r.starts_with(|c: char| c.is_ascii_digit() || "-._".contains(c)) {
                return Some(Achado::Sobra(None));
            }
        }
        if let Some(r) = arquivo.strip_prefix('.').and_then(|a| a.strip_prefix(base)) {
            if r.starts_with(".tmp-") {
                return Some(Achado::Sobra(None));
            }
        }
        for pre in [format!(".{base}.bak-"), format!("{base}.bak-")] {
            if let Some(v) = arquivo.strip_prefix(&pre).filter(|v| !v.is_empty()) {
                return Some(Achado::Sobra(Some(v.to_string())));
            }
        }
    }
    None
}

/// **O quê:** o nome é de uma janela (`…-gui`, `…-gui.exe`)? Janela NUNCA é executada.
/// **Onde:** [`tratar`].
pub fn eh_janela(nome: &str) -> bool {
    nome.strip_suffix(".exe").unwrap_or(nome).ends_with("-gui")
}

/// **O quê:** para onde o Windows move um `.exe` em uso: `<stem>.old` (a mesma regra de
/// `binario::substitui_binario`, que usa `with_extension("old")`). Pura.
/// **Onde:** [`remover_real`] (Windows) e os testes.
pub fn caminho_old(p: &Path) -> PathBuf {
    p.with_extension("old")
}

/// **O quê:** remove `p`. No Windows, um `.exe` em uso (acesso negado) é renomeado para `.old`.
/// **Onde:** [`Ambiente::real`]. Nunca segue symlink: `remove_file` apaga o link.
pub(crate) fn remover_real(p: &Path) -> io::Result<Option<PathBuf>> {
    match std::fs::remove_file(p) {
        Ok(()) => Ok(None),
        #[cfg(windows)]
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            let velho = caminho_old(p);
            match std::fs::remove_file(&velho) {
                Ok(()) => {}
                Err(x) if x.kind() == io::ErrorKind::NotFound => {}
                Err(x) => return Err(x),
            }
            std::fs::rename(p, &velho).map(|()| Some(velho))
        }
        Err(e) => Err(e),
    }
}

/// **O quê:** roda `programa args…` sem `DISPLAY`, com prazo; devolve a primeira linha do stdout
/// se saiu com sucesso. `None` em qualquer outro caso (não existe, falhou, estourou o prazo — o
/// processo é morto). **Onde:** [`versao_do_cli`] e [`dono_do_pacote_real`].
///
/// Não é erro engolido: o resultado é INFORMAÇÃO opcional do relatório; a remoção em si não
/// depende dela.
fn rodar_com_prazo<S: AsRef<OsStr>>(
    programa: S,
    args: &[&OsStr],
    prazo: Duration,
) -> Option<String> {
    let mut filho = Command::new(programa)
        .args(args)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let inicio = Instant::now();
    loop {
        match filho.try_wait() {
            Ok(Some(st)) if st.success() => {
                let mut s = String::new();
                filho.stdout.take()?.read_to_string(&mut s).ok()?;
                return s.lines().next().map(|l| l.trim().to_string());
            }
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if inicio.elapsed() > prazo => {
                let _ = filho.kill();
                let _ = filho.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

/// **O quê:** a versão que uma cópia CLI antiga responde a `--version` (2 s, sem `DISPLAY`).
/// **Onde:** [`tratar`], só para cópias regulares que não são janela.
fn versao_do_cli(p: &Path) -> Option<String> {
    let linha = rodar_com_prazo(p, &[OsStr::new("--version")], PRAZO_VERSAO)?;
    linha
        .split_whitespace()
        .find(|t| t.starts_with(|c: char| c.is_ascii_digit()))
        .map(|t| t.to_string())
}

/// **O quê:** o pacote do sistema dono de `p` (`rpm -qf`, `dpkg -S`, `pacman -Qoq`), se houver.
/// **Onde:** [`Ambiente::real`]. Nunca chama `sudo`.
fn dono_do_pacote_real(p: &Path) -> Option<String> {
    let prazo = Duration::from_secs(3);
    let arq = p.as_os_str();
    let rpm = [OsStr::new("-qf"), OsStr::new("--qf"), OsStr::new("%{NAME}"), arq];
    if let Some(n) = rodar_com_prazo("rpm", &rpm, prazo) {
        return Some(n);
    }
    if let Some(l) = rodar_com_prazo("dpkg", &[OsStr::new("-S"), arq], prazo) {
        return l.split(':').next().map(|s| s.trim().to_string());
    }
    rodar_com_prazo("pacman", &[OsStr::new("-Qoq"), arq], prazo)
}

/// **O quê:** monta o [`Alvo`] de um app da tabela: janela, nome legado e — para o market — o
/// updater aposentado. `limpar_cache` = o app veio de binário (checkout em `build/` sem uso).
///
/// **Onde:** [`apos`], pelos pontos de instalação/atualização de [`super::componentes`] e
/// [`crate::appsdacasa`].
pub fn alvo_do_app(app: &AppGerido, limpar_cache: bool) -> Alvo {
    let sfx = plataforma::exe_suffix();
    let mut frescos = vec![app.caminho()];
    let mut ancora = vec![app.caminho()];
    let mut itens = Vec::new();
    let mut repos: Vec<&'static str> = vec![app.repo];
    if app.politica == Politica::Sempre {
        // O market em execução é, por definição, a cópia boa — e é onde o self-update gravou.
        if let Ok(eu) = std::env::current_exe() {
            frescos.push(eu.clone());
            ancora.push(eu);
        }
        let gui = plataforma::install_dir().join(plataforma::gestor_gui_bin());
        itens.push(Item {
            nome: plataforma::gestor_gui_bin(),
            frescos: vec![gui],
            generico: false,
        });
        for legado in [plataforma::updater_bin_name(), plataforma::gestor_gui_bin_legado()] {
            itens.push(Item { nome: legado, frescos: vec![], generico: false });
        }
        for g in ["overflow-updater", "overflow-updater-gui"] {
            itens.push(Item { nome: format!("{g}{sfx}"), frescos: vec![], generico: true });
        }
        repos.push(plataforma::GESTOR_GUI_REPO);
    }
    itens.push(Item { nome: app.bin_name(), frescos, generico: false });
    if let Some((nome, dst)) = app.gui_nome_e_destino() {
        itens.push(Item { nome, frescos: vec![dst], generico: false });
    }
    if let Some(l) = app.legado {
        itens.push(Item { nome: format!("{l}{sfx}"), frescos: vec![], generico: true });
    }
    Alvo { itens, ancora, repos_de_build: repos, limpar_cache }
}

/// **O quê:** monta o [`Alvo`] do hub (`schematize` + `schematize-gui`), com os nomes do
/// interregno Overflow como legado genérico.
///
/// **Onde:** [`apos`] no fim de [`super::componentes`]`::finish` e no "em dia" do hub.
pub fn alvo_do_hub(limpar_cache: bool) -> Alvo {
    let sfx = plataforma::exe_suffix();
    let (cli, gui) = plataforma::bin_names();
    let dir = plataforma::install_dir();
    let itens = vec![
        Item { nome: cli.clone(), frescos: vec![dir.join(&cli)], generico: false },
        Item { nome: gui.clone(), frescos: vec![dir.join(&gui)], generico: false },
        Item { nome: format!("overflow{sfx}"), frescos: vec![], generico: true },
        Item { nome: format!("overflow-gui{sfx}"), frescos: vec![], generico: true },
    ];
    Alvo {
        itens,
        ancora: vec![dir.join(&cli)],
        repos_de_build: vec![plataforma::APP_REPO, plataforma::GUI_REPO],
        limpar_cache,
    }
}

/// **O quê:** o caminho existe e, canonizado, é o mesmo de algum protegido?
/// **Onde:** [`tratar`].
fn protegido(caminho: &Path, protegidos: &BTreeSet<PathBuf>) -> bool {
    caminho.canonicalize().is_ok_and(|c| protegidos.contains(&c))
}

/// **O quê:** o CONJUNTO canonizado do que não se apaga: cópias boas existentes, âncoras e os
/// protegidos do ambiente. **Onde:** [`limpar_em`].
fn conjunto_protegido(amb: &Ambiente, alvo: &Alvo) -> BTreeSet<PathBuf> {
    let todos = alvo
        .itens
        .iter()
        .flat_map(|i| i.frescos.iter())
        .chain(alvo.ancora.iter())
        .chain(amb.protegidos.iter());
    todos.filter_map(|p| p.canonicalize().ok()).collect()
}

/// **O quê:** executa a limpeza em `amb` para `alvo`. É o motor; não imprime.
///
/// **Onde:** [`apos_em`] e os testes. Sem nenhuma cópia âncora existente, não faz nada.
pub fn limpar_em(amb: &Ambiente, alvo: &Alvo) -> Relatorio {
    let mut rel = Relatorio::default();
    if !alvo.ancora.iter().any(|p| p.is_file()) {
        return rel;
    }
    let prot = conjunto_protegido(amb, alvo);
    let itens: Vec<&Item> = alvo
        .itens
        .iter()
        .filter(|i| i.frescos.is_empty() || i.frescos.iter().any(|p| p.is_file()))
        .collect();
    let canon = |ds: &[PathBuf]| -> Vec<PathBuf> {
        ds.iter().filter_map(|d| d.canonicalize().ok()).collect()
    };
    let (usuario, sistema) = (canon(&amb.dirs_usuario), canon(&amb.dirs_sistema));
    let candidatos = amb
        .path_dirs
        .iter()
        .filter(|d| d.is_absolute())
        .chain(&amb.dirs_usuario)
        .chain(&amb.dirs_padrao)
        .chain(&amb.dirs_sistema);
    let mut vistos = BTreeSet::new();
    for dir in candidatos {
        let c = match dir.canonicalize() {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                rel.avisos.push(format!("não consegui resolver {}: {e}", dir.display()));
                continue;
            }
        };
        if !vistos.insert(c.clone()) {
            continue;
        }
        let ctx = Contexto {
            amb,
            prot: &prot,
            de_usuario: usuario.contains(&c),
            de_sistema: sistema.contains(&c),
        };
        varrer_dir(&ctx, dir, &itens, &mut rel);
    }
    limpar_cache(amb, alvo, &mut rel);
    rel
}

/// **O quê:** o que [`varrer_dir`] e [`tratar`] precisam saber do diretório em curso.
/// **Onde:** [`limpar_em`].
struct Contexto<'a> {
    amb: &'a Ambiente,
    prot: &'a BTreeSet<PathBuf>,
    de_usuario: bool,
    de_sistema: bool,
}

/// **O quê:** lista `dir` (em ordem alfabética) e trata cada arquivo que casa com algum item.
/// **Onde:** [`limpar_em`].
fn varrer_dir(ctx: &Contexto, dir: &Path, itens: &[&Item], rel: &mut Relatorio) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            rel.avisos.push(format!("não consegui listar {}: {e}", dir.display()));
            return;
        }
    };
    let mut nomes = Vec::new();
    for ent in rd {
        match ent {
            Ok(e) => nomes.extend(e.file_name().to_str().map(String::from)),
            Err(e) => rel.avisos.push(format!("leitura de {} falhou: {e}", dir.display())),
        }
    }
    nomes.sort();
    for nome in nomes {
        let achado = itens
            .iter()
            .filter(|i| !i.generico || ctx.de_usuario)
            .find_map(|i| classificar(&nome, &i.nome));
        if let Some(achado) = achado {
            tratar(ctx, &dir.join(&nome), &achado, rel);
        }
    }
}

/// **O quê:** decide o destino de UM arquivo casado: manter, reportar (sistema) ou remover.
///
/// **Onde:** [`varrer_dir`]. Symlink: só o link sai, e fica se resolver para um protegido.
/// Diretório nunca é tocado.
fn tratar(ctx: &Contexto, caminho: &Path, achado: &Achado, rel: &mut Relatorio) {
    let md = match std::fs::symlink_metadata(caminho) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return,
        Err(e) => {
            rel.falhas.push(Falha { caminho: caminho.to_path_buf(), erro: e.to_string() });
            return;
        }
    };
    let ft = md.file_type();
    if ft.is_dir() || protegido(caminho, ctx.prot) {
        return;
    }
    if ctx.de_sistema {
        let pacote = (ctx.amb.dono_do_pacote)(caminho);
        rel.sistema.push(DoSistema { caminho: caminho.to_path_buf(), pacote });
        return;
    }
    let nome = caminho.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let (tipo, versao) = match achado {
        Achado::Sobra(v) => (Tipo::Sobra, v.clone()),
        Achado::Copia if ft.is_symlink() || eh_janela(nome) || !ft.is_file() => (Tipo::Copia, None),
        Achado::Copia => (Tipo::Copia, versao_do_cli(caminho)),
    };
    match (ctx.amb.remover)(caminho) {
        Ok(renomeado_para) => rel.removidos.push(Removido {
            caminho: caminho.to_path_buf(),
            versao,
            tipo,
            renomeado_para,
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => rel.falhas.push(Falha { caminho: caminho.to_path_buf(), erro: e.to_string() }),
    }
}

/// **O quê:** apaga o checkout do app em `build/` (e `dl/`), quando o app veio de BINÁRIO, e o
/// `target/` compartilhado quando não sobrou nenhum checkout. Só dentro de `state_dir/build`.
///
/// **Onde:** [`limpar_em`]. Diretórios do `state_dir` que não são cache (o `pin`) nunca entram.
fn limpar_cache(amb: &Ambiente, alvo: &Alvo, rel: &mut Relatorio) {
    if !alvo.limpar_cache {
        return;
    }
    let build = amb.state_dir.join("build");
    for repo in &alvo.repos_de_build {
        let nome = repo.rsplit('/').next().unwrap_or_default();
        if nome.is_empty() || nome == "target" || nome.starts_with('.') {
            continue;
        }
        remover_dir_de_cache(&build.join(nome), rel);
    }
    match std::fs::read_dir(&build) {
        Ok(rd) => {
            let sobrou = rd
                .flatten()
                .any(|e| e.file_name() != "target" && e.file_type().is_ok_and(|t| t.is_dir()));
            if !sobrou {
                remover_dir_de_cache(&build.join("target"), rel);
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => rel.falhas.push(Falha { caminho: build, erro: e.to_string() }),
    }
    remover_dir_de_cache(&amb.state_dir.join("dl"), rel);
}

/// **O quê:** remove UM diretório de cache, se for um diretório de verdade (nunca symlink).
/// **Onde:** [`limpar_cache`].
fn remover_dir_de_cache(d: &Path, rel: &mut Relatorio) {
    match std::fs::symlink_metadata(d) {
        Ok(m) if m.file_type().is_dir() => match std::fs::remove_dir_all(d) {
            Ok(()) => rel.removidos.push(Removido {
                caminho: d.to_path_buf(),
                versao: None,
                tipo: Tipo::Cache,
                renomeado_para: None,
            }),
            Err(e) => rel.falhas.push(Falha { caminho: d.to_path_buf(), erro: e.to_string() }),
        },
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => rel.falhas.push(Falha { caminho: d.to_path_buf(), erro: e.to_string() }),
    }
}

/// **O quê:** limpa SE `resultado` for sucesso; com falha NÃO toca em nada (a cópia antiga é o
/// fallback de um update que não terminou). Devolve `None` quando pulou.
///
/// **Onde:** os pontos de instalação que têm um `Result` na mão; testável via ambiente injetado.
pub fn apos_em<T, E>(amb: &Ambiente, resultado: &Result<T, E>, alvo: &Alvo) -> Option<Relatorio> {
    resultado.as_ref().ok()?;
    Some(limpar_em(amb, alvo))
}

/// **O quê:** [`apos_em`] no ambiente REAL, imprimindo o relatório.
/// **Onde:** os pontos de [`super::componentes`] que têm um `Result`.
pub fn apos<T, E>(resultado: &Result<T, E>, alvo: &Alvo) -> Option<Relatorio> {
    let rel = apos_em(&Ambiente::real(), resultado, alvo)?;
    rel.imprimir();
    Some(rel)
}

/// **O quê:** limpa no ambiente real, para quem já está num ramo de sucesso. Imprime.
/// **Onde:** fim de instalação/atualização bem-sucedida.
pub fn apos_sucesso(alvo: &Alvo) -> Relatorio {
    let rel = limpar_em(&Ambiente::real(), alvo);
    rel.imprimir();
    rel
}

#[cfg(test)]
mod tests;
