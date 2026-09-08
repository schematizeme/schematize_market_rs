//! OS APPS DA CASA — descoberta e instalação dos apps do ecossistema.
//!
//! **O quê:** descobre se o Deployer está instalado, qual versão, e o invoca como
//! subprocesso quando o usuário pede.
//!
//! **Onde:** `schematize deployer <sub>` (CLI) e, adiante, o card da GUI.
//!
//! ## As três regras desta ponte
//!
//! **1. A ausência do Deployer nunca derruba o schematize** (piso 10). Todo caminho aqui
//! devolve "não instalado" como *estado*, não como erro. O schematize funcionava sem ele
//! ontem e continua funcionando hoje — a ponte é conveniência, não dependência.
//!
//! **2. Subprocesso, não biblioteca.** Ligar os dois crates faria o schematize compilar o
//! Deployer junto, e aí não haveria dois apps: haveria um monólito com dois nomes. É o que o
//! ADR-0010 decidiu evitar.
//!
//! **3. O Deployer nunca devolve segredo por aqui — e é o ponto.** Esta ponte lê **versão** e
//! **código de saída**. Se um dia alguém quiser trazer a passphrase do cofre ou o conteúdo de
//! uma chave por este canal, a resposta é não: o motivo de o Deployer existir é justamente
//! que o segredo **não** transite pelo processo onde o agente opera.

use crate::agentrun::resolve_bin;
use std::path::PathBuf;

/// Um app EXTERNO do ecossistema — instalável à parte, e que o schematize apenas conhece.
///
/// **Por que uma tabela e não um módulo por app:** a lógica de descobrir, versionar e
/// instalar é idêntica; duplicá-la por app seria a divergência esperando acontecer. O que
/// muda entre eles é só o nome do binário, a flag e a frase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppExterno {
    /// Nome do binário no `$PATH`.
    pub bin: &'static str,
    /// Uma linha sobre o que ele faz — vai no `status`.
    pub sobre: &'static str,
}

// O campo `flag` (a flag do `install.sh` que instalava este app) SAIU — ver
// [`instalar_do_fonte`]. Ele só existia para montar o `curl | bash`, que deixou de ser o
// caminho de instalação.

/// Os apps externos que o schematize conhece.
pub const EXTERNOS: &[AppExterno] = &[
    AppExterno {
        bin: "schematize-deployer",
        sobre: "SSH, VPS, DNS e cofre — opera servidor com a credencial fora do agente",
    },
    AppExterno {
        bin: "schematize-optimizer",
        sobre: "mede o ambiente de dev e põe cada software no seu teto de recurso",
    },
];

// O `schematize-skills` e o `schematize-overdev` NÃO estão aqui, e é de propósito: eles ainda
// não existem. Listá-los daria uma linha "não instalado" que a pessoa tentaria instalar, e o
// `install` mandaria uma flag que o `install.sh` não conhece.
//
// Mais que isso: o ADR-0012 diz que eles podem **não sair** — são 11 e 8 consumidores no
// núcleo do hub, e a decisão foi que um overdev meio extraído é pior que um hub com overdev
// dentro. Anunciar aqui o que talvez nunca exista seria prometer pela tabela o que a decisão
// se recusou a prometer. Entram quando forem repositório com release, não antes.

/// **O quê:** acha um app externo pelo nome do binário.
/// **Onde:** a CLI, ao despachar `schematize <app> …`.
pub fn externo(bin: &str) -> Option<&'static AppExterno> {
    EXTERNOS.iter().find(|a| a.bin == bin)
}

// As constantes `BIN`, `REPO` e `INSTALL_SH` saíram daqui (ADR-0013). As duas primeiras
// duplicavam o que a tabela `atualizar::APPS_GERIDOS` já diz — e duas fontes para o mesmo
// fato é como o nome do binário do Deployer ficou errado por um release inteiro. A terceira
// era a URL do `curl | bash`, que deixou de existir como caminho de instalação.

/// O que se sabe do Deployer nesta máquina.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Estado {
    /// Instalado, com o caminho e a versão que ele reporta.
    Instalado { caminho: PathBuf, versao: String },
    /// Não está no `$PATH` nem nos diretórios de fallback.
    Ausente,
    /// O binário existe mas não respondeu `--version` — instalação quebrada.
    ///
    /// É um terceiro estado de propósito: dizer "ausente" para um binário que ESTÁ lá mandaria
    /// a pessoa reinstalar o que já tem, e esconderia a causa real (permissão, biblioteca
    /// faltando, arquitetura errada).
    Quebrado { caminho: PathBuf, erro: String },
}

