//! **Receitas** — o que o market sabe fazer por SO, atrás de um trait (ADR-0026).
//!
//! **O quê:** [`Receitas`] reúne tudo o que muda por sistema operacional e não cabe nas
//! costuras genéricas de `schematize_plataforma`: as receitas de instalação/remoção de
//! linguagens e ferramentas, a família do SO, a regra de métodos disponíveis, a execução de
//! um passo, o bootstrap do toolchain de Rust e as sondas HTTP do GitHub.
//! [`crate::receitas_unix::ReceitasUnix`] é a implementação Linux/FreeBSD/macOS (o código de
//! sempre, intacto); o repo `schematize_windows_rs` provê a do Windows (winget/PowerShell).
//!
//! **Onde:** montado no `main` do binário e entregue a [`crate::cli::run`]; desce pelo
//! [`crate::ctx::Ctx`].

use crate::environments::defs::{Env, Recipe, Step, Tool};
use crate::environments::detect::Family;
use crate::environments::{Machine, Method};
use schematize_plataforma::seams::Plataforma;
use std::path::PathBuf;

/// O que o market faz de forma diferente em cada SO.
pub trait Receitas {
    /// **O quê:** plano de INSTALAÇÃO de uma linguagem por (método × família).
    /// **Onde:** `install`, `switch` e a tabela do `list`. `mise_present` = o `mise` já existe.
    fn install_recipe(&self, env: &Env, method: Method, fam: Family, mise_present: bool) -> Recipe;

    /// **O quê:** plano de REMOÇÃO de uma linguagem. **Onde:** `remove` e `switch`.
    fn remove_recipe(&self, env: &Env, method: Method, fam: Family) -> Recipe;

    /// **O quê:** plano de instalação de uma FERRAMENTA de dev (claude, codex, code, cursor…).
    /// **Onde:** `install <ferramenta>` e o `install --all`.
    fn tool_install_recipe(&self, tool: &Tool, fam: Family) -> Recipe;

    /// **O quê:** plano de remoção de uma ferramenta. **Onde:** `remove <ferramenta>`.
    fn tool_remove_recipe(&self, tool: &Tool, fam: Family) -> Recipe;

    /// **O quê:** a família do SO/distro (decide o gerenciador de pacotes do método `distro`).
    /// **Onde:** [`Machine::probe`], a montagem de todo plano.
    fn family(&self) -> Family;

    /// **O quê:** `Ok` se o método é utilizável nesta máquina; `Err(razão)` caso contrário
    /// (deny-by-default). **Onde:** [`Machine::available`] e o `install --method`.
    fn method_reason(&self, m: &Machine, method: Method) -> Result<(), String>;

    /// **O quê:** executa UM passo do plano (herdando o terminal: saída ao vivo, sudo, senha).
    /// **Onde:** `run_steps` das ações de instalar/remover.
    fn exec_step(&self, plat: &Plataforma, s: &Step) -> Result<(), String>;

    /// **O quê:** garante o Rust/cargo e devolve o caminho do `cargo`.
    /// **Onde:** todo build do fonte (`fonte`, `selfupdate`, `appsdacasa`).
    fn garantir_toolchain(&self, plat: &Plataforma) -> Result<PathBuf, String>;

    /// **O quê:** instala as libs de BUILD (compilador C, pkg-config, X11/GL…) do SO.
    /// **Onde:** antes do primeiro `cargo build` do fonte.
    fn garantir_build_deps(&self, plat: &Plataforma) -> Result<(), String>;

    /// **O quê:** sonda o `releases/latest` de `repo` SEM seguir o redirect e devolve
    /// `"<http_code> <redirect_url>"`; `None` se o cliente HTTP falhou (rede fora).
    /// **Onde:** [`crate::nucleo::rede::url_do_ultimo_release_tipado`].
    fn sondar_ultimo_release(&self, repo: &str) -> Option<String>;

    /// **O quê:** sonda se `url` existe pedindo só o 1º byte; devolve o código HTTP (`"200"`,
    /// `"404"`, …) ou `None` se o cliente falhou. **Onde:** [`crate::nucleo::rede::asset_existe`].
    fn sondar_asset(&self, url: &str) -> Option<String>;

    /// **O quê:** GET de texto com UM cabeçalho extra (`"Accept: ..."`); `None` se falhar ou se
    /// o SO não consegue enviar cabeçalho. **Onde:** a consulta de commit pela API do GitHub
    /// (só quando não há `git`).
    fn texto_com_cabecalho(&self, url: &str, cabecalho: &str) -> Option<String>;
}
