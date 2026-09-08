//! SELF-UPDATE — o market trocando o próprio binário, enquanto ele roda.
//!
//! **O quê:** [`atualizar_a_si_mesmo`] obtém uma versão nova deste programa (asset
//! pré-compilado quando há, senão compilando do fonte) e a coloca no lugar do binário atual,
//! por [`trocar_com_rede`].
//!
//! **Onde:** [`super::atualizar_componente`], quando o item do plano é o próprio market.
//!
//! ## Por que este módulo existe separado (D3)
//!
//! O `schematize_updater_rs` **nunca precisou disto**: ele atualizava o app *de fora*, e o
//! próprio binário só era reconstruído no fim de um build do fonte, como um alvo qualquer.
//! O market herdou o papel de gestor e com ele um modo de falha **novo** — e é o único do
//! plano capaz de deixar a máquina **sem gestor de pacotes nenhum**. Sem gestor, a pessoa não
//! tem por onde consertar: o comando que instalaria o conserto é justamente o que quebrou.
//!
//! ## A rede de segurança, em três camadas
//!
//! 1. **Nunca escrever por cima do binário em execução.** Grava-se `<bin>.novo` AO LADO e
//!    troca-se por `rename(2)`. No Linux o inode aberto sobrevive: o processo em curso termina
//!    íntegro e a próxima execução já é a nova. A alternativa — `install -m755` por cima, como
//!    o `install.sh` faz — funciona para um binário que não está rodando; para o que está
//!    executando o próprio comando, é o caminho para um binário truncado no meio de uma
//!    escrita interrompida.
//! 2. **Verificar o candidato ANTES da troca.** O binário novo tem de responder `--version`
//!    com a versão esperada. Se não responder, a troca não acontece e o antigo continua
//!    valendo. Um asset baixado com HTTP 200 pode não executar aqui (glibc, arquitetura) — e
//!    descobrir isso *depois* de substituir é descobrir tarde demais.
//! 3. **Verificar a CÓPIA, não só a origem.** A verificação roda sobre o `<bin>.novo` já
//!    gravado. Uma cópia truncada por disco cheio passa por qualquer checagem feita na origem.
//!
//! ## O que esta rede NÃO cobre, dito com todas as letras
//!
//! Se a máquina perder energia entre o `rename` e o `fsync` do diretório, o filesystem decide.
//! `rename(2)` é atômico quanto à visibilidade do nome — não é uma transação com durabilidade
//! garantida sem `fsync`. Na prática isso significa: o nome aponta para o binário velho ou
//! para o novo, nunca para um pedaço dos dois. É o que importa aqui.

use super::binario;
use crate::nucleo::{plataforma, rede};
use std::path::{Path, PathBuf};

/// **O quê:** atualiza este programa para `versao_alvo` (ou a última publicada).
///
/// **Onde:** [`super::atualizar_componente`], quando o item é o market.
///
/// **A ordem:** varre temporários órfãos → tenta o asset pré-compilado → cai para o fonte.
/// A varredura vem primeiro de propósito: ver [`limpar_orfaos`].
pub fn atualizar_a_si_mesmo(versao_alvo: Option<&str>) -> Result<(), String> {
    let eu = std::env::current_exe()
        .map_err(|e| format!("não consegui descobrir o caminho deste binário: {e}"))?;
    limpar_orfaos(&eu);

    let alvo = versao_alvo.ok_or(
        "não sei qual versão do market buscar (rede/GitHub indisponível?) — \
         nada foi alterado, o binário atual continua valendo",
    )?;

    if let Some(asset) = plataforma::market_asset_name() {
        match baixar_candidato(&asset, alvo) {
            Ok(cand) => {
                let r = trocar_com_rede(&cand, &eu, alvo);
                let _ = std::fs::remove_file(&cand);
                r?;
                println!("✓ schematize-market atualizado para v{alvo} (binário pré-compilado).");
                return Ok(());
            }
            Err(e) => println!("→ asset do market indisponível ({e}) — compilando do fonte…"),
        }
    } else {
        println!("→ sem binário pré-compilado do market para esta plataforma — do fonte…");
    }

    compilar_e_trocar(&eu, alvo)
}

