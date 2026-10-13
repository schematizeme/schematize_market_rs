//! PLANO e execução: imprime o que vai ser feito, pede consentimento e roda os
//! passos. Nada é executado sem o plano ter sido mostrado (consentimento honesto).

use super::*;

/// Imprime o plano de uma LINGUAGEM (título + passos).
pub(crate) fn print_plan(env: &Env, method: Method, steps: &[Step]) {
    println!("{}", tf("env.plan_title", &[("lang", env.display), ("method", method.slug())]));
    print_steps(steps);
}

/// Imprime o plano de uma FERRAMENTA (título próprio + passos).
pub(crate) fn print_tool_plan(tool: &Tool, steps: &[Step]) {
    println!("{}", tf("env.tool_plan_title", &[("tool", tool.display)]));
    print_steps(steps);
}

/// Imprime a lista de passos (comando exato + procedência + selos sudo/pipe|sh).
/// Compartilhado por linguagens e ferramentas — o guardrail é o mesmo pra ambos.
pub(crate) fn print_steps(steps: &[Step]) {
    for s in steps {
        if s.source == "nota" {
            // passo informativo (no-op) — mostra só a orientação, não como comando.
            let msg = s.cmd.trim_start_matches(": # ");
            println!("    · {msg}");
            continue;
        }
        let mut tags = String::new();
        if s.sudo {
            tags.push_str(&format!(" {}", t("env.tag_sudo")));
        }
        if s.pipe_sh {
            tags.push_str(&format!(" {}", t("env.tag_pipe")));
        }
        println!("    $ {}{}", s.cmd, tags);
        println!("        {} {}", t("env.source"), s.source);
    }
}

/// Pergunta interativa de consentimento (y/N). Falha fechada: erro/EOF = não.
pub fn confirm() -> bool {
    print!("{} ", t("env.confirm"));
    let _ = io::stdout().flush();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes" | "s" | "sim")
}

/// Mensagem de quando não há terminal para perguntar e `-y` não foi dado.
pub const MSG_SEM_TERMINAL: &str =
    "sem terminal para confirmar; rode de novo com -y para seguir (ou --dry-run para só ver o plano)";

/// **O quê:** decide o consentimento SEM nunca cancelar em silêncio: `-y` segue; sem `-y` e com
/// terminal, pergunta (via `perguntar`); sem `-y` e sem terminal, devolve erro acionável que
/// nomeia o `-y` — a pergunta nem é feita.
///
/// **Onde:** [`consentir`] (stdin real) e a suíte (`tem_tty` injetado).
pub fn consentir_com(
    yes: bool,
    tem_tty: bool,
    perguntar: impl FnOnce() -> bool,
) -> Result<bool, String> {
    if yes {
        return Ok(true);
    }
    if !tem_tty {
        return Err(MSG_SEM_TERMINAL.to_string());
    }
    Ok(perguntar())
}

/// **O quê:** [`consentir_com`] com o `stdin` real e a pergunta [`confirm`].
/// **Onde:** `install` de linguagem/ferramenta ([`acoes`]) antes do plano ser executado.
pub fn consentir(yes: bool) -> Result<bool, String> {
    consentir_com(yes, io::IsTerminal::is_terminal(&io::stdin()), confirm)
}

/// O que fazer com um plano depois de exibido — decisão PURA (testável sem I/O).
#[derive(Debug, PartialEq, Eq)]
pub enum PlanAction {
    /// dry-run: nada executado.
    DryRun,
    /// sem consentimento: abortado, nada executado.
    Aborted,
    /// executado (todos os passos rodaram).
    Executed,
}

/// Núcleo testável da execução: decide e roda os passos via o runner injetado.
/// Com `dry_run` NUNCA chama o runner; sem `consent` também não. Retorna o que ocorreu.
pub fn run_steps<R>(
    steps: &[Step],
    dry_run: bool,
    consent: bool,
    mut run: R,
) -> Result<PlanAction, String>
where
    R: FnMut(&Step) -> Result<(), String>,
{
    if dry_run {
        return Ok(PlanAction::DryRun);
    }
    if !consent {
        return Ok(PlanAction::Aborted);
    }
    for s in steps {
        run(s)?;
    }
    Ok(PlanAction::Executed)
}

/// Executa um passo de verdade (a menos que seja nota/no-op).
pub(crate) fn exec_step(s: &Step) -> Result<(), String> {
    if s.source == "nota" {
        return Ok(());
    }
    util::run_shell(&s.cmd)
}

/// Imprime como USAR a imagem docker recém-baixada (docker run + snippet devcontainer).
pub(crate) fn print_docker_usage(env: &Env, img: &str) {
    println!("{}", t("env.docker_usage"));
    println!("    $ docker run --rm -it -v \"$PWD\":/work -w /work {img} {}", env.bin);
    println!("    .devcontainer/devcontainer.json:");
    println!("      {{ \"image\": \"{img}\" }}");
}

/// Avisa (uma vez) que o `--method` é ignorado pra ferramentas.
pub(crate) fn warn_method_ignored(tool: &Tool, method: &Option<String>) {
    if let Some(mm) = method {
        println!("{}", tf("env.tool_method_ignored", &[("tool", tool.display), ("method", mm)]));
    }
}

#[cfg(test)]
mod testes_consentimento {
    use super::*;

    /// Sem TTY e sem `-y`: erro que nomeia `-y`, e a pergunta NUNCA é feita.
    #[test]
    fn sem_tty_sem_yes_erra_nomeando_o_y() {
        let e =
            consentir_com(false, false, || panic!("não pode perguntar sem terminal")).unwrap_err();
        assert!(e.contains("-y"), "{e}");
    }

    /// `-y` segue com ou sem terminal, sem perguntar.
    #[test]
    fn yes_segue_sem_perguntar() {
        for tty in [true, false] {
            assert_eq!(consentir_com(true, tty, || panic!("não pergunta")), Ok(true));
        }
    }

    /// Com terminal e sem `-y`, a resposta da pessoa decide.
    #[test]
    fn com_tty_a_resposta_decide() {
        assert_eq!(consentir_com(false, true, || true), Ok(true));
        assert_eq!(consentir_com(false, true, || false), Ok(false));
    }
}
