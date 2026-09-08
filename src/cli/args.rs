//! A superfície da CLI do market.
//!
//! **Uma lista só.** O pedido original foi "deployer e optimizer deveriam aparecer no env" —
//! e a resposta não foi somar os apps à lista do `env`, foi reconhecer que `env` e `apps`
//! eram o mesmo produto separado em dois. Aqui `list` mostra tudo: linguagens, ferramentas
//! de dev e os apps do ecossistema.

use clap::{Parser, Subcommand};

/// `schematize-market` — instala e remove programas.
#[derive(Parser)]
#[command(
    name = "schematize-market",
    version,
    about = "schematize market — install and remove programs",
    long_about = "Language runtimes (docker|mise|distro|official), dev tools, and the \
                  ecosystem apps — in one list.\n\
                  Works standalone; integrates with the schematize hub when both are present."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    /// Everything installable, and what is already installed. This is what the icon opens.
    List {
        /// Wait for a keypress at the end (used by the desktop launcher).
        #[arg(long)]
        wait: bool,
    },
    /// Install a language runtime, a dev tool, or an ecosystem app.
    Install {
        /// go|rust|node|… , claude|code|codex, or schematize-deployer|schematize-optimizer|…
        what: String,
        /// For languages: docker|mise|distro|official. Ignored for tools and apps.
        #[arg(long)]
        method: Option<String>,
        /// Show what would happen; change nothing.
        #[arg(long)]
        dry_run: bool,
        /// Do not ask before starting.
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Remove a language runtime or a dev tool (auto-detects how it was installed).
    Remove {
        what: String,
        #[arg(long)]
        method: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Switch how a language is installed. Installs the new method FIRST, then removes the old.
    Switch {
        lang: String,
        #[arg(long = "to")]
        to: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Icon and application-menu entry — so the app opens without the hub.
    Desktop {
        #[arg(long)]
        install: bool,
        #[arg(long)]
        remove: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Serializa a ÁRVORE INTEIRA de comandos num texto determinístico.
    ///
    /// **O quê:** para cada comando e subcomando, em ordem alfabética — o caminho, os
    /// aliases, se é oculto, e cada argumento com nome longo/curto, se é obrigatório, se
    /// recebe valor e qual o default. Nada de texto de ajuda: descrição muda com revisão de
    /// prosa e não é contrato; o que a pessoa DIGITA é.
    ///
    /// **Onde:** [`superficie_da_cli_nao_mudou`], contra um snapshot commitado.
    fn superficie(c: &clap::Command, caminho: &str, out: &mut Vec<String>) {
        let nome = if caminho.is_empty() {
            c.get_name().to_string()
        } else {
            format!("{caminho} {}", c.get_name())
        };
        let mut aliases: Vec<_> = c.get_all_aliases().collect();
        aliases.sort_unstable();
        out.push(format!(
            "CMD {nome}{}{}",
            if aliases.is_empty() {
                String::new()
            } else {
                format!(" aliases=[{}]", aliases.join(","))
            },
            if c.is_hide_set() { " (oculto)" } else { "" }
        ));
        // `SOBRE` é a descrição — vai pro arquivo (é dela que o índice de funcionalidades
        // se alimenta) mas FICA DE FORA da comparação: prosa muda em revisão de texto e não
        // é contrato. Quem quebra script é a linha `CMD`/`ARG`, não o `about`.
        if let Some(sobre) = c.get_about() {
            let t = sobre.to_string();
            out.push(format!("  SOBRE {}", t.lines().next().unwrap_or("").trim()));
        }

        let mut args: Vec<String> = c
            .get_arguments()
            .map(|a| {
                let longo = a.get_long().map(|l| format!("--{l}")).unwrap_or_default();
                let curto = a.get_short().map(|s| format!(" -{s}")).unwrap_or_default();
                let val = if a.get_num_args().map(|n| n.takes_values()).unwrap_or(false) {
                    " <valor>"
                } else {
                    ""
                };
                let obrig = if a.is_required_set() { " OBRIGATORIO" } else { "" };
                format!("  ARG {:<20} {longo}{curto}{val}{obrig}", a.get_id().to_string())
            })
            .collect();
        args.sort();
        out.extend(args);

        let mut subs: Vec<&clap::Command> = c.get_subcommands().collect();
        subs.sort_by_key(|s| s.get_name());
        for s in subs {
            superficie(s, &nome, out);
        }
    }

    /// A superfície da CLI é CONTRATO com quem escreveu script, hook e documentação.
    ///
    /// **Onde:** roda a cada `cargo test`, e existe pra que refatorar este módulo seja
    /// seguro — este app nasceu de um CORTE (ADR-0012), e a prova de que o corte não
    /// mudou o que a pessoa digita é esta, não a leitura do diff.
    ///
    /// **Se este teste falhar** e a mudança for INTENCIONAL (comando novo, flag nova),
    /// regenere com `MARKET_REGRAVA_SUPERFICIE=1 cargo test superficie_da_cli` e leia o
    /// diff **linha por linha** antes de commitar: cada linha some é um script de alguém
    /// quebrando. Se foi acidente de refatoração, o teste acabou de fazer o trabalho dele.
    #[test]
    fn superficie_da_cli_nao_mudou() {
        let mut linhas = Vec::new();
        superficie(&Cli::command(), "", &mut linhas);
        let atual = linhas.join("\n") + "\n";

        let snap = std::path::Path::new("tests/superficie-cli.txt");
        if std::env::var_os("MARKET_REGRAVA_SUPERFICIE").is_some() {
            std::fs::write(snap, &atual).expect("gravar o snapshot");
            return;
        }
        let esperado = std::fs::read_to_string(snap).expect(
            "tests/superficie-cli.txt ausente — gere com \
             MARKET_REGRAVA_SUPERFICIE=1 cargo test superficie_da_cli",
        );
        // A ASSERÇÃO é só sobre o contrato: `CMD` e `ARG`. As linhas `SOBRE` viajam no
        // arquivo pra alimentar o índice de funcionalidades, e mudam livremente com revisão
        // de prosa — descrição não quebra o script de ninguém.
        let contrato = |t: &str| -> Vec<String> {
            t.lines().filter(|l| !l.trim_start().starts_with("SOBRE ")).map(String::from).collect()
        };
        if contrato(&atual) == contrato(&esperado) {
            // Só a prosa mudou: regrava sem reprovar.
            if atual != esperado {
                std::fs::write(snap, &atual).expect("regravar a prosa do snapshot");
            }
            return;
        }
        // Diff legível: a primeira divergência é o que a pessoa precisa ver.
        let (a, e) = (contrato(&atual), contrato(&esperado));
        let sumiram: Vec<_> = e.iter().filter(|l| !a.contains(l)).collect();
        let surgiram: Vec<_> = a.iter().filter(|l| !e.contains(l)).collect();
        panic!(
            "a superfície da CLI MUDOU.\n\nsumiram ({}) — cada uma é um script de alguém \
             quebrando:\n  {}\n\nsurgiram ({}):\n  {}\n\nSe foi intencional: \
             MARKET_REGRAVA_SUPERFICIE=1 cargo test superficie_da_cli",
            sumiram.len(),
            sumiram.iter().map(|s| s.to_string()).collect::<Vec<_>>().join("\n  "),
            surgiram.len(),
            surgiram.iter().map(|s| s.to_string()).collect::<Vec<_>>().join("\n  "),
        );
    }
}