/// **O quê:** baixa o asset do market para um temporário AO LADO do binário atual.
///
/// **Onde:** [`atualizar_a_si_mesmo`].
///
/// **Por que ao lado, e não no `/tmp`:** o passo seguinte é um `rename(2)` para o lugar do
/// binário, e rename entre filesystems falha com `EXDEV`. Em muitas máquinas `/tmp` é um
/// `tmpfs` — outro filesystem. Baixar já no diretório de destino elimina a classe inteira.
fn baixar_candidato(asset: &str, alvo: &str) -> Result<PathBuf, String> {
    let eu = std::env::current_exe().map_err(|e| e.to_string())?;
    let dst = eu.with_file_name(format!("{}.baixado", nome_do_arquivo(&eu)));
    let _ = std::fs::remove_file(&dst);
    let url = rede::url_de_asset(plataforma::MARKET_REPO, &format!("v{alvo}"), asset);
    if !rede::download(&url, &dst) {
        let _ = std::fs::remove_file(&dst);
        return Err(format!("download falhou: {url}"));
    }
    binario::make_executable(&dst);
    Ok(dst)
}

/// **O quê:** compila este programa do fonte e troca. **Onde:** [`atualizar_a_si_mesmo`],
/// quando não há asset ou o asset não serve.
///
/// Compila para um caminho AO LADO do binário atual — nunca direto por cima —, e só então
/// entrega a [`trocar_com_rede`], que verifica antes de substituir.
fn compilar_e_trocar(eu: &Path, alvo: &str) -> Result<(), String> {
    let cargo = plataforma::ensure_toolchain()?;
    let cand = eu.with_file_name(format!("{}.compilado", nome_do_arquivo(eu)));
    let _ = std::fs::remove_file(&cand);
    let r = super::fonte::build_one(
        cargo.to_str().unwrap_or("cargo"),
        plataforma::MARKET_REPO,
        &[],
        None,
        &plataforma::market_bin_name(),
        &cand,
    )
    .and_then(|()| trocar_com_rede(&cand, eu, alvo));
    let _ = std::fs::remove_file(&cand);
    r?;
    println!("✓ schematize-market atualizado para v{alvo} (compilado do fonte).");
    Ok(())
}

/// **O quê:** substitui `destino` por `candidato`, com a rede de segurança do D3.
///
/// **Onde:** [`atualizar_a_si_mesmo`] e [`compilar_e_trocar`] — os dois únicos lugares onde um
/// binário substitui o binário EM EXECUÇÃO.
///
/// **A sequência, e por que ela é esta:**
/// 1. `candidato` e `destino` no MESMO filesystem — senão o `rename` falha com `EXDEV` e a
///    troca deixa de ser atômica. A mensagem diz o mount, porque "falhou" não é acionável.
/// 2. Copia para `<destino>.novo`, no diretório do destino.
/// 3. **Verifica a CÓPIA** — executa e confere a versão. É aqui que um candidato truncado,
///    vazio ou sem bit de execução é recusado.
/// 4. Só então o `rename`. Falhou qualquer passo, o `.novo` é apagado e o antigo continua
///    valendo, intacto.
///
/// **Nunca há um instante em que o destino não existe.** Não há `remove_file` no destino em
/// lugar nenhum deste caminho: o `rename` substitui em um passo.
pub fn trocar_com_rede(candidato: &Path, destino: &Path, esperada: &str) -> Result<(), String> {
    if !candidato.is_file() {
        return Err(format!("o candidato {} não existe", candidato.display()));
    }
    let tmp = binario::caminho_temporario(destino);
    verificar_mesmo_filesystem(&tmp, destino)?;

    let _ = std::fs::remove_file(&tmp);
    std::fs::copy(candidato, &tmp)
        .map_err(|e| format!("não consegui gravar {}: {e}", tmp.display()))?;
    binario::make_executable(&tmp);

    // A verificação é sobre a CÓPIA. Uma cópia truncada por disco cheio passa por qualquer
    // checagem feita na origem — e é o binário copiado que vai virar o gestor da máquina.
    if !binario::executa_e_reporta(&tmp, esperada) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "o binário novo não respondeu `--version` com v{esperada} — a troca NÃO foi feita \
             e o {} atual continua valendo",
            nome_do_arquivo(destino)
        ));
    }

    std::fs::rename(&tmp, destino).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("não consegui substituir {}: {e}", destino.display())
    })
}

