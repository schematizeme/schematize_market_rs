# schematize market

Instala e remove programas. Só isso, e bem.

```
schematize-market list                          # tudo que dá pra instalar, e o que já está
schematize-market install go --method mise
schematize-market install schematize-deployer
schematize-market switch rust --to official
schematize-market desktop --install             # ícone no menu de aplicativos

schematize-market update                        # atualiza TUDO (app, GUI, apps, e ele mesmo)
schematize-market update --dry-run              # o plano, sem mexer em nada
schematize-market status                        # versões, plataforma, pin, o que está instalado
schematize-market pin 0.62.0                    # fixa o app numa versão; `pin latest` desafixa
schematize-market unpin                         # volta a seguir a última publicada
schematize-market run                           # abre a GUI pelo caminho absoluto
schematize-market remove node                   # remove um runtime (detecta como foi instalado)
```

## Ele é o dono de INSTALAR e ATUALIZAR (ADR-0013)

Havia três programas que sabiam instalar — este, o `install.sh` e o `schematize-updater` —
cada um do seu jeito, e nenhum deles dono. O `market install schematize-deployer` chegava a
disparar um `curl | bash` do `install.sh`, enquanto o updater já sabia atualizar o deployer.
Dois caminhos para a mesma coisa, com comportamentos diferentes.

O `schematize-updater` foi **absorvido por este app** e deixou de existir. O `install.sh` ficou
sendo só o que ele já era de verdade: o bootstrap de primeira vez, que instala o market e
delega o resto. Quem tinha o comando do updater na mão troca só o nome do programa — os verbos
são os mesmos (`update`, `status`, `pin`, `unpin`, `run`).

### O que o `update` faz, e o que ele NÃO faz

**Faz:** o app (CLI + GUI) pela versão-alvo — binário pronto quando há para a plataforma,
compilando do fonte quando não —, mais cada app do ecossistema **que já esteja instalado**, e
o próprio market.

**Não faz:** instalar o que ninguém pediu. Um app ausente aparece no `--dry-run` como
*"não instalado → ignorado"*, com o motivo. Quem roda `update` quer o que já tem, mais novo —
não software novo aparecendo no `~/.cargo/bin` porque a casa lançou outro produto.

### Como ele troca o próprio binário sem se quebrar

Trocar o executável que está rodando o comando é o único modo de falha capaz de deixar a
máquina **sem gestor de pacotes nenhum** — e sem gestor não há por onde consertar. A rede tem
três camadas: escreve `<bin>.novo` **ao lado** e troca por `rename(2)` atômico (nunca por cima
do binário em execução); **verifica a CÓPIA já gravada** com `--version` antes de trocar (uma
cópia truncada por disco cheio passa por qualquer checagem feita na origem); e varre
temporários órfãos na execução seguinte — num `SIGKILL` nenhum handler nosso roda, então a
única limpeza que funciona é a da próxima vez.

Se qualquer passo falhar, a troca **não acontece** e o binário antigo continua valendo.

## O que ele instala

| | |
|---|---|
| **runtimes** | Go, Rust, Node, Python, Ruby, C#… por `docker`, `mise`, `distro` ou `official` |
| **ferramentas de dev** | Claude Code, VS Code, Codex CLI |
| **apps do ecossistema** | `schematize-deployer`, `schematize-optimizer` (compilados do fonte por ele mesmo) |
| **o app e ele próprio** | `schematize`, `schematize-gui` e o `schematize-market` — via `update` |

**Uma lista só.** O pedido que originou este app foi *"deployer e optimizer deveriam aparecer
no env para instalar"*. A leitura rasa seria somar os apps à tabela do `env`. A leitura certa
foi ver que `env` (linguagens) e `apps` (apps da casa) eram o **mesmo produto** partido em dois
comandos do hub — e promover os dois juntos a app próprio (ADR-0012).

## Ele diz por onde a coisa entrou

Um runtime instalado não vira um "instalado" mudo. O market pergunta ao gerenciador de pacotes
quem é dono do arquivo (`rpm -qf`, `dpkg -S`) e mostra a procedência:

```
  Rust           docker, mise, distro, official     via distro (cargo1.97)
  Node.js        docker, mise, distro, official     via mise
```

É o que torna o `switch` possível: sem saber por onde está instalado, trocar de método é
adivinhação. O `switch` **instala o novo antes de remover o velho**, e **recusa** quando a
procedência é desconhecida — desinstalar o que não se sabe de onde veio é como se perde uma
máquina.

## Distros

Compatibilidade de primeira classe com **Debian/Ubuntu, Fedora, SUSE e Arch** (e derivados),
detectadas por `/etc/os-release`. Quando um pacote não está nos repositórios padrão daquela
família, o market **diz isso e pergunta** em vez de inventar um repositório de terceiro — não
há tentativa de prever 999 distros, há honestidade sobre o que se sabe.

## Independência

Abre e funciona sozinho, sem o hub instalado (piso 10). Tem binário próprio, ícone próprio e
entrada própria no menu de aplicativos. Quando o hub está presente, os dois se enxergam.

## Desenvolvimento

```
cargo test --all-targets      # 58 testes
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

A superfície da CLI é contrato: `tests/superficie-cli.txt` reprova qualquer mudança acidental
em comando ou flag. Se a mudança for intencional,
`MARKET_REGRAVA_SUPERFICIE=1 cargo test superficie_da_cli` e **leia o diff linha por linha**.

## Licença

MIT — ver [LICENSE](LICENSE).