impl Estado {
    /// **O quê:** `true` só quando dá para usar o Deployer agora.
    /// **Onde:** a GUI e o `doctor`, para decidir entre "abrir" e "instalar".
    pub fn utilizavel(&self) -> bool {
        matches!(self, Estado::Instalado { .. })
    }
}

/// **O quê:** descobre o Deployer nesta máquina.
///
/// **Onde:** `schematize deployer status`, o `doctor` e a GUI.
///
/// Usa o mesmo `resolve_bin` do resto do app — que varre o `$PATH` **e** os diretórios de
/// fallback. Sem isso, o app aberto pelo lançador do desktop (que dá PATH mínimo) diria
/// "não instalado" sobre um Deployer que está em `~/.cargo/bin`.
pub fn descobrir_app(bin: &str) -> Estado {
    let Some(caminho) = resolve_bin(bin) else {
        return Estado::Ausente;
    };
    match crate::util::run(&caminho.to_string_lossy(), &["--version"]) {
        Ok(saida) => {
            let versao = saida.split_whitespace().nth(1).unwrap_or("?").to_string();
            Estado::Instalado { caminho, versao }
        }
        Err(e) => Estado::Quebrado { caminho, erro: e },
    }
}

/// **O quê:** instala um app da casa COMPILANDO do fonte, aqui mesmo. `Err` com a causa se
/// não der.
///
/// **Onde:** `schematize-market install <app>`, e o `install.sh --deployer|--optimizer`, que
/// agora delega para cá.
///
/// ## O LOOP que esta função existe para desfazer (ADR-0013, D1)
///
/// O que estava aqui montava `curl -fsSL <install.sh> | bash -s -- --deployer` e executava.
/// Enquanto o `install.sh` era o único que sabia compilar, isso era razoável. Deixou de ser
/// por duas razões, e a segunda é fatal:
///
/// 1. **O market já sabe compilar.** [`crate::atualizar::fonte::build_one`] faz exatamente o
///    que aquele trecho do script fazia — checkout persistente, `target/` compartilhado, build
///    incremental. Manter os dois era manter duas verdades sobre "como se instala um app da
///    casa", que é a duplicação que o ADR-0013 existe para acabar.
/// 2. **O `install.sh` passou a delegar para cá.** Se esta função continuasse chamando o
///    script, `install.sh --deployer` → `market install` → `install.sh --deployer` seria um
///    **loop infinito**, recompilando o mundo a cada volta.
///
/// De quebra some um `curl | bash` de dentro de uma ferramenta: baixar e executar shell
/// arbitrário no meio de um programa é o tipo de elo que uma cadeia de suprimentos não
/// precisa ter.
///
/// **Herda o terminal de propósito:** a compilação leva minutos e pode pedir sudo para as
/// libs de build. Capturar a saída deixaria a pessoa olhando um cursor parado, e o pedido de
/// senha não teria onde aparecer.
pub fn instalar_do_fonte(bin: &str) -> Result<(), String> {
    let app = crate::atualizar::APPS_GERIDOS
        .iter()
        .find(|a| a.bin == bin)
        .ok_or_else(|| format!("não sei de que repositório vem o `{bin}`"))?;

    let cargo = crate::nucleo::plataforma::ensure_toolchain()?;
    // As libs de build do sistema: o Deployer e o Optimizer não desenham janela, mas o
    // `ensure_build_deps` também traz o compilador C e o `pkg-config`, sem os quais nem as
    // deps nativas comuns linkam. É o mesmo passo que o `install.sh` fazia antes de compilar.
    crate::nucleo::plataforma::ensure_build_deps()?;

    crate::atualizar::fonte::build_one(
        cargo.to_str().unwrap_or("cargo"),
        app.repo,
        &[],
        None,
        &app.bin_name(),
        &app.caminho(),
    )
}