/// **O quê:** os dois caminhos ficam no mesmo filesystem? `Err` com mensagem acionável se não.
///
/// **Onde:** [`trocar_com_rede`], antes de qualquer escrita.
///
/// **Por que é uma verificação e não uma suposição:** `<destino>.novo` está, por construção,
/// no diretório do destino — então hoje a resposta é sempre "sim". A verificação existe para
/// o dia em que alguém mudar [`binario::caminho_temporario`] para um `/tmp` "mais limpo": aí
/// o `rename` passa a falhar com `EXDEV` e a troca deixa de ser atômica, silenciosamente, num
/// caminho que só roda em máquina de usuário.
///
/// No Windows a checagem é pulada: lá o `rename` sobre arquivo em uso não funciona de todo
/// jeito, e [`binario::substitui_binario`] usa a estratégia do `.old`.
fn verificar_mesmo_filesystem(a: &Path, b: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // Compara o diretório de cada um: o próprio arquivo pode ainda não existir.
        let dev = |p: &Path| -> Option<u64> {
            let d = p.parent()?;
            std::fs::metadata(d).ok().map(|m| m.dev())
        };
        match (dev(a), dev(b)) {
            (Some(x), Some(y)) if x != y => Err(format!(
                "o temporário ({}) e o destino ({}) estão em sistemas de arquivos diferentes — \
                 a troca não seria atômica. Mova o temporário para o mesmo diretório do \
                 destino, ou monte os dois no mesmo filesystem.",
                a.display(),
                b.display()
            )),
            // Não conseguir ler o `dev` de um deles não é motivo para recusar: o `rename`
            // dirá. Recusar aqui bloquearia a atualização por causa de um `stat` que falhou.
            _ => Ok(()),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        Ok(())
    }
}

/// **O quê:** apaga temporários órfãos deixados por uma tentativa anterior.
///
/// **Onde:** o início de [`atualizar_a_si_mesmo`].
///
/// **Por que na PRÓXIMA execução e não por handler de sinal:** um `SIGINT` no meio da troca
/// mata o processo — nenhum `Drop`, nenhum `defer`, nenhum código nosso roda. `SIGKILL` e uma
/// queda de energia menos ainda. A única limpeza que funciona para os três é a que acontece na
/// execução seguinte, e ela é segura porque os nomes são conhecidos e ficam ao lado de um
/// binário nosso — nada de varrer diretório por padrão.
///
/// Não devolve erro: falhar em apagar lixo não pode impedir uma atualização.
fn limpar_orfaos(eu: &Path) {
    for sufixo in [".novo", ".baixado", ".compilado"] {
        let p = eu.with_file_name(format!("{}{sufixo}", nome_do_arquivo(eu)));
        if p.is_file() && std::fs::remove_file(&p).is_ok() {
            println!("→ removido temporário de uma tentativa anterior: {}", p.display());
        }
    }
}

/// **O quê:** o nome do arquivo de um caminho, ou um fallback estável.
/// **Onde:** as mensagens e a montagem dos temporários deste módulo.
fn nome_do_arquivo(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .map(String::from)
        .unwrap_or_else(plataforma::market_bin_name)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::nucleo::util;

    /// Sandbox com um "binário" velho que funciona, no papel do market em execução.
    fn sandbox(nome: &str) -> (PathBuf, PathBuf) {
        let d = std::env::temp_dir().join(format!("market-self-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let velho = d.join("schematize-market");
        std::fs::write(&velho, "#!/bin/sh\necho \"schematize-market 1.0.0\"\n").unwrap();
        binario::make_executable(&velho);
        (d, velho)
    }

    /// O binário velho ainda responde a versão dele? É a pergunta que todo teste hostil aqui
    /// faz depois de uma troca recusada.
    fn velho_intacto(p: &Path) -> bool {
        binario::executa_e_reporta(p, "1.0.0")
    }

    /// **O caminho feliz:** um candidato que executa e reporta a versão esperada substitui o
    /// antigo, e o `.novo` não fica para trás.
    #[test]
    fn candidato_bom_substitui_e_nao_deixa_lixo() {
        let (d, velho) = sandbox("bom");
        let cand = d.join("candidato");
        std::fs::write(&cand, "#!/bin/sh\necho \"schematize-market 2.0.0\"\n").unwrap();
        binario::make_executable(&cand);

        trocar_com_rede(&cand, &velho, "2.0.0").expect("o candidato bom tinha de passar");

        assert!(binario::executa_e_reporta(&velho, "2.0.0"), "o novo não ficou no lugar");
        assert!(!binario::caminho_temporario(&velho).exists(), "sobrou o `.novo`");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **A camada 2 do D3:** candidato que EXECUTA mas reporta a versão ERRADA não passa. É o
    /// asset da tag trocada — roda perfeitamente e instala outra coisa.
    #[test]
    fn candidato_com_versao_errada_nao_troca() {
        let (d, velho) = sandbox("versao");
        let cand = d.join("candidato");
        std::fs::write(&cand, "#!/bin/sh\necho \"schematize-market 0.0.1\"\n").unwrap();
        binario::make_executable(&cand);

        let e = trocar_com_rede(&cand, &velho, "2.0.0").unwrap_err();
        assert!(e.contains("2.0.0"), "a mensagem tem de dizer o que se esperava: {e}");
        assert!(e.contains("continua valendo"), "e que nada foi quebrado: {e}");
        assert!(velho_intacto(&velho), "o binário velho tinha de sobreviver");
        assert!(!binario::caminho_temporario(&velho).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **O TESTE HOSTIL do checklist — os casos que deixariam a máquina sem gestor.**
    ///
    /// Candidato truncado, vazio, e um que não consegue executar de jeito nenhum (interpretador
    /// inexistente — o análogo testável de um binário compilado para outra arquitetura, que
    /// baixa com HTTP 200 e morre no `exec`). Nos TRÊS o binário velho tem de continuar
    /// respondendo, e nenhum `.novo` pode ficar para trás.
    #[test]
    fn candidato_truncado_vazio_ou_inexecutavel_nunca_troca() {
        let casos: [(&str, &[u8]); 3] = [
            // Truncado: começa como um ELF e termina no meio.
            ("truncado", b"\x7fELF\x02\x01\x01\x00pela-metade"),
            // Vazio: zero bytes.
            ("vazio", b""),
            // Executável, mas o `exec` falha — interpretador que não existe.
            ("nao-executa", b"#!/nao/existe/interpretador\necho oi\n"),
        ];
        for (nome, conteudo) in casos {
            let (d, velho) = sandbox(nome);
            let cand = d.join("candidato");
            std::fs::write(&cand, conteudo).unwrap();
            binario::make_executable(&cand);

            let r = trocar_com_rede(&cand, &velho, "2.0.0");
            assert!(r.is_err(), "caso `{nome}`: a troca NÃO podia ter acontecido");
            assert!(
                velho_intacto(&velho),
                "caso `{nome}`: o gestor da máquina foi destruído — é o modo de falha do R1"
            );
            assert!(
                !binario::caminho_temporario(&velho).exists(),
                "caso `{nome}`: sobrou um `.novo` órfão"
            );
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    /// **O caso "sem bit de execução" é DIFERENTE dos três acima, e o comportamento é
    /// deliberado:** a troca ACONTECE.
    ///
    /// Um arquivo baixado nunca vem com bit de execução — é o estado normal de todo candidato
    /// que chega pela rede. A rede de segurança não é "recusar o que não tem o bit", é
    /// **`chmod` na cópia e verificar se ela executa**. Recusar aqui reprovaria todo download
    /// legítimo e deixaria a máquina sem nunca se atualizar, que é o mesmo destino por outro
    /// caminho.
    ///
    /// O que continua garantido: se o `chmod` falhar (filesystem `noexec`, permissão), a cópia
    /// não executa, [`binario::executa_e_reporta`] recusa, e o velho sobrevive — é o caso
    /// `nao-executa` acima. A permissão nunca é uma suposição; é sempre uma verificação.
    #[test]
    fn candidato_sem_bit_de_execucao_e_corrigido_e_nao_recusado() {
        let (d, velho) = sandbox("sem-permissao");
        let cand = d.join("candidato");
        std::fs::write(&cand, "#!/bin/sh\necho \"schematize-market 2.0.0\"\n").unwrap();
        util::definir_modo(&cand, 0o644); // como todo arquivo recém-baixado

        trocar_com_rede(&cand, &velho, "2.0.0")
            .expect("um candidato bom sem o bit tinha de ser corrigido, não recusado");
        assert!(binario::executa_e_reporta(&velho, "2.0.0"), "a troca não aconteceu");
        assert!(!binario::caminho_temporario(&velho).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Candidato que nem existe é recusado antes de qualquer escrita.
    #[test]
    fn candidato_inexistente_e_recusado_sem_escrever_nada() {
        let (d, velho) = sandbox("ausente");
        let r = trocar_com_rede(&d.join("nao-existe"), &velho, "2.0.0");
        assert!(r.is_err());
        assert!(velho_intacto(&velho));
        assert!(!binario::caminho_temporario(&velho).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **A camada 1:** o temporário fica no diretório do DESTINO, e a checagem de filesystem
    /// concorda. Se alguém mudar `caminho_temporario` para o `/tmp`, este teste é quem avisa —
    /// antes de o `EXDEV` aparecer na máquina de um usuário.
    #[test]
    fn o_temporario_e_o_destino_ficam_no_mesmo_filesystem() {
        let (d, velho) = sandbox("fs");
        let tmp = binario::caminho_temporario(&velho);
        assert_eq!(tmp.parent(), velho.parent(), "o `.novo` saiu do diretório do destino");
        verificar_mesmo_filesystem(&tmp, &velho).expect("mesmo diretório é o mesmo filesystem");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// E a checagem RECUSA, com mensagem acionável, quando de fato são mounts diferentes.
    ///
    /// `/proc` é um filesystem próprio em todo Linux — serve de segundo mount sem precisar de
    /// privilégio. Onde ele não existir (macOS), o teste não tem o que afirmar e sai.
    #[test]
    fn filesystems_diferentes_dao_erro_acionavel() {
        use std::os::unix::fs::MetadataExt;
        let (d, velho) = sandbox("exdev");
        let outro = Path::new("/proc/self/x");
        let (Ok(a), Ok(b)) = (std::fs::metadata(&d), std::fs::metadata("/proc")) else {
            let _ = std::fs::remove_dir_all(&d);
            return;
        };
        if a.dev() == b.dev() {
            let _ = std::fs::remove_dir_all(&d);
            return; // sem dois mounts distintos, não há o que provar aqui
        }
        let e = verificar_mesmo_filesystem(outro, &velho).unwrap_err();
        assert!(e.contains("sistemas de arquivos diferentes"), "{e}");
        assert!(e.contains("mesmo diretório"), "a mensagem tem de dizer o que fazer: {e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **Interrupção no meio (SIGINT/SIGKILL/queda de energia):** o `.novo` que ficou é varrido
    /// na execução SEGUINTE — nenhum handler de sinal roda numa morte súbita, então a limpeza
    /// que funciona é a da próxima vez.
    #[test]
    fn temporario_orfao_e_varrido_na_execucao_seguinte() {
        let (d, velho) = sandbox("orfao");
        // Simula as três formas de sobra possíveis.
        for sufixo in [".novo", ".baixado", ".compilado"] {
            std::fs::write(d.join(format!("schematize-market{sufixo}")), b"lixo").unwrap();
        }
        limpar_orfaos(&velho);
        for sufixo in [".novo", ".baixado", ".compilado"] {
            assert!(
                !d.join(format!("schematize-market{sufixo}")).exists(),
                "o órfão `{sufixo}` sobreviveu — acumularia um binário por interrupção"
            );
        }
        assert!(velho_intacto(&velho), "a varredura não pode tocar no binário de verdade");
        let _ = std::fs::remove_dir_all(&d);
    }
}