/// **O quê:** põe o app recém-instalado no menu de aplicativos. Best-effort, mas **nunca
/// mudo**.
///
/// **Onde:** depois de [`instalar_do_fonte`], uma vez por app.
///
/// **Por que best-effort e por que falante:** sem ícone é chato; derrubar a instalação por
/// causa dele é pior. Mas best-effort **não é mudo** — o `install.sh` chamava
/// `desktop --instalar >/dev/null 2>&1`, e quando a CLI dos apps foi traduzida a flag virou
/// `--install`: as chamadas passaram a falhar **em silêncio** e dois apps sumiram do menu sem
/// uma linha de erro. Aqui a falha diz o comando exato para repetir à mão.
pub fn registrar_no_menu(bin: &str) {
    let caminho = match resolve_bin(bin) {
        Some(p) => p,
        None => return,
    };
    match crate::util::run(&caminho.to_string_lossy(), &["desktop", "--install"]) {
        Ok(_) => println!("✓ {bin} já aparece no menu de aplicativos."),
        Err(e) => {
            println!("aviso: {bin} foi instalado, mas não consegui pôr o ícone no menu.");
            println!("  o que falhou: {} desktop --install", caminho.display());
            println!("  disse: {}", e.lines().next().unwrap_or("").trim());
            println!("  rode o comando acima para tentar de novo; o app funciona pelo");
            println!("  terminal do mesmo jeito.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `utilizavel` só é verdade no estado que de fato dá para usar. Sem esta distinção, um
    /// binário quebrado seria tratado como bom e a GUI abriria o que não abre.
    #[test]
    fn so_o_instalado_e_utilizavel() {
        let bom = Estado::Instalado { caminho: "/x/deployer".into(), versao: "0.2.1".into() };
        assert!(bom.utilizavel());
        assert!(!Estado::Ausente.utilizavel());
        let ruim = Estado::Quebrado { caminho: "/x/deployer".into(), erro: "libc".into() };
        assert!(!ruim.utilizavel(), "binário que não responde não pode contar como instalado");
    }

    /// **O LOOP, travado por teste.** Nenhum app da casa se instala por `curl | bash` do
    /// `install.sh` — o market compila ele mesmo. Se alguém reintroduzir aquele caminho, o
    /// `install.sh --deployer` (que hoje delega para cá) volta a reentrar em si mesmo.
    #[test]
    fn instalar_app_da_casa_nao_passa_por_curl_nem_pelo_install_sh() {
        let fonte = include_str!("appsdacasa.rs");
        // Só o código de PRODUÇÃO: este próprio teste cita os literais proibidos (é o que
        // ele procura), e os comentários acima explicam por que o `curl | bash` saiu —
        // citá-los ali é o contrário de um bug.
        let producao = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
        for (n, linha) in producao.lines().enumerate() {
            let l = linha.trim();
            if l.starts_with("//") || l.starts_with("///") {
                continue;
            }
            for proibido in ["curl ", "install.sh", "bash -s"] {
                assert!(
                    !l.contains(proibido),
                    "linha {}: `{proibido}` voltou ao caminho de instalar app da casa — \
                     é o loop do ADR-0013",
                    n + 1
                );
            }
        }
    }

    /// Todo app instalável tem de ter repo conhecido — senão [`instalar_do_fonte`] não sabe de
    /// onde compilar e a pessoa recebe um erro em vez de um app.
    #[test]
    fn todo_app_instalavel_sabe_de_onde_vem() {
        for e in EXTERNOS {
            let a = crate::atualizar::APPS_GERIDOS.iter().find(|a| a.bin == e.bin);
            assert!(a.is_some(), "{} é instalável mas não tem repo em APPS_GERIDOS", e.bin);
            assert!(a.unwrap().repo.contains('/'), "{}: repo inválido", e.bin);
        }
    }

    /// **A regra que a ponte existe para cumprir:** descobrir NUNCA falha. Numa máquina sem
    /// Deployer o resultado é `Ausente` — um estado —, não um erro que suba pro chamador.
    #[test]
    fn descobrir_nunca_falha_mesmo_sem_deployer() {
        // Não afirmamos QUAL estado (a máquina de quem roda pode ter o Deployer instalado);
        // afirmamos que a função retorna, sem panicar e sem `Result`.
        let e = descobrir_app("schematize-deployer");
        assert!(matches!(e, Estado::Instalado { .. } | Estado::Ausente | Estado::Quebrado { .. }));
    }
}
